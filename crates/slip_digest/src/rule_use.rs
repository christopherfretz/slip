//! How each rule was used (`spec/verify.md` §9.1 "Rule use", §9.5). Work
//! package WP-069.
//!
//! With [`Options::rule_use`](crate::Options) and a rule table that is not
//! empty, the walk counts, for each rule `r` (its index in
//! `Declared.rules`, which is [`RuleTable::rules`]' order):
//!
//! - `via(r)`: the entities whose own `(parent, kind, name, disc)`, parent
//!   and discriminator mapped, matched `r` ([`RuleTable::map_traced`]);
//! - `direct(r)`: the entities no rule matched whose key is the key `r`
//!   produces, `(r.to_parent, r.kind, r.to_name, r.disc)`
//!   ([`RuleTable::produces`]: at most one rule does, `spec/verify.md`
//!   §2.1);
//!   for a namespace rule, see below.
//!
//! **One place.** An entity's key is mapped in one place,
//! `Walk::key` (`keys.rs`), which calls [`map`] here; the trace is kept
//! by entity (its first declaration, `Walk::entity`), so an entity is
//! counted once however many declarations it has, however many times its
//! key is asked for, and whether it is declared in the unit's files or
//! only reached through a reference. A key computed again (one not
//! memoised, below a depth limit or a cycle) replaces the trace it gave
//! before, and a computation that forms no key, or stops at the depth
//! limit, removes it ([`forget`]): the trace is that of the entity's last
//! key, which once memoised is its only one.
//!
//! **What is not counted** (`spec/landed/verify/WP-069.md`): the names a
//! key takes from another entity's mapped key without a rule matching it
//! are not a rule's use. A specialisation (its name is its template's,
//! mapped, `spec_key`) and a member of an instantiation (its name is its
//! pattern member's, mapped, `follow_patterns`) are counted by their own
//! mapped key only. No rule matches a specialisation (a rule of kind
//! `specialisation` is refused). A rule matches an instantiated member's
//! own (plain) key only where it produces exactly the key following its
//! pattern gives (WP-119 item 1): `follow_patterns` then returns the plain
//! key, and the one mapping here gives the followed key through that rule,
//! so the member is in `via(r)`, as it is in a unit that does not
//! instantiate its class, and in no rule's `direct`; every other rule on it
//! is `key/rule-on-instantiated-member`. A member no rule matches is in
//! `direct(r)` only if `r` produces its key: for a member of a class's
//! instantiation only a rule whose `to_parent` is that specialisation
//! (one written for the member itself), possibly for an instantiated
//! friend, whose parent is a namespace. An
//! entity with no key (its key is indeterminate with no key formed: under
//! a refused table, a cycle, `key/rule-on-instantiated-member`) is not
//! counted; one whose key is formed and carries a cause is.
//!
//! **No extra work when off.** With the option off, or an empty table,
//! `Walk::rule_use` is `None` and [`map`] is `RuleTable::map`.
//!
//! **A rule of kind `namespace`** (`spec/verify.md` §9.2 "Stale for a
//! namespace rule", WP-111). The namespace merges, its members do not: its
//! `via` is counted as above, but its `direct` and `kept_via` count the
//! owned entities, not namespaces, that are keyed directly under a
//! namespace (their parent, in the owned view of `spec/verify.md` §2.1
//! rule 2, is a namespace: not a class member, not an `extern "C"`
//! entity, not a specialisation, which is keyed under its template). For
//! such an entity the walk keeps its namespace (`Counting::parents`, from
//! `Walk::namespace_parent`) and whether its mapped key is in
//! `Declared.kept`; for each namespace in the owned view, the trace of its
//! own key (an owned namespace's in its entry, a dependency namespace's
//! in `Counting::views`). [`finish`] climbs the chain of namespaces of
//! each such entity, **all** of it:
//!
//! - a level no rule matched whose key a rule `r` produces (its own,
//!   unmapped key is `T(r)`; with no two producers of one key, §2.1, this
//!   is the level being left in the old namespace): the entity is inside the namespace `r` says
//!   the owned members left; it counts in `direct(r)` unless its key is
//!   kept;
//! - a level a rule `r` matched: the entity's namespace chain was mapped
//!   by `r`; it counts in `kept_via(r)` if its key is kept.
//!
//! Each rule once per entity. The levels between the entity and the one
//! that decides do not matter: an inline namespace, the unnamed namespace
//! and a nested namespace another rule maps elsewhere are levels like any
//! other (`spec/landed/verify/WP-111.md` §1). A namespace entity's own
//! `direct` trace is not counted (the merge of §2.1).
//!
//! **A namespace's unit in the owned view** (WP-140, `spec/digest.md`
//! §3.4, `spec/verify.md` §9.2). A dependency namespace that has a unit in
//! the owned view ([`view_unit`], from `units::view_units`, only when that
//! unit is written: its key differs from the own key) counts its
//! owned-view trace in `via`, once per namespace entity however many owned
//! openings it has, as an owned namespace's own key does.
//!
//! **Kept keys seen.** Every entity, owned or not, whose mapped key is in
//! `Declared.kept` gives that key's index to `Output`'s list of kept keys
//! seen, so the run can report a `kept` key no observer has (§9.2).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use slip_kiln::kiln::ast::ids::DeclId;
use slip_model::Digest;
use slip_model::agreement::Use;
use slip_model::keys::{Key, KeyKind};

