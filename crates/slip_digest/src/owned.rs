//! Ownership under rules (`spec/verify.md` §2.1 "Rules rename owned
//! entities only", `spec/digest.md` §3.4 "Ownership"). Work package
//! WP-110.
//!
//! Under a non-empty rule table the walk is given [`ReadOnly`], a
//! predicate over the image's files. An entity is a **dependency entity**
//! if one of its declarations lies in a read-only file (by its expansion
//! location; no location in a file is read-only; a file entered through
//! an include chain that passes through a read-only file is read-only)
//! or has its name token spelled in one (a dependency macro's body), if
//! it is a specialisation, or an implicit deduction guide, of a dependency
//! template, or if it is an implicitly declared member of a dependency
//! class; otherwise it is **owned** ([`Walk::owned`], once per entity).
//! A rule matches an owned entity only (`Walk::rule_applies`, rule 1),
//! and the namespaces around a member are keyed from the member's point
//! of view (`Walk::namespace_parent` in `keys.rs`, rule 2).
//!
//! Under an empty table nothing here is read: [`Walk::rule_applies`] is
//! `false` before it asks, and `namespace_parent` is `unit_parent`.
//!
//! **Rebuilt files** (WP-118, `spec/verify.md` §2.1). A file has one of
//! three classes ([`Class`]): owned, rebuilt (a file the build writes
//! again from source in the run), or read-only. For every question asked
//! here a rebuilt file answers as an owned one does, on its own path and
//! on an include chain, with one exception (R2.2): a declaration of an
//! entity with C language linkage that lies in a rebuilt file lies in a
//! read-only file. The chain of an inclusion has three answers
//! ([`Chain`]): read-only, or clear with the set of the `rebuilt` entries
//! on it; the set is accounting only (`rule_use.rs`, R6.4), so the
//! decisions read two states as before.

use std::collections::{BTreeSet, HashMap, HashSet};

use slip_kiln::kiln::ast::DeclId;
use slip_kiln::kiln::ast::decl::{DeclFlags, DeclKind, TemplLinkKind};
use slip_kiln::kiln::ast::names::DeclNameView;
use slip_kiln::kiln::source::{FileId, Loc, SourceManager};
use slip_model::transform::{FileClass, Ownership};

use crate::keys::{KeyRes, info};
use crate::units::REDECL_LIMIT;
use crate::walk::Walk;

/// The class of one file of the image (`spec/verify.md` §2.1, WP-118):
/// what the walk is told of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Class {
    /// The run owns it.
    Owned,
    /// The build writes it again from source in the run; the index of the
    /// longest entry that covers it in `Ownership.rebuilt`.
    Rebuilt(u32),
    /// Read-only: a dependency's.
    ReadOnly,
}

/// Which files of the image are read-only (`spec/edits.md` §2.1), for the
/// walk under a non-empty rule table: `true` for a file the run may not
/// edit. With [`ReadOnlyFiles`], each file's [`Class`] (WP-118).
///
/// A predicate over Kiln's file ids, made by the caller: the walk knows
/// neither the run's `Ownership` (in subject paths) nor how the image's
/// files map to subject paths (the verify job's roots, `spec/verify.md`
/// §5.1); the caller knows both, and [`ReadOnlyFiles`] is the one way
/// this crate offers to join them. Borrowed, so that `Options` stays
/// `Copy`; `Sync`, so that it stays `Sync` (a worker shares one across
/// its threads). The walk asks each file it reaches once
/// (`OwnedState::files`).
#[derive(Clone, Copy)]
pub struct ReadOnly<'a>(Inner<'a>);

