//! A reference to a namespace under rules (`spec/digest.md` §3.4 "A
//! reference to a namespace"; WP-141, `spec/landed/digest/WP-141.md` and
//! the ruling's addenda in `docs/wp/WP-141.md`).
//!
//! WP-140 gives a namespace that owned code and a dependency both open a
//! unit under the key of each of its **views**. A reference to such a
//! namespace has to say which key it means. Two rules:
//!
//! - **Named to find members** (R1): the scope of a using-declaration in
//!   its key, the namespace qualifier of a name left unresolved (in a
//!   stream or in a profile), and the qualifier of a concept-id. The
//!   namespace is written with the key of the part of it that held each
//!   declaration the lookup found ([`Walk::write_ns_found`]): one key is
//!   `Ref::unit`, several `Ref::views`.
//! - **Named whole** (R2): the namespace a using-directive at namespace
//!   scope nominates, and the target of a namespace alias
//!   ([`Walk::write_ns_whole`]). In an owned file the unit is written
//!   with the reading of the namespace's own key, which is `Ref::unit(k)`
//!   only if the reference is **whole for `k`**: every namespace of
//!   `E(k)` (the namespaces that hold something under `k`) is named, by
//!   the alias, or by the using-directives of the same enclosing namespace
//!   entity in the same file taken together; else `Ref::part(k, view)`.
//!   In a read-only file there is one reading, never `part`.
//!
//! Everything here is order-independent: the table of namespaces
//! ([`Table`]) is built from the AST and from ownership, not from what
//! the walk has keyed so far, so a key or a content digest computed in a
//! fresh walk (`records::key_agreement`, `query::Keys`) is the one the
//! full walk gives.
//!
//! **The guards that keep every digest where it was without two views.**
//! Under the empty table, and under a table with no rule of kind
//! `namespace`, nothing here is read ([`Walk::ns_rules`]): every reference
//! is `write_ref`'s. R1 asks for a route only when the named namespace
//! has two view keys ([`Walk::ns_view_keys`]); with one it is `write_ref`'s
//! too. R2 is asked of every whole-namespace reference under a namespace
//! rule (a namespace with one view key can share it with another one's
//! view, C3), but a namespace alone under its key is whole, and its one
//! reading is the bytes `write_ref` writes (`tests/inert.rs`).

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use slip_kiln::kiln::ast::DeclId;
use slip_kiln::kiln::ast::decl::{DeclFlags, DeclKind, NamespaceData};
use slip_kiln::kiln::source::FileId;
use slip_model::Digest;
use slip_model::keys::KeyKind;

use crate::Cause;
use crate::keys::{KeyRes, Role, info};
use crate::owned::{At, Class};
use crate::refs::{REF_NONE, REF_UNIT};
use crate::stream::Stream;
use crate::units::REDECL_LIMIT;
use crate::walk::Walk;

/// `Ref::views`, then the count and the keys, ascending (digest §4.1).
pub(crate) const REF_VIEWS: u64 = 6;
/// `Ref::part`, then the key and the view (`true`: owned) (digest §4.1).
pub(crate) const REF_PART: u64 = 7;

/// A namespace's view key could not be formed where a reference needs it.
const NO_VIEW_KEY: Cause = Cause::Unaccounted("ref/namespace-view");

/// The bound on the namespaces a route through using-directives visits.
const ROUTE_LIMIT: usize = 4096;

/// What the walk keeps for this module: the table, made on first need,
/// and two counters.
#[derive(Default)]
pub(crate) struct NsRefState {
    table: Option<Arc<Table>>,
    by_key: Option<Arc<ByKey>>,
    /// `digest.ref.namespace_no_referent`: references named to find
    /// members that found nothing, written with every view key.
    pub no_referent: u64,
    /// `digest.ref.namespace_route_unknown`: found declarations whose
    /// route through the named namespace is not known (a using-declaration
    /// in it: Kiln keeps no shadow in a found set, addendum 2), each taken
    /// with every view key.
    pub route_unknown: u64,
    /// The using-directives and aliases whose written reading is
    /// `Ref::part`, with the view of the part, for their display name;
    /// filled by [`notes`].
    pub part: HashMap<DeclId, bool>,
    /// What the lookup found, for the next canonical qualifier written
    /// (`Walk::write_canonical_nns`): set by an R1 site just before it
    /// writes its qualifier, taken by that write.
    pub pending: Option<Vec<DeclId>>,
}