use crate::walk::Walk;

/// How far a chain of namespaces is climbed (an AST is far shallower).
const CHAIN_LIMIT: usize = 4096;

/// What one entity's key says of the rules.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Trace {
    /// The rule that matched its own key.
    Via(usize),
    /// No rule matched; the rule that produces its key.
    Direct(usize),
}

/// What the walk keeps of one entity's last key.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Entry {
    trace: Option<Trace>,
    /// The index in `kept` of its mapped key.
    kept: Option<usize>,
    /// Its key is of kind `namespace`.
    namespace: bool,
    /// Owned (or a dependency entity by the per-inclusion rule alone), not
    /// a namespace and not a guide of a dependency template: counted for
    /// namespace rules if its parent is a namespace.
    member: bool,
}

impl Entry {
    fn is_empty(&self) -> bool {
        self.trace.is_none() && self.kept.is_none() && !self.member
    }
}

/// The traces of one walk, by entity.
#[derive(Debug, Default)]
pub(crate) struct Counting {
    entries: HashMap<DeclId, Entry>,
    /// A dependency namespace's own key in the owned view (`key_in` with
    /// `Some(true)`): the trace of the rules along it.
    views: HashMap<DeclId, Option<Trace>>,
    /// An entity's namespace in the owned view: the parent its key was
    /// formed under, when that is a namespace.
    parents: HashMap<DeclId, DeclId>,
    /// The dependency namespaces that have a unit of their own in the
    /// owned view (`units::view_units`, WP-140): the rule that matched
    /// that view's key counts `via` once for each.
    view_units: BTreeSet<DeclId>,
    /// `Declared.kept`, ascending.
    kept: Vec<Digest>,
}

impl Counting {
    /// Counting with `Declared.kept` (ascending, as `Declared` holds it).
    pub(crate) fn new(kept: &[Digest]) -> Counting {
        Counting { kept: kept.to_vec(), ..Counting::default() }
    }

    /// `e`'s trace is now `t`; `None` removes the one it had.
    #[cfg(test)]
    fn record(&mut self, e: DeclId, t: Option<Trace>) {
        self.set(e, Entry { trace: t, ..Entry::default() });
    }

    fn set(&mut self, e: DeclId, entry: Entry) {
        if entry.is_empty() {
            self.entries.remove(&e);
        } else {
            self.entries.insert(e, entry);
        }
    }

    fn kept_index(&self, key: &Key) -> Option<usize> {
        if self.kept.is_empty() {
            return None;
        }
        self.kept.binary_search(&key.id()).ok()
    }