#[derive(Clone, Copy)]
enum Inner<'a> {
    Every,
    NoFile,
    Files(&'a ReadOnlyFiles),
    Fn(&'a (dyn Fn(FileId) -> bool + Sync)),
}

impl<'a> ReadOnly<'a> {
    /// Every file is read-only: every entity is a dependency entity, no
    /// rule matches, and every key is the key of the empty table. What
    /// the walk is given when ownership is not known (the map phase,
    /// whose table is empty, and any caller without a run's files):
    /// knowing less never accepts more.
    pub const EVERY_FILE: ReadOnly<'static> = ReadOnly(Inner::Every);

    /// No file is read-only: every entity with a location in a file is
    /// owned (WP-103's behaviour, for tests that compare with it).
    pub const NO_FILE: ReadOnly<'static> = ReadOnly(Inner::NoFile);

    /// The files [`ReadOnlyFiles`] decided: the verify job's.
    pub fn files(files: &'a ReadOnlyFiles) -> ReadOnly<'a> {
        ReadOnly(Inner::Files(files))
    }

    /// Any predicate (a test's).
    pub fn new(f: &'a (dyn Fn(FileId) -> bool + Sync)) -> ReadOnly<'a> {
        ReadOnly(Inner::Fn(f))
    }

    /// The class of `file`: [`Self::EVERY_FILE`] answers read-only,
    /// [`Self::NO_FILE`] owned, a predicate owned or read-only; only
    /// [`ReadOnlyFiles`] answers rebuilt.
    pub fn class(&self, file: FileId) -> Class {
        match self.0 {
            Inner::Every => Class::ReadOnly,
            Inner::NoFile => Class::Owned,
            Inner::Files(f) => f.class(file),
            Inner::Fn(f) if f(file) => Class::ReadOnly,
            Inner::Fn(_) => Class::Owned,
        }
    }

    /// Whether `file` is read-only (its class is [`Class::ReadOnly`]).
    pub fn read_only(&self, file: FileId) -> bool {
        self.class(file) == Class::ReadOnly
    }
}

impl std::fmt::Debug for ReadOnly<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let what = match self.0 {
            Inner::Every => "EVERY_FILE",
            Inner::NoFile => "NO_FILE",
            Inner::Files(_) => "files",
            Inner::Fn(_) => "fn",
        };
        write!(f, "ReadOnly({what})")
    }
}

/// The read-only files of one image, decided once per buffer of its
/// source manager (`spec/verify.md` §2.1, §5.1): what the verify job
/// gives the walk of an image under a non-empty rule table, through
/// [`ReadOnly::files`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadOnlyFiles {
    /// By `FileId::index`.
    classes: Vec<Class>,
}

impl ReadOnlyFiles {
    /// A buffer is read-only unless `files` is given, the image maps it to
    /// a subject path (`subject_path`, the walk's `Input::subject_path`:
    /// empty for a buffer that is no file), and an entry covers the path
    /// (`Ownership::class_of`: the longest covering entry decides; a path
    /// that is not absolute is covered by none). Owned or rebuilt by the
    /// list of that entry (WP-118).
    pub fn new(
        files: Option<&Ownership>,
        sm: &SourceManager,
        subject_path: &dyn Fn(FileId) -> Vec<u8>,
    ) -> ReadOnlyFiles {
        let classes = sm
            .file_ids()
            .map(|f| match files {
                None => Class::ReadOnly,
                Some(o) => {
                    let path = subject_path(f);
                    if path.is_empty() {
                        return Class::ReadOnly;
                    }
                    match o.class_of(&path) {
                        (FileClass::Owned, _) => Class::Owned,
                        (FileClass::Rebuilt, Some(i)) => match u32::try_from(i) {
                            Ok(i) => Class::Rebuilt(i),
                            Err(_) => Class::ReadOnly,
                        },
                        (FileClass::Rebuilt | FileClass::ReadOnly, _) => Class::ReadOnly,
                    }
                }
            })
            .collect();
        ReadOnlyFiles { classes }
    }

    /// The class of `file`; a buffer made after [`Self::new`] is read-only.
    pub fn class(&self, file: FileId) -> Class {
        self.classes.get(file.index()).copied().unwrap_or(Class::ReadOnly)
    }

    /// Whether `file` is read-only; a buffer made after [`Self::new`] is.
    pub fn read_only(&self, file: FileId) -> bool {
        self.class(file) == Class::ReadOnly
    }
}

/// The include chain of one inclusion (`spec/verify.md` §2.1, WP-118
/// R3.1): a file on it is read-only by its own path (or it is longer than
/// `INCLUDE_CHAIN_LIMIT`), or none is, with the set of the `rebuilt`
/// entries that cover a file on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Chain {
    ReadOnly,
    Clear(SetId),
}

/// Where a file location is (WP-118): read-only (its file by its own path,
/// or its chain), or clear, with the set of the `rebuilt` entries of its
/// chain and of its own file, and its own file when that is rebuilt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum At {
    ReadOnly,
    Clear { set: SetId, own: Option<FileId> },
}

/// A set of indices into `Ownership.rebuilt`, interned
/// ([`Sets`]); `SetId(0)` is the empty set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct SetId(u32);

impl SetId {
    pub(crate) const EMPTY: SetId = SetId(0);
}

