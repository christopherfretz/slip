//! Ownership under rules (`spec/verify.md` §2.1 "Rules rename owned
//! entities only", `spec/digest.md` §3.4 "Ownership"). Work package
//! WP-110.
//!
//! Under a non-empty rule table the walk is given [`ReadOnly`], a
//! predicate over the image's files. An entity is a **dependency entity**
//! if one of its declarations lies in a read-only file (by its expansion
//! location; no location in a file is read-only), if it is a
//! specialisation, or an implicit deduction guide, of a dependency
//! template, or if it is an implicitly declared member of a dependency
//! class; otherwise it is **owned** ([`Walk::owned`], once per entity).
//! A rule matches an owned entity only (`Walk::rule_applies`, rule 1),
//! and the namespaces around a member are keyed from the member's point
//! of view (`Walk::namespace_parent` in `keys.rs`, rule 2).
//!
//! Under an empty table nothing here is read: [`Walk::rule_applies`] is
//! `false` before it asks, and `namespace_parent` is `unit_parent`.

use std::collections::{HashMap, HashSet};

use slip_kiln::kiln::ast::DeclId;
use slip_kiln::kiln::ast::decl::{DeclFlags, DeclKind, TemplLinkKind};
use slip_kiln::kiln::ast::names::DeclNameView;
use slip_kiln::kiln::source::{FileId, SourceManager};
use slip_model::transform::Ownership;

use crate::keys::KeyRes;
use crate::units::REDECL_LIMIT;
use crate::walk::Walk;

/// Which files of the image are read-only (`spec/edits.md` §2.1), for the
/// walk under a non-empty rule table: `true` for a file the run may not
/// edit.
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

    /// Whether `file` is read-only.
    pub fn read_only(&self, file: FileId) -> bool {
        match self.0 {
            Inner::Every => true,
            Inner::NoFile => false,
            Inner::Files(f) => f.read_only(file),
            Inner::Fn(f) => f(file),
        }
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
    read_only: Vec<bool>,
}

impl ReadOnlyFiles {
    /// A buffer is read-only unless `files` is given, the image maps it to
    /// a subject path (`subject_path`, the walk's `Input::subject_path`:
    /// empty for a buffer that is no file), and the path is owned
    /// (`Ownership::is_owned`: some entry covers it and the longest is in
    /// `owned`; a path that is not absolute is covered by none).
    pub fn new(
        files: Option<&Ownership>,
        sm: &SourceManager,
        subject_path: &dyn Fn(FileId) -> Vec<u8>,
    ) -> ReadOnlyFiles {
        let read_only = sm
            .file_ids()
            .map(|f| match files {
                None => true,
                Some(o) => {
                    let path = subject_path(f);
                    path.is_empty() || !o.is_owned(&path)
                }
            })
            .collect();
        ReadOnlyFiles { read_only }
    }

    /// Whether `file` is read-only; a buffer made after [`Self::new`] is.
    pub fn read_only(&self, file: FileId) -> bool {
        self.read_only.get(file.index()).copied().unwrap_or(true)
    }
}

/// The walk's ownership state: entities by ownership, and the keys of
/// namespaces seen from a member of the other ownership (rule 2).
#[derive(Default)]
pub(crate) struct OwnedState {
    memo: HashMap<DeclId, bool>,
    in_progress: HashSet<DeclId>,
    /// Files by whether they are read-only, as `ReadOnly` answered.
    files: HashMap<FileId, bool>,
    /// Cycles met (an entity asked again while its ownership is being
    /// decided): what is decided while one is met is not memoised.
    cycles: u64,
    /// The key of a namespace (its entity) under the view `.1` (`true`:
    /// owned, rules apply along the chain) when that is not the
    /// namespace's own ownership.
    pub views: HashMap<(DeclId, bool), KeyRes>,
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
        debug_assert!(!self.rules.is_empty(), "ownership is not consulted under no rule");
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
        for d in chain {
            if !self.in_owned_file(d) {
                return false;
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
        true
    }

    /// The template an implicit deduction guide (or the function template
    /// of one) is made from; `None` for any other entity.
    fn implicit_guide_template(&self, e: DeclId) -> Option<DeclId> {
        let ast = self.ast;
        let guide = match ast.decls.get(e).kind() {
            DeclKind::CXXDeductionGuide => e,
            DeclKind::FunctionTemplate => ast.template_info(e).and_then(|i| i.pattern)?,
            _ => return None,
        };
        let n = ast.decls.get(guide);
        if n.kind() != DeclKind::CXXDeductionGuide || !n.has(DeclFlags::IMPLICIT) {
            return None;
        }
        match ast.names.view(n.name()) {
            DeclNameView::DeductionGuide(t) => Some(t),
            _ => None,
        }
    }

    /// Declaration `d` lies in a file the run owns: its expansion location
    /// (the outermost macro invocation that produced it, else where it is
    /// written) is in a buffer that is a file (not `<built-in>`, the
    /// command line or scratch space) and the file is not read-only.
    pub(crate) fn in_owned_file(&mut self, d: DeclId) -> bool {
        let loc = self.sm.expansion_loc(self.ast.decls.get(d).loc());
        if self.sm.is_written_in_scratch_space(loc) {
            return false;
        }
        let Some((file, _)) = self.sm.offset_of(loc) else { return false };
        if self.sm.file_name(file).starts_with('<') {
            return false;
        }
        if let Some(&r) = self.owned_state.files.get(&file) {
            return !r;
        }
        let r = self.read_only.read_only(file);
        self.owned_state.files.insert(file, r);
        !r
    }
}

#[cfg(test)]
mod tests {
    use slip_kiln::kiln::source::{FileId, SourceManager};
    use slip_model::transform::Ownership;

    use super::{ReadOnly, ReadOnlyFiles};

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
    }
}
