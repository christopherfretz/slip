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
//!   ([`RuleTable::producers`]: every rule that produces it, when two do);
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
//! mapped key only. No rule matches it (a rule of kind `specialisation`
//! is refused; one on an instantiated member is
//! `key/rule-on-instantiated-member`), and it is in `direct(r)` only if
//! `r` produces that key: never for a member of a class's instantiation,
//! whose parent is a specialisation, which is no rule's `to_parent`, but
//! possibly for an instantiated friend, whose parent is a namespace. An
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
//! - a level no rule matched whose key some rules produce (its own,
//!   unmapped key is `T(r)`): the entity is inside the namespace `r` says
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
    /// No rule matched; the rules that produce its key (not empty).
    Direct(Vec<usize>),
}

/// What the walk keeps of one entity's last key.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Entry {
    trace: Option<Trace>,
    /// The index in `kept` of its mapped key.
    kept: Option<usize>,
    /// Its key is of kind `namespace`.
    namespace: bool,
    /// Owned and not a namespace: counted for namespace rules if its
    /// parent is a namespace.
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
/// rules that produce it.
fn traced(w: &Walk<'_>, key: Key) -> (Key, Option<Trace>) {
    let (mapped, via) = w.rules.map_traced(key);
    let trace = if let Some(r) = via {
        Some(Trace::Via(r))
    } else {
        let direct: Vec<usize> = w.rules.producers(&mapped).collect();
        (!direct.is_empty()).then_some(Trace::Direct(direct))
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
    if let Some(c) = w.rule_use.as_mut() {
        let namespace = mapped.kind == KeyKind::Namespace;
        // A deduction guide must be in its template's scope
        // ([temp.deduct.guide]), as a specialisation is keyed under its
        // template: left with it, it is not counted (WP-111).
        let member = !namespace && mapped.kind != KeyKind::DeductionGuide;
        let entry = Entry { trace, kept: c.kept_index(&mapped), namespace, member };
        c.set(e, entry);
    }
    mapped
}

/// Entity `e`'s key is `key`, which no rule maps (a dependency entity,
/// `spec/verify.md` §2.1 rule 1): counted only as `direct` for the rules
/// that produce it, as [`map`] counts a key no rule matches, and never for
/// a namespace rule's `direct` or `kept_via` (§9.2).
pub(crate) fn unmapped(w: &mut Walk<'_>, e: DeclId, key: Key) -> Key {
    let rules = w.rules;
    if let Some(c) = w.rule_use.as_mut() {
        let direct: Vec<usize> = rules.producers(&key).collect();
        let entry = Entry {
            trace: (!direct.is_empty()).then_some(Trace::Direct(direct)),
            kept: c.kept_index(&key),
            namespace: key.kind == KeyKind::Namespace,
            member: false,
        };
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
        match view {
            None => {
                c.entries.remove(&e);
            }
            Some(true) => {
                c.views.insert(e, None);
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
    for (e, entry) in &c.entries {
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
            Some(Trace::Direct(rs)) if !entry.namespace => direct.extend(rs.iter().copied()),
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

/// For an entity whose parent is a namespace: the rules that produce the
/// own key of a namespace around it no rule matched, and the rules that
/// matched one; each once, ascending. `None` when its parent is not a
/// namespace.
fn chain(c: &Counting, e: DeclId) -> Option<(BTreeSet<usize>, BTreeSet<usize>)> {
    let mut ns = *c.parents.get(&e)?;
    let (mut direct, mut via) = (BTreeSet::new(), BTreeSet::new());
    for _ in 0..CHAIN_LIMIT {
        match c.level(ns) {
            Some(Trace::Via(r)) => {
                via.insert(*r);
            }
            Some(Trace::Direct(rs)) => direct.extend(rs.iter().copied()),
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
    use super::{Counting, Trace, finish};
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
        c.record(f, Some(Trace::Direct(vec![0, 2])));
        assert_eq!(
            finish(Some(c)),
            [
                Use { rule: 0, via: 0, direct: 1, kept_via: 0 },
                Use { rule: 1, via: 1, direct: 0, kept_via: 0 },
                Use { rule: 2, via: 0, direct: 1, kept_via: 0 },
            ]
        );

        let mut c = Counting::default();
        c.record(e, Some(Trace::Via(0)));
        c.record(e, Some(Trace::Direct(vec![1])));
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
}