/// The sets of `rebuilt` entries the walk met, interned. With no rebuilt
/// file only the empty set exists.
pub(crate) struct Sets {
    list: Vec<Vec<u32>>,
    ids: HashMap<Vec<u32>, SetId>,
}

impl Default for Sets {
    fn default() -> Sets {
        Sets { list: vec![Vec::new()], ids: HashMap::from([(Vec::new(), SetId::EMPTY)]) }
    }
}

impl Sets {
    /// The entries of `s`, ascending.
    pub(crate) fn get(&self, s: SetId) -> &[u32] {
        self.list.get(s.0 as usize).map_or(&[], Vec::as_slice)
    }

    /// `s` with `i` added.
    fn with(&mut self, s: SetId, i: u32) -> SetId {
        let mut v = self.get(s).to_vec();
        match v.binary_search(&i) {
            Ok(_) => return s,
            Err(at) => v.insert(at, i),
        }
        self.intern(v)
    }

    /// The union of `a` and `b`.
    fn union(&mut self, a: SetId, b: SetId) -> SetId {
        if a == b || b == SetId::EMPTY {
            return a;
        }
        if a == SetId::EMPTY {
            return b;
        }
        let mut v: Vec<u32> = self.get(a).to_vec();
        v.extend_from_slice(self.get(b));
        v.sort_unstable();
        v.dedup();
        self.intern(v)
    }

    fn intern(&mut self, v: Vec<u32>) -> SetId {
        if let Some(&id) = self.ids.get(&v) {
            return id;
        }
        let id = SetId(u32::try_from(self.list.len()).unwrap_or(u32::MAX));
        self.list.push(v.clone());
        self.ids.insert(v, id);
        id
    }
}

/// The walk's ownership state: entities by ownership, and the keys of
/// namespaces seen from a member of the other ownership (rule 2).
#[derive(Default)]
pub(crate) struct OwnedState {
    memo: HashMap<DeclId, bool>,
    in_progress: HashSet<DeclId>,
    /// Files by class, as `ReadOnly` answered.
    files: HashMap<FileId, Class>,
    /// Inclusions, by the location of the `#include` that entered them:
    /// their include chain (WP-118: read-only, or clear with its set).
    inclusions: HashMap<Loc, Chain>,
    /// The sets of `rebuilt` entries of the chains (WP-118).
    pub(crate) sets: Sets,
    /// The read-only files that cut a chain (WP-118 R1.3 rule 3): the
    /// first read-only file met above an inclusion of a file that is not
    /// read-only by its own path. For the hints.
    pub(crate) cutters: BTreeSet<FileId>,
    /// Cycles met (an entity asked again while its ownership is being
    /// decided): what is decided while one is met is not memoised.
    cycles: u64,
    /// Set by `records::facts` (`slip declare`), which asks ownership
    /// after a walk under the empty table: ownership does not depend on
    /// the table, and that walk's keys are not read for it.
    pub query: bool,
    /// The key of a namespace (its entity) under the view `.1` (`true`:
    /// owned, rules apply along the chain) when that is not the
    /// namespace's own ownership.
    pub views: HashMap<(DeclId, bool), KeyRes>,
    /// Set when the walk forms its lists of ownership (`own_lists.rs`,
    /// WP-146): `ns_parents` and `keyed` are kept.
    pub record_parents: bool,
    /// The namespace (its entity) each entity (its entity) was keyed
    /// under, in any view (`Walk::namespace_parent`): the chain of
    /// namespaces `own_lists.rs` climbs. Kept only with `record_parents`.
    pub ns_parents: HashMap<DeclId, DeclId>,
    /// Each entity whose own key `Walk::keys` memoises: whether the key
    /// was formed under the rules (`Walk::rule_applies` as `Walk::key_in`
    /// asked it: owned), and the key before the table. Kept only with
    /// `record_parents`, beside the memo, so that the lists read the
    /// ownership the key was formed under, not `Walk::owned` again
    /// (WP-146 ruling 2, F5).
    pub keyed: HashMap<DeclId, (bool, slip_model::keys::Key)>,
}