    /// The trace of namespace `ns`'s own key in the owned view.
    fn level(&self, ns: DeclId) -> Option<&Trace> {
        match self.views.get(&ns) {
            Some(t) => t.as_ref(),
            None => self.entries.get(&ns).and_then(|e| e.trace.as_ref()),
        }
    }
}

/// The trace of a key under the table: the rule that matched it, or the
/// rule that produces it.
fn traced(w: &Walk<'_>, key: Key) -> (Key, Option<Trace>) {
    let (mapped, via) = w.rules.map_traced(key);
    let trace = match via {
        Some(r) => Some(Trace::Via(r)),
        None => w.rules.produces(&mapped).map(Trace::Direct),
    };
    (mapped, trace)
}

/// The last step of mapping entity `e`'s key (`RuleTable::map`), with the
/// trace kept when the walk counts rule use. `e` is owned.
pub(crate) fn map(w: &mut Walk<'_>, e: DeclId, key: Key) -> Key {
    if w.rule_use.is_none() {
        return w.rules.map(key);
    }
    let (mapped, trace) = traced(w, key);
    let namespace = mapped.kind == KeyKind::Namespace;
    let member = !namespace && !guide_of_dependency(w, e, &mapped);
    if let Some(c) = w.rule_use.as_mut() {
        let entry = Entry { trace, kept: c.kept_index(&mapped), namespace, member };
        c.set(e, entry);
    }
    mapped
}

/// `e` is a deduction guide whose template is a dependency entity (N29
/// `good-5`): keyed beside a template the run cannot move, it is not a
/// member a namespace rule counts. A guide of an owned template is
/// counted like any owned member, whatever scope it is in (Kiln accepts a
/// guide left in another scope than its template's, which clang 19
/// rejects: `spec/landed/verify/WP-111.md`, For Kiln). A guide whose
/// template is not found is counted.
fn guide_of_dependency(w: &mut Walk<'_>, e: DeclId, key: &Key) -> bool {
    if key.kind != KeyKind::DeductionGuide {
        return false;
    }
    match w.guide_template(e) {
        Some(t) => !w.owned(t),
        None => false,
    }
}

/// Entity `e`'s key is `key`, which no rule maps (a dependency entity,
/// `spec/verify.md` §2.1 rule 1): counted as `direct` for the rule that
/// produces it, as [`map`] counts a key no rule matches. For a namespace
/// rule's `direct` and `kept_via` (§9.2) it is counted only when it is a
/// dependency entity by the per-inclusion rule alone
/// (`Walk::only_by_inclusion`, WP-111 review F2): every declaration in a
/// file the run's files own, one entered through a read-only file. Such an
/// entity is counted as an owned member is (the observer that leaves it
/// behind is stale unless its key is kept; kept, an observer that moved
/// it is stale by `kept_via`). An entity the dependency itself declares is
/// not counted.
pub(crate) fn unmapped(w: &mut Walk<'_>, e: DeclId, key: Key) -> Key {
    if w.rule_use.is_none() {
        return key;
    }
    let namespace = key.kind == KeyKind::Namespace;
    let member = !namespace && w.only_by_inclusion(e) && !guide_of_dependency(w, e, &key);
    let trace = w.rules.produces(&key).map(Trace::Direct);
    if let Some(c) = w.rule_use.as_mut() {
        let entry = Entry { trace, kept: c.kept_index(&key), namespace, member };
        c.set(e, entry);
    }
    key
}

/// Namespace `e`'s key in the owned view (`Walk::key_in` with
/// `Some(true)`: a dependency namespace around an owned member), mapped
/// with its trace kept as a level of the member's chain.
pub(crate) fn in_owned_view(w: &mut Walk<'_>, e: DeclId, key: Key) -> Key {
    if w.rule_use.is_none() {
        return w.rules.map(key);
    }
    let (mapped, trace) = traced(w, key);
    if let Some(c) = w.rule_use.as_mut() {
        c.views.insert(e, trace);
    }
    mapped
}