/// One namespace entity.
#[derive(Clone, Copy, Debug, Default)]
struct NsInfo {
    /// Its own ownership (`Walk::owned`).
    owned: bool,
    /// Some declaration of it is `inline`.
    inline: bool,
    /// Its enclosing namespace entity (`None`: the global namespace, or
    /// not at namespace scope).
    parent: Option<DeclId>,
    /// By view (`[dependency, owned]`): an entity of that ownership
    /// (`Walk::owned`, the entity's, not the file of one declaration)
    /// that is not a namespace is keyed with it as its namespace parent
    /// (`Walk::key_namespace`): a member, or a content-keyed using-directive,
    /// alias, using-declaration or `static_assert`.
    member: [bool; 2],
    /// By view: it holds something (A4; WP-140 review F3): a member of
    /// that view, or a nested namespace that holds something in that view,
    /// transitively. An opening alone holds nothing.
    holds: [bool; 2],
}

/// Where a using-directive (or a namespace) is, at namespace scope.
#[derive(Clone, Copy)]
enum NsScope {
    /// The global namespace.
    Global,
    /// The namespace entity.
    In(DeclId),
}

impl NsScope {
    /// The enclosing namespace entity; `None` for the global namespace.
    fn ns(self) -> Option<DeclId> {
        match self {
            NsScope::Global => None,
            NsScope::In(n) => Some(n),
        }
    }
}

impl NsInfo {
    /// The view exists: the namespace holds something in it
    /// ([`Walk::ns_holds`], the one predicate).
    fn exists(&self, view: bool) -> bool {
        self.holds[usize::from(view)]
    }
}

/// The namespaces of the image, and the using-directives at namespace
/// scope.
struct Table {
    info: HashMap<DeclId, NsInfo>,
    /// The namespace entities, ascending by id: the order `ByKey` is made
    /// in (any order gives the same sets).
    order: Vec<DeclId>,
    /// By (enclosing namespace entity, `None` for the global one; file of
    /// the directive, `None` for no file): the namespaces nominated there.
    named: HashMap<(Option<DeclId>, Option<FileId>), BTreeSet<DeclId>>,
    /// By enclosing namespace entity: the namespaces its using-directives
    /// nominate, each with the directive's ownership by path (`true`:
    /// owned).
    dirs: HashMap<DeclId, Vec<(DeclId, bool)>>,
    /// By namespace entity: its inline namespaces.
    inline_children: HashMap<DeclId, Vec<DeclId>>,
}

impl Table {
    /// `ns` and its inline namespace set, transitively ([namespace.def]).
    fn inline_set(&self, ns: DeclId) -> Vec<DeclId> {
        let mut out = vec![ns];
        let mut i = 0;
        while i < out.len() && out.len() < ROUTE_LIMIT {
            if let Some(c) = self.inline_children.get(&out[i]) {
                for &x in c {
                    if !out.contains(&x) {
                        out.push(x);
                    }
                }
            }
            i += 1;
        }
        out
    }

    /// Whether qualified lookup in `t` follows using-directives to
    /// namespace `c` ([namespace.qual]/2): `c` is in the inline set of `t`
    /// or of a namespace nominated, transitively, by the directives of
    /// one visited. `None`: the bound was reached.
    fn reaches(&self, t: DeclId, c: DeclId) -> Option<bool> {
        let mut seen: Vec<DeclId> = Vec::new();
        let mut stack = vec![t];
        while let Some(n) = stack.pop() {
            for m in self.inline_set(n) {
                if seen.contains(&m) {
                    continue;
                }
                if seen.len() >= ROUTE_LIMIT {
                    return None;
                }
                seen.push(m);
                if m == c {
                    return Some(true);
                }
                for &(x, _) in self.dirs.get(&m).into_iter().flatten() {
                    stack.push(x);
                }
            }
        }
        Some(false)
    }
}

/// `E(k)` for every `k`: the namespaces that hold something under it.
struct ByKey {
    map: HashMap<Digest, Vec<DeclId>>,
    /// A namespace that holds something in a view whose key cannot be
    /// formed: whether it is in some `E(k)` is not known.
    cause: Option<Cause>,
}

/// One reading of a whole-namespace reference that is not whole (the
/// notes of `spec/landed/digest/WP-141.md` §8.6), informational.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct NamespacePart {
    /// The key of the unit that holds the reference (a using-directive's,
    /// a namespace alias's).
    pub unit: Digest,
    /// The view key the reading is of.
    pub key: Digest,
    /// The reading is the one written (the namespace's own key); else it
    /// is the reading of the namespace's other view key.
    pub written: bool,
    /// For each namespace of `E(key)` the reference does not name: its
    /// name as a person would write it and the subject path of the file
    /// of its first opening (empty: in no file); ascending.
    pub missing: Vec<(String, Vec<u8>)>,
}

/// What a route gives one found declaration.
enum Route {
    /// The views of the part, or parts, of the named namespace it came
    /// through.
    Views(Vec<bool>),
    /// Not known: every view key.
    Unknown,
}