impl Walk<'_> {
    /// Whether a rule may apply to the key of `d`'s entity (rule 1 of
    /// `spec/verify.md` §2.1): the table is not empty and the entity is
    /// owned; for a namespace keyed in a member's view, the view.
    pub(crate) fn rule_applies(&mut self, d: DeclId, view: Option<bool>) -> bool {
        if self.rules.is_empty() {
            return false;
        }
        match view {
            Some(b) => b,
            None => self.owned(d),
        }
    }

    /// `d`'s entity is owned (`spec/verify.md` §2.1, the three bullets).
    /// Once per entity. A cycle is a dependency entity, not memoised.
    pub(crate) fn owned(&mut self, d: DeclId) -> bool {
        debug_assert!(
            !self.rules.is_empty() || self.owned_state.query,
            "ownership is not consulted under no rule"
        );
        let e = self.entity(d);
        if let Some(&o) = self.owned_state.memo.get(&e) {
            return o;
        }
        if !self.owned_state.in_progress.insert(e) {
            self.owned_state.cycles += 1;
            return false;
        }
        let cycles = self.owned_state.cycles;
        let o = self.decide_owned(e);
        self.owned_state.in_progress.remove(&e);
        if self.owned_state.cycles == cycles {
            self.owned_state.memo.insert(e, o);
        }
        o
    }

    fn decide_owned(&mut self, e: DeclId) -> bool {
        let ast = self.ast;
        // Bullet 1: every declaration, by its expansion location.
        let chain = self.chain(e);
        if chain.len() > REDECL_LIMIT {
            return false;
        }
        // WP-118 R6.4, with the accounting on: R(e), the `rebuilt` entries
        // on the declarations' chains and of their own files; D(e), the
        // rebuilt files a declaration lies in.
        let counting = self.rule_use.is_some();
        let mut through = SetId::EMPTY;
        let mut lies: Vec<FileId> = Vec::new();
        for d in chain {
            if !self.in_owned_file(d) {
                return false;
            }
            if counting {
                for at in self.ats_of(d).into_iter().flatten() {
                    if let At::Clear { set, own } = at {
                        through = self.owned_state.sets.union(through, set);
                        lies.extend(own);
                    }
                }
            }
        }
        // Bullet 2: a specialisation (explicit, partial or implicit) of a
        // dependency template, read where `compute_key` reads its
        // template; an implicit deduction guide of one.
        let r = self.representative(e);
        let template = match ast.templ_link(r).get() {
            Some((TemplLinkKind::Specialization, _)) => ast.spec_info(r).map(|(_, si)| si.template),
            Some((TemplLinkKind::PartialSpecialization, _)) => {
                ast.partial_spec_info(r).map(|(_, pi)| pi.template)
            }
            Some((TemplLinkKind::Pattern | TemplLinkKind::MemberOfSpecialization, _)) | None => {
                None
            }
        };
        if let Some(t) = template
            && !self.owned(t)
        {
            return false;
        }
        if let Some(t) = self.implicit_guide_template(e)
            && !self.owned(t)
        {
            return false;
        }
        // Bullet 3: an implicitly declared member of a dependency class.
        if ast.decls.get(r).has(DeclFlags::IMPLICIT)
            && let Some(class) = ast.decls.get(r).ctx()
            && ast.decls.record(class).is_some()
            && !self.owned(class)
        {
            return false;
        }
        if through != SetId::EMPTY || !lies.is_empty() {
            lies.sort_unstable();
            lies.dedup();
            let set = self.owned_state.sets.get(through).to_vec();
            crate::rule_use::through(self, e, set, lies);
        }
        true
    }

    /// The template an implicit deduction guide (or the function template
    /// of one) is made from; `None` for any other entity.
    fn implicit_guide_template(&self, e: DeclId) -> Option<DeclId> {
        self.guide(e)
            .filter(|&g| self.ast.decls.get(g).has(DeclFlags::IMPLICIT))
            .and_then(|g| self.template_of_guide(g))
    }

    /// The template a deduction guide (or the function template of one),
    /// implicit or written, deduces; `None` for any other entity.
    pub(crate) fn guide_template(&self, e: DeclId) -> Option<DeclId> {
        self.guide(e).and_then(|g| self.template_of_guide(g))
    }

    /// `e` as a deduction guide: itself, or the pattern of a function
    /// template that is one.
    fn guide(&self, e: DeclId) -> Option<DeclId> {
        let ast = self.ast;
        let guide = match ast.decls.get(e).kind() {
            DeclKind::CXXDeductionGuide => e,
            DeclKind::FunctionTemplate => ast.template_info(e).and_then(|i| i.pattern)?,
            _ => return None,
        };
        (ast.decls.get(guide).kind() == DeclKind::CXXDeductionGuide).then_some(guide)
    }

    fn template_of_guide(&self, guide: DeclId) -> Option<DeclId> {
        match self.ast.names.view(self.ast.decls.get(guide).name()) {
            DeclNameView::DeductionGuide(t) => Some(t),
            _ => None,
        }
    }

    /// `d`'s entity is a dependency entity only by the per-inclusion rule
    /// (WP-111 review F2, `spec/verify.md` §9.2): it is not owned, yet
    /// every declaration is in a file (its expansion location; a name
    /// token spelled in a file, in that file too) that is not read-only by
    /// its own path. So some declaration was entered through an include
    /// chain that passes through a read-only file. Asked only to count a
    /// namespace rule's use.
    pub(crate) fn only_by_inclusion(&mut self, d: DeclId) -> bool {
        let e = self.entity(d);
        if self.owned(e) {
            return false;
        }
        let chain = self.chain(e);
        if chain.len() > REDECL_LIMIT {
            return false;
        }
        for d in chain {
            let loc = self.ast.decls.get(d).loc();
            match self.buffer_of(self.sm.expansion_loc(loc)) {
                Some(f) if !self.file_read_only(f) => {}
                _ => return false,
            }
            if self.has_name_token(d)
                && let Some(f) = self.buffer_of(self.sm.spelling_loc(loc))
                && self.file_read_only(f)
            {
                return false;
            }
            // WP-118 R2.2: a C-linkage declaration lying in a rebuilt file
            // is read-only by its own path.
            if self.c_linkage_in_rebuilt(d) {
                return false;
            }
        }
        true
    }

    /// Declaration `d` lies in a file the run owns (`spec/verify.md`
    /// §2.1, bullet 1): its expansion location (the outermost macro
    /// invocation that produced it, else where it is written) is in an
    /// owned file, and its name token, if it has one, is not spelled in a
    /// read-only file (a dependency macro's body that spells the name). A
    /// name spelled in no file (made by `##`, in scratch space) is decided
    /// by the expansion location alone.
    ///
    /// A rebuilt file answers as an owned one (WP-118), except that a
    /// declaration of an entity with C language linkage that lies in a
    /// rebuilt file (its expansion location's file, or its name token's
    /// spelling file, is rebuilt by its own path) lies in a read-only
    /// file (R2.2, per declaration).
    pub(crate) fn in_owned_file(&mut self, d: DeclId) -> bool {
        let [expansion, spelling] = self.ats_of(d);
        if !matches!(expansion, Some(At::Clear { .. })) || spelling == Some(At::ReadOnly) {
            return false;
        }
        let lies =
            [expansion, spelling].iter().any(|a| matches!(a, Some(At::Clear { own: Some(_), .. })));
        !(lies && self.c_linkage_entity(d))
    }

    /// Where `d` is: its expansion location, and its name token's
    /// spelling location when it has a name token (`None` when it has
    /// none, or the location is in no file).
    pub(crate) fn ats_of(&mut self, d: DeclId) -> [Option<At>; 2] {
        let loc = self.ast.decls.get(d).loc();
        let expansion = self.read_only_at(self.sm.expansion_loc(loc));
        let spelling = if self.has_name_token(d) {
            self.read_only_at(self.sm.spelling_loc(loc))
        } else {
            None
        };
        [expansion, spelling]
    }

    /// `d`'s entity has C language linkage (`Walk::in_c_linkage`, gated
    /// by the kind's `c_linkage`, as the key's parent is decided).
    fn c_linkage_entity(&self, d: DeclId) -> bool {
        let e = self.entity(d);
        info(self.ast.decls.get(e).kind()).c_linkage && self.in_c_linkage(e)
    }

    /// WP-118 R2.2: `d` declares an entity with C language linkage and
    /// lies in a rebuilt file by its own path (its expansion location's
    /// file, or its name token's spelling file).
    pub(crate) fn c_linkage_in_rebuilt(&mut self, d: DeclId) -> bool {
        let loc = self.ast.decls.get(d).loc();
        let mut files = vec![self.buffer_of(self.sm.expansion_loc(loc))];
        if self.has_name_token(d) {
            files.push(self.buffer_of(self.sm.spelling_loc(loc)));
        }
        let lies =
            files.into_iter().flatten().any(|f| matches!(self.file_class(f), Class::Rebuilt(_)));
        lies && self.c_linkage_entity(d)
    }

    /// Whether `d` has a name token of its own: not an implicit
    /// declaration, and its name is not empty.
    fn has_name_token(&self, d: DeclId) -> bool {
        let n = self.ast.decls.get(d);
        if n.has(DeclFlags::IMPLICIT) {
            return false;
        }
        match self.ast.names.view(n.name()) {
            DeclNameView::Empty => false,
            DeclNameView::Identifier(_)
            | DeclNameView::Operator(_)
            | DeclNameView::Constructor(_)
            | DeclNameView::Destructor(_)
            | DeclNameView::Conversion(_)
            | DeclNameView::LiteralOperator(_)
            | DeclNameView::DeductionGuide(_) => true,
        }
    }

    /// For a file location: `None` if it is in no file (invalid, scratch
    /// space, a buffer whose name begins with `<`); else read-only if its
    /// file is read-only or it was entered through an include chain that
    /// passes through a read-only file, and clear otherwise, with the set
    /// of the `rebuilt` entries of its chain and of its own file, and its
    /// own file when that is rebuilt (WP-118 R3.1). Kiln has one buffer
    /// per file but one location range per inclusion
    /// (`SourceManager::add_inclusion`, `include_loc`), so this is decided
    /// per inclusion.
    pub(crate) fn read_only_at(&mut self, loc: Loc) -> Option<At> {
        let file = self.buffer_of(loc)?;
        let class = self.file_class(file);
        if class == Class::ReadOnly {
            return Some(At::ReadOnly);
        }
        match self.include_chain(self.sm.include_loc(loc)) {
            Chain::ReadOnly => Some(At::ReadOnly),
            Chain::Clear(set) => Some(match class {
                Class::Rebuilt(i) => {
                    At::Clear { set: self.owned_state.sets.with(set, i), own: Some(file) }
                }
                Class::Owned | Class::ReadOnly => At::Clear { set, own: None },
            }),
        }
    }

    /// The buffer of file location `loc`, if it is a file.
    pub(crate) fn buffer_of(&self, loc: Loc) -> Option<FileId> {
        if self.sm.is_written_in_scratch_space(loc) {
            return None;
        }
        let (file, _) = self.sm.offset_of(loc)?;
        if self.sm.file_name(file).starts_with('<') {
            return None;
        }
        Some(file)
    }

    /// Whether `file` is read-only by its own path (its class is
    /// [`Class::ReadOnly`]; a rebuilt file is not, WP-118).
    pub(crate) fn file_read_only(&mut self, file: FileId) -> bool {
        self.file_class(file) == Class::ReadOnly
    }

    /// The class of `file`, as `ReadOnly` answers, once per file.
    pub(crate) fn file_class(&mut self, file: FileId) -> Class {
        if let Some(&c) = self.owned_state.files.get(&file) {
            return c;
        }
        let c = self.read_only.class(file);
        self.owned_state.files.insert(file, c);
        c
    }

    /// The include chain of the inclusion entered by the `#include` at
    /// `inc` (WP-118 R3.1): read-only if a file on it (the file holding
    /// `inc`, or one further up) is read-only by its own path, else clear
    /// with the set of the `rebuilt` entries covering a file on it. An
    /// includer that is no file (the command line) decides nothing. The
    /// first read-only file met is recorded as a cutter (R1.3). Memoised
    /// per inclusion: each element of the path walked gets the union of
    /// the entries from itself upwards, or read-only for all when the walk
    /// ended read-only.
    fn include_chain(&mut self, inc: Loc) -> Chain {
        // Each `#include` location walked, with the entry of its file
        // when that is rebuilt.
        let mut path: Vec<(Loc, Option<u32>)> = Vec::new();
        let mut at = inc;
        let mut end = Chain::Clear(SetId::EMPTY);
        while at.is_valid() {
            if let Some(&c) = self.owned_state.inclusions.get(&at) {
                end = c;
                break;
            }
            let class = self.buffer_of(at).map(|f| (f, self.file_class(f)));
            match class {
                Some((f, Class::ReadOnly)) => {
                    path.push((at, None));
                    self.owned_state.cutters.insert(f);
                    end = Chain::ReadOnly;
                    break;
                }
                Some((_, Class::Rebuilt(i))) => path.push((at, Some(i))),
                Some((_, Class::Owned)) | None => path.push((at, None)),
            }
            // A chain longer than any the preprocessor allows is taken as
            // passing through a read-only file.
            if path.len() > INCLUDE_CHAIN_LIMIT {
                end = Chain::ReadOnly;
                break;
            }
            at = self.sm.include_loc(at);
        }
        let Chain::Clear(mut set) = end else {
            for (p, _) in path {
                self.owned_state.inclusions.insert(p, Chain::ReadOnly);
            }
            return Chain::ReadOnly;
        };
        for &(p, entry) in path.iter().rev() {
            if let Some(i) = entry {
                set = self.owned_state.sets.with(set, i);
            }
            self.owned_state.inclusions.insert(p, Chain::Clear(set));
        }
        Chain::Clear(set)
    }
}

