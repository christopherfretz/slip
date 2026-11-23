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
//! before: the trace is that of the entity's last key, which once
//! memoised is its only one.
//!
//! **What is not counted** (`spec/landed/verify/WP-069.md`): the names a
//! key takes from another entity's mapped key without a rule matching it
//! are not a rule's use. A specialisation (its name is its template's,
//! mapped, `spec_key`) and a member of an instantiation (its name is its
//! pattern member's, mapped, `follow_patterns`) are counted by their own
//! mapped key only, which no rule matches and none produces (a rule of
//! kind `specialisation` is refused; an instantiated member's parent is
//! a specialisation, never a rule's `to_parent`). An entity with no key
//! (its key is indeterminate with no key formed: under a refused table,
//! a cycle, `key/rule-on-instantiated-member`) is not counted; one whose
//! key is formed and carries a cause is.
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

/// The last step of mapping entity `e`'s key (`RuleTable::map`), with the
/// trace kept when the walk counts rule use.
pub(crate) fn map(w: &mut Walk<'_>, e: DeclId, key: Key) -> Key {
    let rules = w.rules;
    let Some(c) = w.rule_use.as_mut() else {
        return rules.map(key);
    };
    let (mapped, via) = rules.map_traced(key);
    let trace = match via {
        Some(r) => Some(Trace::Via(r)),
        None => {
            let direct: Vec<usize> = rules.producers(&mapped).collect();
            (!direct.is_empty()).then_some(Trace::Direct(direct))
        }
    };
    match trace {
        Some(t) => {
            c.traces.insert(e, t);
        }
        None => {
            c.traces.remove(&e);
        }
    }
    mapped
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
    by_rule
        .into_iter()
        .map(|(r, (via, direct))| Use { rule: r as u64, via, direct })
        .collect()
}