impl Walk<'_> {
    /// Whether a namespace may have a second key: the table has a rule of
    /// kind `namespace`. Without one every namespace has one key in every
    /// view and every key is one namespace's.
    pub(crate) fn ns_rules(&self) -> bool {
        !self.rules.is_empty() && self.rules.rules().iter().any(|r| r.kind == KeyKind::Namespace)
    }

    /// The key of namespace `ns` in `view`: its own key in its own view,
    /// else `Walk::view_key` (rule 2, from the point of view of a member
    /// of that ownership).
    fn ns_view_key(&mut self, ns: DeclId, view: bool) -> KeyRes {
        if self.owned(ns) == view { self.key(ns) } else { self.view_key(ns, view) }
    }

    /// R1: `ns` named to find members, `found` what the lookup found
    /// (`spec/digest.md` §3.4, addenda A1 and 2). With one view key (or
    /// under no namespace rule) the reference is `write_ref`'s. Else each
    /// found declaration gives the key of the part of `ns` that held it:
    /// a member of `ns` or of its inline namespace set, its own view; one
    /// reached through using-directives written there, the ownership of
    /// those directives; anything else (a using-declaration in `ns`, whose
    /// shadow Kiln does not keep) every view key, counted. Nothing found
    /// (an ill-formed name, [basic.lookup.qual]) takes every view key,
    /// counted. The keys sorted, duplicates removed: one is `Ref::unit`,
    /// two `Ref::views`.
    pub(crate) fn write_ns_found(&mut self, s: &mut Stream, ns: DeclId, found: &[DeclId]) {
        if crate::local::numbering_mode(self) {
            self.write_ref(s, ns);
            return;
        }
        let Some((ids, cause)) = self.ns_found_ids(ns, found, true) else {
            self.write_ref(s, ns);
            return;
        };
        if let [id] = ids.as_slice() {
            s.uv(REF_UNIT).digest(id);
        } else {
            s.uv(REF_VIEWS).count(ids.len());
            for id in &ids {
                s.digest(id);
            }
        }
        s.absorb(cause);
        let ns = self.entity(self.ast.resolve_alias(ns));
        crate::units::demand(self, ns);
    }

    /// The keys R1 writes for `ns` named to find `found`, ascending and
    /// without duplicates, and the cause of the own key; `None` where the
    /// reference is `write_ref`'s (no namespace rule, or one view key).
    /// `count`: the counters are the walk's (a query does not count).
    fn ns_found_ids(
        &mut self,
        ns: DeclId,
        found: &[DeclId],
        count: bool,
    ) -> Option<(Vec<Digest>, Option<Cause>)> {
        if !self.ns_rules() {
            return None;
        }
        let ns = self.entity(self.ast.resolve_alias(ns));
        let keys = self.ns_view_keys(ns);
        if keys.len() < 2 {
            return None;
        }
        let table = self.ns_table();
        let set = table.inline_set(ns);
        let mut views: Vec<bool> = Vec::new();
        let mut every = found.is_empty();
        if every && count {
            self.ns_refs.no_referent += 1;
        }
        for &f in found {
            match self.route(&table, &set, f) {
                Route::Views(v) => views.extend(v),
                Route::Unknown => {
                    if count {
                        self.ns_refs.route_unknown += 1;
                    }
                    every = true;
                }
            }
        }
        // Never empty: `keys` holds both views (two keys), every known
        // route gives one, and an empty found set takes every key.
        let mut ids: Vec<Digest> =
            keys.iter().filter(|(v, _)| every || views.contains(v)).map(|&(_, id)| id).collect();
        ids.sort();
        ids.dedup();
        Some((ids, keys_cause(self, ns)))
    }

    /// The route of found declaration `f` through `set`, the named
    /// namespace's inline set (addendum 2, in its order).
    fn route(&mut self, table: &Table, set: &[DeclId], f: DeclId) -> Route {
        let ast = self.ast;
        let e = self.entity(ast.strip_shadow(f));
        let Some(c) = self.enclosing_namespace(e) else { return Route::Unknown };
        if set.contains(&c) {
            return Route::Views(vec![self.owned(e)]);
        }
        let mut views = Vec::new();
        for &n in set {
            for &(t, own) in table.dirs.get(&n).into_iter().flatten() {
                match table.reaches(t, c) {
                    Some(true) => views.push(own),
                    Some(false) => {}
                    None => return Route::Unknown,
                }
            }
        }
        if views.is_empty() { Route::Unknown } else { Route::Views(views) }
    }

    /// The namespace entity `d` is a member of, through transparent
    /// contexts (a linkage specification, an export); `None` for the
    /// global namespace or any other scope.
    fn enclosing_namespace(&self, d: DeclId) -> Option<DeclId> {
        let ast = self.ast;
        let mut c = ast.decls.get(d).ctx();
        for _ in 0..ROUTE_LIMIT {
            let x = c?;
            match info(ast.decls.get(x).kind()).role {
                Role::Transparent => c = ast.decls.get(x).ctx(),
                Role::Entity(KeyKind::Namespace) => return Some(self.entity(x)),
                Role::Root
                | Role::Entity(_)
                | Role::Content(_)
                | Role::Nothing
                | Role::Local
                | Role::InParent
                | Role::Unsupported(_) => return None,
            }
        }
        None
    }

    /// R2: namespace `target` named whole by `d` (a using-directive at
    /// namespace scope, or a namespace alias) (`spec/digest.md` §3.4).
    pub(crate) fn write_ns_whole(&mut self, s: &mut Stream, d: DeclId, target: DeclId) {
        if !self.ns_rules() || crate::local::numbering_mode(self) {
            self.write_ref(s, target);
            return;
        }
        let ns = self.entity(self.ast.resolve_alias(target));
        // At namespace scope only: a directive or an alias at block scope
        // is written as before (row B1).
        if self.directive_scope(d).is_none() {
            self.write_ref(s, ns);
            return;
        }
        // The guard, by key (Addendum 3): plain iff the namespace has one
        // view key and no other namespace holds something under it.
        if self.ns_r2_plain(ns) {
            self.write_ref(s, ns);
            return;
        }
        match self.whole_reading(d, ns) {
            Reading::Plain => self.write_ref(s, ns),
            Reading::Unit(id, cause) => {
                s.uv(REF_UNIT).digest(&id).absorb(cause);
                crate::units::demand(self, ns);
            }
            Reading::Part(id, view, cause) => {
                s.uv(REF_PART).digest(&id).bool(view).absorb(cause);
                crate::units::demand(self, ns);
            }
            Reading::None(c) => {
                s.uv(REF_NONE).indeterminate(c);
            }
        }
    }

    /// The reading `d`'s reference to namespace entity `ns` is written
    /// with.
    fn whole_reading(&mut self, d: DeclId, ns: DeclId) -> Reading {
        let table = self.ns_table();
        let Some(&ni) = table.info.get(&ns) else { return Reading::Plain };
        if self.read_only_by_path(d) {
            // One reading, the dependency view's (its one view key if it
            // has no dependency part); never `part`.
            if ni.owned && ni.exists(false) {
                return match self.view_key(ns, false) {
                    KeyRes::Key { id, cause, .. } => Reading::Unit(id, cause),
                    KeyRes::None(c) => Reading::None(c.unwrap_or(NO_VIEW_KEY)),
                };
            }
            return Reading::Plain;
        }
        self.rebuilt_reference(d, ns);
        let (id, cause) = match self.key(ns) {
            KeyRes::Key { id, cause, .. } => (id, cause),
            KeyRes::None(_) => return Reading::Plain,
        };
        let named = self.named_by(&table, d, ns);
        let by_key = self.ns_by_key();
        if missing(&by_key, &id, &named).next().is_none() {
            Reading::Unit(id, cause.or(by_key.cause))
        } else {
            Reading::Part(id, ni.owned, cause.or(by_key.cause))
        }
    }

    /// The namespaces `d` names together with the other references that
    /// count for "whole": for an alias, its target alone; for a
    /// using-directive, the namespaces nominated by the using-directives
    /// of the same enclosing namespace entity in the same file (A3).
    fn named_by(&mut self, table: &Table, d: DeclId, ns: DeclId) -> BTreeSet<DeclId> {
        if self.ast.decls.get(d).kind() == DeclKind::UsingDirective
            && let Some(scope) = self.directive_scope(d).map(NsScope::ns)
        {
            let file = self.ns_file_of(d);
            if let Some(set) = table.named.get(&(scope, file)) {
                let mut set = set.clone();
                set.insert(ns);
                return set;
            }
        }
        BTreeSet::from([ns])
    }

    /// `d` is in a file that is read-only by the path of the file it is
    /// written in (its expansion location's), however that file was
    /// included (A4). A declaration in no file is not read-only here: its
    /// reference has readings, which is the conservative answer (a reading
    /// that is not whole is `part`).
    fn read_only_by_path(&mut self, d: DeclId) -> bool {
        match self.ns_file_of(d) {
            Some(f) => self.file_read_only(f),
            None => false,
        }
    }

    /// WP-118 R6.4: `d`, a reference to namespace `ns` that has readings
    /// because the file it is written in is not read-only by its own path,
    /// is recorded for `relied` when that file is rebuilt and not
    /// read-only for its inclusion: read-only, it would have had one
    /// reading. Only with the accounting on.
    fn rebuilt_reference(&mut self, d: DeclId, ns: DeclId) {
        if self.rule_use.is_none() {
            return;
        }
        let Some(f) = self.ns_file_of(d) else { return };
        if !matches!(self.file_class(f), Class::Rebuilt(_)) {
            return;
        }
        let loc = self.sm.expansion_loc(self.ast.decls.get(d).loc());
        if let Some(At::Clear { set, .. }) = self.read_only_at(loc) {
            let set = self.owned_state.sets.get(set).to_vec();
            crate::rule_use::ns_reference(self, ns, &set);
        }
    }

    /// The file of `d`'s expansion location, if it is in one.
    fn ns_file_of(&self, d: DeclId) -> Option<FileId> {
        let loc = self.ast.decls.get(d).loc();
        self.buffer_of(self.sm.expansion_loc(loc))
    }

    /// The enclosing namespace of using-directive `d`, through
    /// transparent contexts; `None` if it is not at namespace scope.
    fn directive_scope(&self, d: DeclId) -> Option<NsScope> {
        let ast = self.ast;
        let mut c = ast.decls.get(d).ctx();
        for _ in 0..ROUTE_LIMIT {
            let Some(x) = c else { return Some(NsScope::Global) };
            match info(ast.decls.get(x).kind()).role {
                Role::Root => return Some(NsScope::Global),
                Role::Transparent => c = ast.decls.get(x).ctx(),
                Role::Entity(KeyKind::Namespace) => return Some(NsScope::In(self.entity(x))),
                Role::Entity(_)
                | Role::Content(_)
                | Role::Nothing
                | Role::Local
                | Role::InParent
                | Role::Unsupported(_) => return None,
            }
        }
        None
    }

    /// The table, made on first need from every declaration of the AST.
    #[allow(clippy::too_many_lines, reason = "one pass over the declarations, for every table")]
    fn ns_table(&mut self) -> Arc<Table> {
        if let Some(t) = &self.ns_refs.table {
            return Arc::clone(t);
        }
        let ast = self.ast;
        let mut info_of: HashMap<DeclId, NsInfo> = HashMap::new();
        let mut order = Vec::new();
        let mut named: HashMap<(Option<DeclId>, Option<FileId>), BTreeSet<DeclId>> = HashMap::new();
        let mut dirs: HashMap<DeclId, Vec<(DeclId, bool)>> = HashMap::new();
        let mut seen: HashSet<DeclId> = HashSet::new();
        let ids: Vec<DeclId> = ast.decls.ids().collect();
        for &d in &ids {
            if ast.decls.get(d).kind() != DeclKind::Namespace {
                continue;
            }
            let e = self.entity(d);
            if info_of.contains_key(&e) {
                continue;
            }
            let owned = self.owned(e);
            let chain = self.chain(e);
            let mut inline = false;
            if chain.len() <= REDECL_LIMIT {
                for &x in &chain {
                    if let Some(n) = ast.decls.namespace(x) {
                        inline |= n.flags & NamespaceData::INLINE != 0;
                    }
                }
            }
            let parent = self.directive_scope(e).and_then(NsScope::ns);
            info_of
                .insert(e, NsInfo { owned, inline, parent, member: [false; 2], holds: [false; 2] });
            order.push(e);
        }
        // Addendum A4: a read-only opening holds, even when empty (an
        // empty owned opening is what a transform leaves behind, and holds
        // nothing).
        for &d in &ids {
            if ast.decls.get(d).kind() == DeclKind::Namespace && self.read_only_by_path(d) {
                let e = self.entity(d);
                if let Some(i) = info_of.get_mut(&e) {
                    i.member[0] = true;
                }
            }
        }
        for &d in &ids {
            let kind = ast.decls.get(d).kind();
            if kind == DeclKind::UsingDirective
                && let Some(scope) = self.directive_scope(d).map(NsScope::ns)
                && let Some(t) = ast.using_target(d)
            {
                let file = self.ns_file_of(d);
                let t = self.entity(ast.resolve_alias(t));
                named.entry((scope, file)).or_default().insert(t);
                if let Some(n) = scope {
                    let own = !self.read_only_by_path(d);
                    if own {
                        self.rebuilt_reference(d, t);
                    }
                    let v = dirs.entry(n).or_default();
                    if !v.contains(&(t, own)) {
                        v.push((t, own));
                    }
                }
            }
            if kind == DeclKind::Namespace {
                continue;
            }
            // The ruling's members (spec/verify.md §2.1): (a) an entity,
            // declared by the source or made by the compiler; (b) a
            // content-keyed declaration that is written: the
            // using-directive an unnamed namespace implies is no member.
            match info(kind).role {
                Role::Entity(_) => {}
                Role::Content(_) if !ast.decls.get(d).has(DeclFlags::IMPLICIT) => {}
                Role::Content(_)
                | Role::Unsupported(_)
                | Role::Nothing
                | Role::Transparent
                | Role::Root
                | Role::Local
                | Role::InParent => continue,
            }
            let e = self.entity(d);
            // F1 of the second review: a C-linkage function or variable is
            // keyed under `c_linkage`, but lookup finds it as a member of
            // every namespace a declaration of it is in ([dcl.link],
            // [namespace.udir]). Counted there, per declaration, in the
            // ownership of the declaration's file: the entity may be the
            // dependency's (it declares it too, `owned/c-function-dependency-declares`)
            // while an owned opening is what makes it a member here.
            if info(kind).c_linkage && self.in_c_linkage(e) {
                if let Some(n) = self.enclosing_namespace(d)
                    && info_of.contains_key(&n)
                {
                    let b = self.in_owned_file(d);
                    if let Some(i) = info_of.get_mut(&n) {
                        i.member[usize::from(b)] = true;
                    }
                }
                continue;
            }
            if !seen.insert(e) {
                continue;
            }
            let Some(n) = self.key_namespace(e) else { continue };
            if !info_of.contains_key(&n) {
                continue;
            }
            let b = self.owned(e);
            if let Some(i) = info_of.get_mut(&n) {
                i.member[usize::from(b)] = true;
            }
        }
        order.sort();
        // A4: what a namespace holds, deepest first, so that a nested
        // namespace's answer is known before its parent's.
        let depth = |n: DeclId| {
            let mut k = 0usize;
            let mut at = info_of.get(&n).and_then(|i| i.parent);
            while let Some(p) = at {
                k += 1;
                if k > ROUTE_LIMIT {
                    break;
                }
                at = info_of.get(&p).and_then(|i| i.parent);
            }
            k
        };
        let mut by_depth: Vec<(usize, DeclId)> = order.iter().map(|&n| (depth(n), n)).collect();
        by_depth.sort_by(|a, b| b.cmp(a));
        let mut inline_children: HashMap<DeclId, Vec<DeclId>> = HashMap::new();
        for &(_, n) in &by_depth {
            let Some(mut i) = info_of.get(&n).copied() else { continue };
            i.holds[0] |= i.member[0];
            i.holds[1] |= i.member[1];
            info_of.insert(n, i);
            if let Some(p) = i.parent
                && let Some(pi) = info_of.get_mut(&p)
            {
                pi.holds[0] |= i.holds[0];
                pi.holds[1] |= i.holds[1];
                if i.inline {
                    inline_children.entry(p).or_default().push(n);
                }
            }
        }
        for v in inline_children.values_mut() {
            v.sort();
        }
        let t = Arc::new(Table { info: info_of, order, named, dirs, inline_children });
        self.ns_refs.table = Some(Arc::clone(&t));
        t
    }

    /// `E(k)` for every key, made on first need.
    fn ns_by_key(&mut self) -> Arc<ByKey> {
        if let Some(b) = &self.ns_refs.by_key {
            return Arc::clone(b);
        }
        let table = self.ns_table();
        let aborts = self.aborts;
        let mut map: HashMap<Digest, Vec<DeclId>> = HashMap::new();
        let mut cause = None;
        for &n in &table.order {
            let Some(&ni) = table.info.get(&n) else { continue };
            for view in [false, true] {
                if !ni.exists(view) {
                    continue;
                }
                match self.ns_view_key(n, view) {
                    KeyRes::Key { id, .. } => {
                        let v = map.entry(id).or_default();
                        if !v.contains(&n) {
                            v.push(n);
                        }
                    }
                    KeyRes::None(c) => cause = cause.or(Some(c.unwrap_or(NO_VIEW_KEY))),
                }
            }
        }
        let b = Arc::new(ByKey { map, cause });
        if self.aborts == aborts {
            self.ns_refs.by_key = Some(Arc::clone(&b));
        }
        b
    }

    /// Whether namespace `n` holds something in `view` (`true`: owned):
    /// an entity of that ownership keyed under it, or a nested namespace
    /// that holds something in that view, transitively (A4; WP-140 review
    /// F3). Made from every declaration of the AST (`ns_table`), so it
    /// does not depend on the order the walk keys entities in. The one
    /// predicate of a view's existence: `E(k)`, R1's and R2's view keys,
    /// and `units::view_units`.
    pub(crate) fn ns_holds(&mut self, n: DeclId, view: bool) -> bool {
        let n = self.entity(n);
        let table = self.ns_table();
        table.info.get(&n).is_some_and(|i| i.holds[usize::from(view)])
    }

    /// R2's guard, by key (Addendum 3): a reference to `ns` that R2
    /// covers is written plain iff `ns` has exactly one view key `k` and
    /// no other namespace entity holds something under `k` (`E(k)` is `ns`
    /// alone, or empty when `ns` holds nothing: no other reading can
    /// differ from the key). A namespace whose own key has a cause has no
    /// view key and is written plain, with that cause. When a view key of
    /// some namespace cannot be formed, whether it is `k` is not known: not
    /// plain (fails closed). Per reference; no image-wide shortcut.
    pub(crate) fn ns_r2_plain(&mut self, ns: DeclId) -> bool {
        let keys = self.ns_view_keys(ns);
        match keys.as_slice() {
            [] => true,
            [(_, k)] => {
                let by_key = self.ns_by_key();
                by_key.cause.is_none()
                    && by_key.map.get(k).is_none_or(|v| v.iter().all(|&n| n == ns))
            }
            _ => false,
        }
    }

    /// The view keys of namespace `ns` (its own key, and its other view's
    /// where that view exists and the key differs), each with its view;
    /// for R1, A5 (`spelling.rs`) and the notes. Only keys formed without
    /// a cause; empty when its own key has one.
    pub(crate) fn ns_view_keys(&mut self, ns: DeclId) -> Vec<(bool, Digest)> {
        let ns = self.entity(ns);
        let own = match self.key(ns) {
            KeyRes::Key { id, cause: None, .. } => id,
            KeyRes::Key { .. } | KeyRes::None(_) => return Vec::new(),
        };
        let mine = self.owned(ns);
        if !self.ns_rules() {
            return vec![(mine, own)];
        }
        let table = self.ns_table();
        let Some(&ni) = table.info.get(&ns) else { return vec![(mine, own)] };
        let mut out = vec![(ni.owned, own)];
        if ni.exists(!ni.owned)
            && let KeyRes::Key { id, cause: None, .. } = self.view_key(ns, !ni.owned)
            && id != own
        {
            out.push((!ni.owned, id));
        }
        out
    }

    /// The single key R1 writes for `ns` named to find `found`, or `None`
    /// when it writes `Ref::views` or cannot form it (A5: an obligation at
    /// an R1 site holds only with that key; `spelling.rs`).
    pub(crate) fn ns_found_key(&mut self, ns: DeclId, found: &[DeclId]) -> Option<Digest> {
        match self.ns_found_ids(ns, found, false) {
            Some((ids, None)) => match ids.as_slice() {
                [id] => Some(*id),
                _ => None,
            },
            Some((_, Some(_))) => None,
            None => match self.key(self.ast.resolve_alias(ns)) {
                KeyRes::Key { id, cause: None, .. } => Some(id),
                KeyRes::Key { .. } | KeyRes::None(_) => None,
            },
        }
    }

    /// The key the reading of `d` (a using-directive or a namespace
    /// alias) is written with for its target, when that reading is whole
    /// and formed without a cause; `None` for a reading that is `part`
    /// (A5: an obligation at a whole-namespace reference holds only with a
    /// whole reading's key).
    pub(crate) fn ns_whole_key(&mut self, d: DeclId) -> Option<Digest> {
        let t = self.ast.using_target(d)?;
        let ns = self.entity(self.ast.resolve_alias(t));
        let plain = |w: &mut Self| match w.key(ns) {
            KeyRes::Key { id, cause: None, .. } => Some(id),
            KeyRes::Key { .. } | KeyRes::None(_) => None,
        };
        if !self.ns_rules() || self.directive_scope(d).is_none() || self.ns_r2_plain(ns) {
            return plain(self);
        }
        match self.whole_reading(d, ns) {
            Reading::Plain => plain(self),
            Reading::Unit(id, None) => Some(id),
            Reading::Unit(_, Some(_)) | Reading::Part(..) | Reading::None(_) => None,
        }
    }
}

