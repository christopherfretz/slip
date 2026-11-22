//! Must-differ pairs (`spec/digest.md` §14, `spec/digest-kinds.md` §1.5):
//! over declared and `tu` units, the keys whose units differ between the
//! sides are exactly the `changed`, `added` and `removed` lines; derived
//! units are free unless a `derived-changed` line names one.

mod common;

use std::collections::BTreeSet;
use std::path::Path;

use common::{Digested, Expect, Side};
use slip_model::digests::GroupClass;

/// The units of `d` under `key` that are not derived.
fn declared(d: &Digested, key: &slip_model::Digest) -> Vec<(GroupClass, slip_model::Digest, u64)> {
    d.units
        .get(key)
        .into_iter()
        .flatten()
        .filter(|u| u.class != GroupClass::Derived)
        .map(|u| (u.class, u.digest, u.flags))
        .collect()
}

fn derived(d: &Digested, key: &slip_model::Digest) -> Vec<(slip_model::Digest, u64)> {
    d.units
        .get(key)
        .into_iter()
        .flatten()
        .filter(|u| u.class == GroupClass::Derived)
        .map(|u| (u.digest, u.flags))
        .collect()
}

fn one(dir: &Path) -> Result<(), String> {
    let expect = common::expect(dir)?;
    if expect.is_empty() {
        return Err("a differ test needs at least one line in expect.txt".into());
    }
    let a = common::digest_side(dir, Side::A)?;
    let b = common::digest_side(dir, Side::B)?;
    let mut problems = Vec::new();
    for (side, d) in [("A", &a), ("B", &b)] {
        for u in common::hard_indeterminate(d) {
            problems.push(format!("side {side}: indeterminate {u}"));
        }
    }
    let keys: BTreeSet<_> = a.units.keys().chain(b.units.keys()).copied().collect();
    let mut got = BTreeSet::new();
    for k in &keys {
        let (da, db) = (declared(&a, k), declared(&b, k));
        let name = if a.units.contains_key(k) { a.name(k) } else { b.name(k) };
        match (da.is_empty(), db.is_empty()) {
            (true, true) => {}
            (false, true) => {
                got.insert(Expect::Removed(name.clone()));
            }
            (true, false) => {
                got.insert(Expect::Added(name.clone()));
            }
            (false, false) => {
                if da != db {
                    got.insert(Expect::Changed(name.clone()));
                }
            }
        }
        if expect.contains(&Expect::DerivedChanged(name.clone()))
            && derived(&a, k) != derived(&b, k)
        {
            got.insert(Expect::DerivedChanged(name));
        }
    }
    let want: BTreeSet<_> =
        expect.iter().filter(|e| !matches!(e, Expect::Indeterminate(_))).cloned().collect();
    if got != want {
        let missing: Vec<_> = want.difference(&got).collect();
        let extra: Vec<_> = got.difference(&want).collect();
        problems
            .push(format!("expected but not seen: {missing:?}\nseen but not expected: {extra:?}"));
    }
    if problems.is_empty() { Ok(()) } else { Err(problems.join("\n")) }
}

#[test]
fn differ() {
    common::run_all("differ", one);
}
