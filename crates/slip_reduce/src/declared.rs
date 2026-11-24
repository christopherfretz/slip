//! The declared changes of an edit set (`spec/edits.md` §6.2).

use std::collections::{BTreeMap, BTreeSet};

use slip_model::declared::Declared;
use slip_model::editset::{EditSet, SetState};
use slip_model::keys::{Rule, RuleTable};
use slip_model::transform::Ownership;

/// §6.2's `rule_conflict`: two rules with one `(parent, kind, name, disc)`
/// and different targets, or two rules of any kind, namespaces included,
/// with one target `(to_parent, kind, to_name, disc)` and different
/// `(parent, name)` (`spec/verify.md` §2.1, "One way to each key").
/// `rules` holds no two equal rules.
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
    for r in rules {
        let from = (r.parent, &r.name);
        if let Some(other) = targets.insert((r.to_parent, r.kind, &r.to_name, r.disc), from)
            && other != from
        {
            return true;
        }
    }
    false
}

/// The rules of `rules` whose match no other rule of `rules` has and
/// whose target no other rule of `rules` has: what an edit set's `rules`
/// (a rule table) can hold of the shards' `renamed` rules when two of them
/// disagree (landed WP-062 §7; `spec/verify.md` §2.1, "One way to each
/// key"). Such a set is refused with `rule_conflict` ([`conflicts`]).
pub(crate) fn table_of(rules: &BTreeSet<Rule>) -> Vec<Rule> {
    let mut matches: BTreeMap<_, usize> = BTreeMap::new();
    let mut targets: BTreeMap<_, usize> = BTreeMap::new();
    for r in rules {
        *matches.entry((r.parent, r.kind, &r.name, r.disc)).or_default() += 1;
        *targets.entry((r.to_parent, r.kind, &r.to_name, r.disc)).or_default() += 1;
    }
    let kept: Vec<Rule> = rules
        .iter()
        .filter(|r| {
            matches[&(r.parent, r.kind, &r.name, r.disc)] == 1
                && targets[&(r.to_parent, r.kind, &r.to_name, r.disc)] == 1
        })
        .cloned()
        .collect();
    // One rule per match and per target, each with names the shard's
    // reader accepted: `RuleTable::new` has nothing to refuse. Were it to
    // refuse, the set keeps no rule rather than panic, and [`declared`]
    // makes the table of the whole union again and fails with the error.
    RuleTable::new(kept).map(|t| t.rules().to_vec()).unwrap_or_default()
}

/// The union of §6.2: the set's `rules` and the `rule` of every subject of
/// the set (the selected subjects and followers).
pub(crate) fn union_of(set: &EditSet) -> BTreeSet<Rule> {
    set.rules.iter().cloned().chain(set.subjects.iter().filter_map(|s| s.rule.clone())).collect()
}

/// §6.2, from an edit set and the request's `ownership`: `None` for a
/// refused one. `Err` if the set does not validate, or its rules conflict
/// (reduce refuses such a set). A `Declared` with rules holds the run's
/// files, which are the request's ownership (`spec/verify.md` §2,
/// WP-110); one without rules holds none.
pub fn declared(set: &EditSet, ownership: &Ownership) -> Result<Option<Declared>, String> {
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
    let files = (!table.is_empty()).then(|| ownership.clone());
    Declared::new(table.rules().to_vec(), changed.into_iter().collect(), Vec::new(), Vec::new())
        .map(|d| Some(d.with_files(files)))
        .map_err(|e| e.to_string())
}