/// Dependency namespace `e` has a unit under its key in the owned view
/// (`units::view_units`, WP-140): the trace of that key
/// (`Counting::views`) counts as the namespace's own, once.
pub(crate) fn view_unit(w: &mut Walk<'_>, e: DeclId) {
    if let Some(c) = w.rule_use.as_mut() {
        c.view_units.insert(e);
    }
}

/// Entity `e`'s parent in the owned view is the namespace `ns`
/// (`Walk::namespace_parent`).
pub(crate) fn namespace_parent(w: &mut Walk<'_>, e: DeclId, ns: DeclId) {
    if let Some(c) = w.rule_use.as_mut() {
        c.parents.insert(e, ns);
    }
}

/// Entity `e`'s key was computed again and formed none (or stopped at the
/// depth limit): it is counted for no rule, whatever an earlier
/// computation said. `view` is the view it was computed in: a namespace's
/// key in the owned view is forgotten as a level, its own key as the
/// entity's.
pub(crate) fn forget(w: &mut Walk<'_>, e: DeclId, view: Option<bool>) {
    if let Some(c) = w.rule_use.as_mut() {
        c.forget(e, view);
    }
}

impl Counting {
    /// [`forget`]'s change: the entity's entry, or its level in the owned
    /// view, is gone (a level forgotten is `None`, not absent, so that
    /// [`Counting::level`] does not fall back to the namespace's own
    /// entry).
    fn forget(&mut self, e: DeclId, view: Option<bool>) {
        match view {
            None => {
                self.entries.remove(&e);
            }
            Some(true) => {
                self.views.insert(e, None);
            }
            Some(false) => {}
        }
    }
}

