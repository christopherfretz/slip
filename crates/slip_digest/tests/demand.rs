//! Demand independence (`spec/digest.md` §14): every key in both sides has
//! the same digests and flags; `added` lines, if any, are exactly the
//! declared keys only B has.

mod common;

use std::collections::BTreeSet;
use std::path::Path;

use common::{Expect, Side};
use slip_model::digests::GroupClass;

fn one(dir: &Path) -> Result<(), String> {
    let expect = common::expect(dir)?;
    let a = common::digest_side(dir, Side::A)?;
    let b = common::digest_side(dir, Side::B)?;
    let mut problems = Vec::new();
    for (side, d) in [("A", &a), ("B", &b)] {
        for u in common::hard_indeterminate(d) {
            problems.push(format!("side {side}: indeterminate {u}"));
        }
    }
    for (k, ua) in &a.units {
        if let Some(ub) = b.units.get(k)
            && ua != ub
        {
            problems.push(format!("differs: {}", a.name(k)));
        }
    }
    let added: BTreeSet<Expect> =
        expect.iter().filter(|e| matches!(e, Expect::Added(_))).cloned().collect();
    if !added.is_empty() {
        let got: BTreeSet<Expect> = b
            .units
            .iter()
            .filter(|(k, v)| {
                !a.units.contains_key(*k) && v.iter().any(|u| u.class == GroupClass::Declared)
            })
            .map(|(k, _)| Expect::Added(b.name(k)))
            .collect();
        if got != added {
            problems.push(format!("added: expected {added:?}, saw {got:?}"));
        }
    }
    if problems.is_empty() { Ok(()) } else { Err(problems.join("\n")) }
}

#[test]
fn demand() {
    common::run_all("demand", one);
}