/// The cause the own key of `ns` was formed with, if any.
fn keys_cause(w: &mut Walk<'_>, ns: DeclId) -> Option<Cause> {
    match w.key(ns) {
        KeyRes::Key { cause, .. } => cause,
        KeyRes::None(c) => c,
    }
}

/// What a whole-namespace reference is written as.
enum Reading {
    /// As `write_ref` writes it.
    Plain,
    Unit(Digest, Option<Cause>),
    Part(Digest, bool, Option<Cause>),
    None(Cause),
}

/// The namespaces of `E(k)` that `named` does not hold.
fn missing<'b>(
    by_key: &'b ByKey,
    k: &Digest,
    named: &'b BTreeSet<DeclId>,
) -> impl Iterator<Item = DeclId> + 'b {
    by_key.map.get(k).into_iter().flatten().copied().filter(move |n| !named.contains(n))
}

/// The notes record (`Output::namespace_parts`) and the part set for the
/// display names, over the units written: for each owned using-directive
/// at namespace scope and each namespace alias, each of its target's view
/// keys whose reading is not whole. Run once, after the walk.
pub(crate) fn notes(w: &mut Walk<'_>, written: &[crate::units::Written]) -> Vec<NamespacePart> {
    if !w.ns_rules() {
        return Vec::new();
    }
    let ast = w.ast;
    let mut out = Vec::new();
    for u in written {
        let Some(d) = u.entity else { continue };
        if u.view.is_some()
            || u.resolved.is_some()
            || u.default_argument.is_some()
            || u.operands.is_some()
        {
            continue;
        }
        let kind = ast.decls.get(d).kind();
        let whole_kind = match kind {
            DeclKind::UsingDirective => w.directive_scope(d).is_some(),
            DeclKind::NamespaceAlias => true,
            _ => false,
        };
        if !whole_kind || w.read_only_by_path(d) {
            continue;
        }
        let Some(t) = ast.using_target(d) else { continue };
        let ns = w.entity(ast.resolve_alias(t));
        let table = w.ns_table();
        let Some(&ni) = table.info.get(&ns) else { continue };
        if w.ns_r2_plain(ns) {
            // One reading (the guard of `write_ns_whole`).
            continue;
        }
        let named = w.named_by(&table, d, ns);
        let by_key = w.ns_by_key();
        let own = match w.key(ns) {
            KeyRes::Key { id, .. } => Some(id),
            KeyRes::None(_) => None,
        };
        for (_, k) in w.ns_view_keys(ns) {
            let gone: Vec<DeclId> = missing(&by_key, &k, &named).collect();
            if gone.is_empty() {
                continue;
            }
            let written_reading = Some(k) == own;
            if written_reading {
                w.ns_refs.part.insert(d, ni.owned);
            }
            let mut miss: Vec<(String, Vec<u8>)> = gone
                .into_iter()
                .map(|m| {
                    let path = w.first_file(m).map(|f| (w.subject_path)(f)).unwrap_or_default();
                    (crate::names::display_name(ast, m), path)
                })
                .collect();
            miss.sort();
            out.push(NamespacePart {
                unit: u.key,
                key: k,
                written: written_reading,
                missing: miss,
            });
        }
    }
    out.sort();
    out.dedup();
    out
}