/// `Output::rule_use`: one `Use` per rule with a count that is not 0,
/// sorted by rule; and the indices into `kept` of the keys some entity
/// has, ascending.
pub(crate) fn finish(c: Option<Counting>) -> (Vec<Use>, Vec<u64>) {
    let Some(c) = c else { return (Vec::new(), Vec::new()) };
    let mut by_rule: BTreeMap<usize, Use> = BTreeMap::new();
    let mut seen = BTreeSet::new();
    // A namespace with a unit in the owned view and no entry of its own
    // (its own key, unmapped, traces nothing) is counted as one with an
    // empty entry.
    let empty = Entry::default();
    let viewed = c.view_units.iter().filter(|e| !c.entries.contains_key(e)).map(|e| (e, &empty));
    for (e, entry) in c.entries.iter().chain(viewed) {
        if let Some(k) = entry.kept {
            seen.insert(k as u64);
        }
        // Each rule counts once per entity and count.
        let (mut via, mut direct, mut kept_via) =
            (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
        match &entry.trace {
            Some(Trace::Via(r)) => {
                via.insert(*r);
            }
            // A namespace that keeps the key a rule maps another onto is
            // the merge of §2.1; its members are counted below.
            Some(Trace::Direct(r)) if !entry.namespace => {
                direct.insert(*r);
            }
            Some(Trace::Direct(_)) | None => {}
        }
        if entry.member
            && let Some((d, v)) = chain(&c, *e)
        {
            if entry.kept.is_some() {
                kept_via.extend(v);
            } else {
                direct.extend(d);
            }
        }
        // A dependency namespace with a unit in the owned view (WP-140):
        // the rule that matched that view's key is used by it, once.
        if c.view_units.contains(e)
            && let Some(Some(Trace::Via(r))) = c.views.get(e)
        {
            via.insert(*r);
        }
        for r in via {
            at(&mut by_rule, r).via += 1;
        }
        for r in direct {
            at(&mut by_rule, r).direct += 1;
        }
        for r in kept_via {
            at(&mut by_rule, r).kept_via += 1;
        }
    }
    (by_rule.into_values().collect(), seen.into_iter().collect())
}

/// The `Use` of rule `r`, made with every count 0 if it has none.
fn at(by_rule: &mut BTreeMap<usize, Use>, r: usize) -> &mut Use {
    by_rule.entry(r).or_insert(Use { rule: r as u64, via: 0, direct: 0, kept_via: 0 })
}

/// For an entity whose parent is a namespace: the rule that produces the
/// own key of each namespace around it no rule matched, and the rule that
/// matched each other one; each once, ascending. `None` when its parent is not a
/// namespace.
fn chain(c: &Counting, e: DeclId) -> Option<(BTreeSet<usize>, BTreeSet<usize>)> {
    let mut ns = *c.parents.get(&e)?;
    let (mut direct, mut via) = (BTreeSet::new(), BTreeSet::new());
    for _ in 0..CHAIN_LIMIT {
        match c.level(ns) {
            Some(Trace::Via(r)) => {
                via.insert(*r);
            }
            Some(Trace::Direct(r)) => {
                direct.insert(*r);
            }
            None => {}
        }
        match c.parents.get(&ns) {
            Some(&p) => ns = p,
            None => break,
        }
    }
    Some((direct, via))
}

#[cfg(test)]
mod tests {
    use super::{Counting, Entry, Trace, finish};
    use slip_kiln::kiln::ast::ids::DeclId;
    use slip_model::agreement::Use;

    // The trace is the entity's last: a later one replaces it, `None`
    // removes it (WP-069 review, F3; mutation D: the first trace wins).
    #[test]
    fn last_trace_wins() {
        let finish = |c: Option<Counting>| finish(c).0;
        let (e, f) = (DeclId::new(1), DeclId::new(2));
        let mut c = Counting::default();
        c.record(e, Some(Trace::Via(0)));
        c.record(e, Some(Trace::Via(1)));
        c.record(f, Some(Trace::Direct(2)));
        assert_eq!(
            finish(Some(c)),
            [
                Use { rule: 1, via: 1, direct: 0, kept_via: 0 },
                Use { rule: 2, via: 0, direct: 1, kept_via: 0 },
            ]
        );

        let mut c = Counting::default();
        c.record(e, Some(Trace::Via(0)));
        c.record(e, Some(Trace::Direct(1)));
        assert_eq!(finish(Some(c)), [Use { rule: 1, via: 0, direct: 1, kept_via: 0 }]);

        let mut c = Counting::default();
        c.record(e, Some(Trace::Via(0)));
        c.record(f, Some(Trace::Via(0)));
        c.record(e, None);
        assert_eq!(finish(Some(c)), [Use { rule: 0, via: 1, direct: 0, kept_via: 0 }]);

        let mut c = Counting::default();
        c.record(e, None);
        assert_eq!(finish(Some(c)), []);
        assert_eq!(finish(None), []);
    }

    // A namespace's level in the owned view, forgotten, is no level: not
    // the trace it had, and not its own entry's (WP-111 review, mutant D3:
    // forgetting a view did nothing).
    #[test]
    fn a_forgotten_view_is_no_level() {
        let (ns, member) = (DeclId::new(1), DeclId::new(2));
        let mut c = Counting::default();
        c.set(ns, Entry { trace: Some(Trace::Direct(3)), namespace: true, ..Entry::default() });
        c.views.insert(ns, Some(Trace::Direct(0)));
        c.set(member, Entry { member: true, ..Entry::default() });
        c.parents.insert(member, ns);
        assert_eq!(c.level(ns), Some(&Trace::Direct(0)));
        c.forget(ns, Some(true));
        assert_eq!(c.level(ns), None);
        c.forget(ns, Some(false));
        assert_eq!(c.level(ns), None);
        assert_eq!(finish(Some(c)).0, []);
        let mut c = Counting::default();
        c.set(member, Entry { trace: Some(Trace::Via(1)), member: true, ..Entry::default() });
        c.forget(member, None);
        assert_eq!(finish(Some(c)).0, []);
    }
}
