//! Demand independence (`spec/digest.md` §14): every key in both sides has
//! the same digests and flags, and the declared keys only B has are
//! exactly the `added` lines (none if there are none); A has no declared
//! key B lacks. A key both sides declare is in the declared group of one
//! file on both, and a declared group that holds no key of one side only
//! has the same sum on both (`spec/digest.md` §11).

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use common::{Expect, Side};
use slip_model::Digest;
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
    if let Some(e) = expect.iter().find(|e| !matches!(e, Expect::Added(_))) {
        return Err(format!("{e:?}: a demand test has `added` lines only"));
    }
    let added: BTreeSet<Expect> = expect;
    let declared = |d: &common::Digested, other: &common::Digested| -> BTreeSet<String> {
        d.units
            .iter()
            .filter(|(k, v)| {
                !other.units.contains_key(*k) && v.iter().any(|u| u.class == GroupClass::Declared)
            })
            .map(|(k, _)| d.name(k))
            .collect()
    };
    let got: BTreeSet<Expect> = declared(&b, &a).into_iter().map(Expect::Added).collect();
    if got != added {
        problems.push(format!("declared keys only B has: expected {added:?}, saw {got:?}"));
    }
    let only_a = declared(&a, &b);
    if !only_a.is_empty() {
        problems.push(format!("declared keys only A has: {only_a:?}"));
    }
    let (fa, fb) = (declared_files(&a), declared_files(&b));
    for (k, f) in &fa {
        if let Some(g) = fb.get(k)
            && f != g
        {
            problems.push(format!("declared in another file: {}", a.name(k)));
        }
    }
    let one_side: BTreeSet<&Vec<u8>> = fa
        .iter()
        .filter(|(k, _)| !fb.contains_key(*k))
        .chain(fb.iter().filter(|(k, _)| !fa.contains_key(*k)))
        .map(|(_, f)| f)
        .collect();
    let sums = |d: &common::Digested| -> BTreeMap<Vec<u8>, (u64, u64, Digest)> {
        d.output
            .digests
            .groups
            .iter()
            .filter(|g| g.class == GroupClass::Declared && !one_side.contains(&g.file))
            .map(|g| (g.file.clone(), (g.count, g.indeterminate, g.digest)))
            .collect()
    };
    if sums(&a) != sums(&b) {
        problems.push("a declared group's sum differs".into());
    }
    if problems.is_empty() { Ok(()) } else { Err(problems.join("\n")) }
}

/// The file of the declared group that holds each declared key.
fn declared_files(d: &common::Digested) -> BTreeMap<Digest, Vec<u8>> {
    let mut out = BTreeMap::new();
    for g in d.output.digests.groups.iter().filter(|g| g.class == GroupClass::Declared) {
        for u in g.units.iter().flatten() {
            out.insert(u.key, g.file.clone());
        }
    }
    out
}

#[test]
fn demand() {
    common::run_all("demand", one);
}
