//! The declared changes of an edit set (`spec/edits.md` §6.2).

use std::collections::{BTreeMap, BTreeSet};

use slip_model::declared::Declared;
use slip_model::editset::{EditSet, SetState};
use slip_model::keys::{KeyKind, Rule, RuleTable};

/// §6.2's `rule_conflict`: two rules with one `(parent, kind, name, disc)`
/// and different targets, or two rules of a kind other than `namespace`
/// with one target `(to_parent, kind, to_name, disc)` and different
/// `(parent, name)`. `rules` holds no two equal rules.
pub(crate) fn conflicts(rules: &BTreeSet<Rule>) -> bool {
    // The derived order compares `(parent, kind, name, disc)` first, so
    // rules with one match are adjacent; being different, they conflict.
    let mut prev: Option<&Rule> = None;
    for r in rules {
        if let Some(p) = prev
            && (p.parent, p.kind, &p.name, p.disc) == (r.parent, r.kind, &r.name, r.disc)
        {
            return true;
        }
        prev = Some(r);
    }
    let mut targets = BTreeMap::new();
    for r in rules.iter().filter(|r| r.kind != KeyKind::Namespace) {
        let from = (r.parent, &r.name);
        if let Some(other) = targets.insert((r.to_parent, r.kind, &r.to_name, r.disc), from)
            && other != from
        {
            return true;
        }
    }
    false
}

/// The rules of `rules` whose match no other rule of `rules` has: what an
/// edit set's `rules` (a rule table) can hold of the shards' `renamed`
/// rules when two of them disagree (landed WP-062 §7).
pub(crate) fn table_of(rules: &BTreeSet<Rule>) -> Vec<Rule> {
    let list: Vec<&Rule> = rules.iter().collect();
    let same = |a: &Rule, b: &Rule| (a.parent, a.kind, &a.name, a.disc) == (b.parent, b.kind, &b.name, b.disc);
    let kept: Vec<Rule> = (0..list.len())
        .filter(|&i| {
            !((i > 0 && same(list[i - 1], list[i])) || (i + 1 < list.len() && same(list[i], list[i + 1])))
        })
        .map(|i| list[i].clone())
        .collect();
    match RuleTable::new(kept) {
        Ok(t) => t.rules().to_vec(),
        Err(e) => unreachable!("rules with distinct matches make a table: {e}"),
    }
}

/// The union of §6.2: the set's `rules` and the `rule` of every subject of
/// the set (the selected subjects and followers).
pub(crate) fn union_of(set: &EditSet) -> BTreeSet<Rule> {
    set.rules.iter().cloned().chain(set.subjects.iter().filter_map(|s| s.rule.clone())).collect()
}

/// §6.2, from an edit set alone: `None` for a refused one. `Err` if the set
/// does not validate, or its rules conflict (reduce refuses such a set).
pub fn declared(set: &EditSet) -> Result<Option<Declared>, String> {
    set.validate()?;
    if set.state == SetState::Refused {
        return Ok(None);
    }
    let rules = union_of(set);
    if conflicts(&rules) {
        return Err("the rules of an accepted edit set conflict (rule_conflict)".into());
    }
    let table = RuleTable::new(rules.into_iter().collect()).map_err(|e| e.to_string())?;
    let changed: BTreeSet<_> = set.changed.iter().filter(|c| c.in_force).map(|c| c.key).collect();
    Declared::new(table.rules().to_vec(), changed.into_iter().collect(), Vec::new(), Vec::new())
        .map(Some)
        .map_err(|e| e.to_string())
}