/// Past this many inclusions an include chain is taken as passing
/// through a read-only file (the conservative answer; Kiln's include
/// depth limit is far below it).
const INCLUDE_CHAIN_LIMIT: usize = 4096;

#[cfg(test)]
mod tests {
    use slip_kiln::kiln::source::{FileId, SourceManager};
    use slip_model::transform::Ownership;

    use super::{Class, ReadOnly, ReadOnlyFiles};

    fn p(s: &str) -> Vec<u8> {
        s.as_bytes().to_vec()
    }

    /// Buffers named `names`, in order, and their ids.
    fn buffers(names: &[&str]) -> (SourceManager, Vec<FileId>) {
        let sm = SourceManager::new();
        let ids = names.iter().map(|n| sm.add_virtual(n, String::new()).unwrap()).collect();
        (sm, ids)
    }

    /// `spec/verify.md` §2.1, §5.1: the job's files, against an ownership
    /// in subject paths and an image whose files are under a root of its
    /// own (`/post/...` for subject root `/subject/...`), as the verify
    /// job maps a post-image back (the map here is the test's).
    #[test]
    fn read_only_files_maps_through_the_subject_path() {
        let files = Ownership {
            owned: vec![p("/subject")],
            readonly: vec![p("/subject/dep"), p("/subject/src/gen")],
            rebuilt: Vec::new(),
        };
        let names = [
            "/post/src/a.h",
            "/post/dep/d.h",
            "/post/src/gen/g.h",
            "<built-in>",
            "/elsewhere/x.h",
            "rel.h",
            "/post/src/generated.h",
        ];
        let (sm, ids) = buffers(&names);
        // The root map; a buffer that is no file has the empty path.
        let subject = |f: FileId| -> Vec<u8> {
            let name = sm.file_name(f);
            match name.strip_prefix("/post/") {
                Some(rest) => p(&format!("/subject/{rest}")),
                None if name.starts_with('<') => Vec::new(),
                None => p(name),
            }
        };
        let ro = ReadOnlyFiles::new(Some(&files), &sm, &subject);
        let got: Vec<bool> = ids.iter().map(|&f| ro.read_only(f)).collect();
        // a.h owned; dep read-only; gen read-only (the longest entry);
        // the built-in buffer, a file outside every entry, and a relative
        // path (covered by no entry) read-only; `generated.h` is not under
        // `/subject/src/gen` (`covers` is by component).
        assert_eq!(got, [false, true, true, true, true, true, false]);
        let r = ReadOnly::files(&ro);
        assert_eq!(ids.iter().map(|&f| r.read_only(f)).collect::<Vec<_>>(), got);
        // A buffer made later is read-only.
        let later = sm.add_virtual("/post/src/b.h", String::new()).unwrap();
        assert!(ro.read_only(later));
        // No files given: every file is read-only.
        let none = ReadOnlyFiles::new(None, &sm, &subject);
        assert!(ids.iter().all(|&f| none.read_only(f)));
    }