impl Walk<'_> {
    /// The file of the first opening of namespace entity `n` (in chain
    /// order) that is in a file.
    fn first_file(&self, n: DeclId) -> Option<FileId> {
        self.chain(n).into_iter().find_map(|x| self.ns_file_of(x))
    }
}

/// The display name of a using-directive whose written reading is
/// `Ref::part` says which part it names (`spec/landed/digest/WP-141.md`
/// §8.6, provisional): `<using_directive legacy (the part the rule
/// leaves)>` for a namespace that is the dependency's, `(the part the
/// rule moves)` for an owned one. Applied per unit by `names::names`,
/// so that a unit's name alone is the one among the walk's (two
/// directives can share a key, one naming a part and one not).
pub(crate) fn part_name(w: &Walk<'_>, u: &crate::units::Written, name: &mut String) {
    let Some(d) = u.entity else { return };
    let Some(&owned) = w.ns_refs.part.get(&d) else { return };
    if u.view.is_some() || w.ast.decls.get(d).kind() != DeclKind::UsingDirective {
        return;
    }
    let what = if owned { " (the part the rule moves)" } else { " (the part the rule leaves)" };
    if name.ends_with('>') {
        name.insert_str(name.len() - 1, what);
    } else {
        name.push_str(what);
    }
}

