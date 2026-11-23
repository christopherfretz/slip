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
//!   ([`RuleTable::producers`]: every rule that produces it, when two do).
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

use std::collections::{BTreeMap, HashMap};

use slip_kiln::kiln::ast::ids::DeclId;
use slip_model::agreement::Use;
use slip_model::keys::Key;

use crate::walk::Walk;

/// What one entity's key says of the rules.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Trace {
    /// The rule that matched its own key.
    Via(usize),
    /// No rule matched; the rules that produce its key (not empty).
    Direct(Vec<usize>),
}

/// The traces of one walk, by entity.
#[derive(Debug, Default)]
pub(crate) struct Counting {
    traces: HashMap<DeclId, Trace>,
}

impl Counting {
    /// `e`'s trace is now `t`; `None` removes the one it had.
    fn record(&mut self, e: DeclId, t: Option<Trace>) {
        match t {
            Some(t) => {
                self.traces.insert(e, t);
            }
            None => {
                self.traces.remove(&e);
            }
        }
    }
}

/// The last step of mapping entity `e`'s key (`RuleTable::map`), with the
/// trace kept when the walk counts rule use.
pub(crate) fn map(w: &mut Walk<'_>, e: DeclId, key: Key) -> Key {
    let rules = w.rules;
    let Some(c) = w.rule_use.as_mut() else {
        return rules.map(key);
    };
    let (mapped, via) = rules.map_traced(key);
    let trace = if let Some(r) = via {
        Some(Trace::Via(r))
    } else {
        let direct: Vec<usize> = rules.producers(&mapped).collect();
        (!direct.is_empty()).then_some(Trace::Direct(direct))
    };
    c.record(e, trace);
    mapped
}

/// Entity `e`'s key is `key`, which no rule maps (a dependency entity,
/// `spec/verify.md` §2.1 rule 1): counted only as `direct` for the rules
/// that produce it, as [`map`] counts a key no rule matches.
pub(crate) fn unmapped(w: &mut Walk<'_>, e: DeclId, key: Key) -> Key {
    let rules = w.rules;
    if let Some(c) = w.rule_use.as_mut() {
        let direct: Vec<usize> = rules.producers(&key).collect();
        c.record(e, (!direct.is_empty()).then_some(Trace::Direct(direct)));
    }
    key
}

/// Entity `e`'s key was computed again and formed none (or stopped at the
/// depth limit): it is counted for no rule, whatever an earlier
/// computation said.
pub(crate) fn forget(w: &mut Walk<'_>, e: DeclId) {
    if let Some(c) = w.rule_use.as_mut() {
        c.record(e, None);
    }
}

/// `Output::rule_use`: one `Use` per rule with a count that is not 0,
/// sorted by rule.
pub(crate) fn finish(c: Option<Counting>) -> Vec<Use> {
    let Some(c) = c else { return Vec::new() };
    let mut by_rule: BTreeMap<usize, (u64, u64)> = BTreeMap::new();
    for t in c.traces.values() {
        match t {
            Trace::Via(r) => by_rule.entry(*r).or_default().0 += 1,
            Trace::Direct(rs) => {
                for r in rs {
                    by_rule.entry(*r).or_default().1 += 1;
                }
            }
        }
    }
    by_rule.into_iter().map(|(r, (via, direct))| Use { rule: r as u64, via, direct }).collect()
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
        let (e, f) = (DeclId::new(1), DeclId::new(2));
        let mut c = Counting::default();
        c.record(e, Some(Trace::Via(0)));
        c.record(e, Some(Trace::Via(1)));
        c.record(f, Some(Trace::Direct(vec![0, 2])));
        assert_eq!(
            finish(Some(c)),
            [
                Use { rule: 0, via: 0, direct: 1 },
                Use { rule: 1, via: 1, direct: 0 },
                Use { rule: 2, via: 0, direct: 1 },
            ]
        );

        let mut c = Counting::default();
        c.record(e, Some(Trace::Via(0)));
        c.record(e, Some(Trace::Direct(vec![1])));
        assert_eq!(finish(Some(c)), [Use { rule: 1, via: 0, direct: 1 }]);

        let mut c = Counting::default();
        c.record(e, Some(Trace::Via(0)));
        c.record(f, Some(Trace::Via(0)));
        c.record(e, None);
        assert_eq!(finish(Some(c)), [Use { rule: 0, via: 1, direct: 0 }]);

        let mut c = Counting::default();
        c.record(e, None);
        assert_eq!(finish(Some(c)), []);
        assert_eq!(finish(None), []);
    }
}