    /// `spec/verify.md` §2.1 (what the verify job must do): the subject
    /// path is the file's real path. `ReadOnlyFiles` decides on exactly
    /// the path it is given and normalises nothing, so a read-only file
    /// reached through a symbolic link under an owned root is read-only
    /// only when the job resolves the link. The link is modelled here
    /// (`/post/src/vendor` -> `/post/dep`); the job resolves real ones.
    #[test]
    fn read_only_files_needs_the_real_path() {
        let files = Ownership {
            owned: vec![p("/subject")],
            readonly: vec![p("/subject/dep")],
            rebuilt: Vec::new(),
        };
        let (sm, ids) = buffers(&["/post/dep/d.h", "/post/src/vendor/d.h", "/post/src/a.h"]);
        let spelled =
            |f: FileId| -> Vec<u8> { p(&sm.file_name(f).replacen("/post/", "/subject/", 1)) };
        let real = |f: FileId| -> Vec<u8> {
            let name = sm.file_name(f).replacen("/post/src/vendor/", "/post/dep/", 1);
            p(&name.replacen("/post/", "/subject/", 1))
        };
        let by_real = ReadOnlyFiles::new(Some(&files), &sm, &real);
        assert_eq!(
            ids.iter().map(|&f| by_real.read_only(f)).collect::<Vec<_>>(),
            [true, true, false]
        );
        // The spelling as the subject path: the same file, two answers.
        let by_spelling = ReadOnlyFiles::new(Some(&files), &sm, &spelled);
        assert_eq!(
            ids.iter().map(|&f| by_spelling.read_only(f)).collect::<Vec<_>>(),
            [true, false, false]
        );
    }