/// `digest.ref.namespace_no_referent` and
/// `digest.ref.namespace_route_unknown`, each only when not 0 (so that no
/// output under a table that gives no namespace a second view changes),
/// kept sorted by name.
pub(crate) fn counters(w: &Walk<'_>, counters: &mut Vec<slip_model::Counter>) {
    for (name, value) in [
        ("digest.ref.namespace_no_referent", w.ns_refs.no_referent),
        ("digest.ref.namespace_route_unknown", w.ns_refs.route_unknown),
    ] {
        if value != 0 {
            counters.push(slip_model::Counter { name: name.to_owned(), value });
        }
    }
    counters.sort_by(|a, b| a.name.cmp(&b.name));
}

/// What the lookup of expression `e` found, for R1 at its qualifier: an
/// unresolved name's set (`UnresolvedLookupExpr.b`), a concept-id's
/// concept (A2); `None` for any other expression (its qualifier is no R1
/// site: a dependent destructor name, a dependent member).
pub(crate) fn found_of(w: &Walk<'_>, e: slip_kiln::kiln::ast::ExprId) -> Option<Vec<DeclId>> {
    found_list(w.ast, e)
}

/// [`found_of`] on an AST (for `spelling.rs`).
pub(crate) fn found_list(
    ast: &slip_kiln::kiln::ast::Ast,
    e: slip_kiln::kiln::ast::ExprId,
) -> Option<Vec<DeclId>> {
    use slip_kiln::kiln::ast::ListId;
    use slip_kiln::kiln::ast::expr::ExprKind;
    let n = ast.exprs.get(e);
    match n.kind() {
        ExprKind::UnresolvedLookupExpr => Some(
            ListId::try_new(n.b())
                .map(|l| ast.lists.iter::<DeclId>(l).collect())
                .unwrap_or_default(),
        ),
        ExprKind::ConceptSpecializationExpr => DeclId::try_new(n.a()).map(|c| vec![c]),
        _ => None,
    }
}