    /// WP-118 (`spec/verify.md` §2.1, A1, A1c, A2, A9): the class of each
    /// buffer over three lists, by the longest covering entry and the real
    /// path; a rebuilt file carries its entry's index; a buffer that is no
    /// file or made later is read-only.
    #[test]
    fn read_only_files_three_classes() {
        let files = Ownership {
            owned: vec![p("/s/src")],
            readonly: vec![p("/b"), p("/b/gen/_deps")],
            rebuilt: vec![p("/b/gen"), p("/s")],
        };
        let names = [
            "/post/s/src/a.h",      // owned (under the rebuilt /s: the longer owned entry, A9)
            "/post/s/gen/g.h",      // rebuilt, entry 1 (/s)
            "/b/gen/x.inc",         // rebuilt, entry 0 (under the read-only /b: A1c)
            "/b/gen/_deps/inc/d.h", // read-only (the longer read-only entry: A1)
            "/b/generated/y.h",     // read-only (/b/gen does not cover /b/generated)
            "/b/gen/link.h",        // a link to /opt/dep/d.h: read-only by its real path (A2)
            "/opt/dep/d.h",         // read-only (no entry)
            "<built-in>",           // no file
        ];
        let (sm, ids) = buffers(&names);
        let subject = |f: FileId| -> Vec<u8> {
            let name = sm.file_name(f);
            if name.starts_with('<') {
                return Vec::new();
            }
            let real = name.replacen("/b/gen/link.h", "/opt/dep/d.h", 1);
            p(real.strip_prefix("/post").unwrap_or(&real))
        };
        let ro = ReadOnlyFiles::new(Some(&files), &sm, &subject);
        let got: Vec<Class> = ids.iter().map(|&f| ro.class(f)).collect();
        use Class::{Owned, ReadOnly as R, Rebuilt};
        assert_eq!(got, [Owned, Rebuilt(1), Rebuilt(0), R, R, R, R, R]);
        let r = ReadOnly::files(&ro);
        assert_eq!(ids.iter().map(|&f| r.class(f)).collect::<Vec<_>>(), got);
        // read_only is the class ReadOnly: a rebuilt file is not.
        assert_eq!(
            ids.iter().map(|&f| ro.read_only(f)).collect::<Vec<_>>(),
            [false, false, false, true, true, true, true, true]
        );
        let later = sm.add_virtual("/post/s/gen/h.h", String::new()).unwrap();
        assert_eq!(ro.class(later), R);
        // The spelling as the subject path would make the link rebuilt (M14).
        let spelled = |f: FileId| -> Vec<u8> { p(sm.file_name(f)) };
        let by_spelling = ReadOnlyFiles::new(Some(&files), &sm, &spelled);
        assert_eq!(by_spelling.class(ids[5]), Rebuilt(0));
    }

    #[test]
    fn the_two_constants_and_a_predicate() {
        let (_sm, ids) = buffers(&["a", "b"]);
        assert!(ReadOnly::EVERY_FILE.read_only(ids[0]));
        assert!(!ReadOnly::NO_FILE.read_only(ids[0]));
        let second = ids[1];
        let f = move |f: FileId| f == second;
        let r = ReadOnly::new(&f);
        assert!(!r.read_only(ids[0]) && r.read_only(ids[1]));
        assert_eq!(format!("{:?}", ReadOnly::EVERY_FILE), "ReadOnly(EVERY_FILE)");
        // WP-118: the constants and a predicate answer two classes only.
        assert_eq!(ReadOnly::EVERY_FILE.class(ids[0]), Class::ReadOnly);
        assert_eq!(ReadOnly::NO_FILE.class(ids[0]), Class::Owned);
        assert_eq!((r.class(ids[0]), r.class(ids[1])), (Class::Owned, Class::ReadOnly));
    }
}
