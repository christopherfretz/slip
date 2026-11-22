//! The judgments of the pair runners (`spec/digest.md` §14,
//! `spec/digest-kinds.md` §1.5, `spec/landed/harness/WP-047.md`), apart
//! from the runners so that `tests/runner.rs` can put them to its own
//! fixture directories: `differ`, `demand` and `equal` over one
//! directory, and the pure checks under them over two digested sides.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use slip_model::Digest;
use slip_model::digests::GroupClass;

use super::{Digested, Expect, Side, derived_differences, digest_side, exact_derived, expect};

/// The units of `d` under `key` that are not derived.
fn declared(d: &Digested, key: &Digest) -> Vec<(GroupClass, Digest, u64)> {
    d.units
        .get(key)
        .into_iter()
        .flatten()
        .filter(|u| u.class != GroupClass::Derived)
        .map(|u| (u.class, u.digest, u.flags))
        .collect()
}

/// Whether a `derived-*` line holds without `exact derived`, given the
/// derived differences `dd`: `derived-added` and `derived-removed` as
/// written; `derived-changed` when a key of its name differs in any way,
/// presence included (its meaning before WP-047, kept for open
/// branches).
fn holds_loosely(line: &Expect, dd: &BTreeSet<Expect>) -> bool {
    match line {
        Expect::DerivedAdded(_) | Expect::DerivedRemoved(_) => dd.contains(line),
        Expect::DerivedChanged(n) => {
            dd.contains(&Expect::DerivedAdded(n.clone()))
                || dd.contains(&Expect::DerivedRemoved(n.clone()))
                || dd.contains(&Expect::DerivedChanged(n.clone()))
        }
        _ => false,
    }
}

/// The side errors every pair runner reports: units indeterminate for a
/// cause other than `Pending`.
fn hard(a: &Digested, b: &Digested) -> Vec<String> {
    let mut problems = Vec::new();
    for (side, d) in [("A", a), ("B", b)] {
        for u in super::hard_indeterminate(d) {
            problems.push(format!("side {side}: indeterminate {u}"));
        }
    }
    problems
}

/// A must-differ test (`tests/differ.rs`): over declared and `tu` units,
/// the keys whose units differ are exactly the `changed`, `added` and
/// `removed` lines; every `derived-added` / `derived-removed` line holds;
/// a `derived-changed` line holds when a key of its name differs in any
/// way; with `exact derived` the derived differences are exactly the
/// `derived-*` lines, `derived-changed` meaning both sides have it.
///
/// `Ok` holds the derived differences no line names, each as the line
/// that would name it (for `SLIP_DIGEST_DERIVED=report`); one a
/// `derived-changed` line covers only loosely ends in
/// ` (written derived-changed)`.
pub fn differ(dir: &Path) -> Result<Vec<String>, String> {
    let expect = expect(dir)?;
    let exact = exact_derived(dir)?;
    if expect.is_empty() {
        return Err("a differ test needs at least one line in expect.txt".into());
    }
    if let Some(Expect::Indeterminate(n)) =
        expect.iter().find(|e| matches!(e, Expect::Indeterminate(_)))
    {
        return Err(format!(
            "`indeterminate {n}`: a differ test asserts no indeterminate unit; it belongs under tests/indeterminate"
        ));
    }
    let a = digest_side(dir, Side::A)?;
    let b = digest_side(dir, Side::B)?;
    judge_differ(&expect, exact, &a, &b)
}

/// [`differ`] over two digested sides.
pub fn judge_differ(
    expect: &BTreeSet<Expect>,
    exact: bool,
    a: &Digested,
    b: &Digested,
) -> Result<Vec<String>, String> {
    let mut problems = hard(a, b);
    let keys: BTreeSet<_> = a.units.keys().chain(b.units.keys()).copied().collect();
    let mut got = BTreeSet::new();
    for k in &keys {
        let (da, db) = (declared(a, k), declared(b, k));
        let name = if a.units.contains_key(k) { a.name(k) } else { b.name(k) };
        match (da.is_empty(), db.is_empty()) {
            (true, true) => {}
            (false, true) => {
                got.insert(Expect::Removed(name));
            }
            (true, false) => {
                got.insert(Expect::Added(name));
            }
            (false, false) => {
                if da != db {
                    got.insert(Expect::Changed(name));
                }
            }
        }
    }
    let dd = derived_differences(a, b);
    if exact {
        got.extend(dd.iter().cloned());
    } else {
        got.extend(expect.iter().filter(|e| holds_loosely(e, &dd)).cloned());
    }
    if got != *expect {
        let missing: Vec<_> = expect.difference(&got).collect();
        let extra: Vec<_> = got.difference(expect).collect();
        problems
            .push(format!("expected but not seen: {missing:?}\nseen but not expected: {extra:?}"));
    }
    if !problems.is_empty() {
        return Err(problems.join("\n"));
    }
    let loose: BTreeSet<&str> = expect
        .iter()
        .filter_map(|e| if let Expect::DerivedChanged(n) = e { Some(n.as_str()) } else { None })
        .collect();
    Ok(dd
        .iter()
        .filter(|e| !expect.contains(e))
        .map(|e| {
            if loose.contains(e.name()) {
                format!("{} (written derived-changed)", e.line())
            } else {
                e.line()
            }
        })
        .collect())
}

/// A demand-independence test (`tests/demand.rs`, `spec/digest.md` §14):
/// every key in both sides has the same units; the declared keys only B
/// has are exactly the `added` lines and A has none B lacks; a key both
/// sides declare is in the declared group of one file on both, and a
/// declared group that holds no key of one side only has the same sum on
/// both (§11). Every `derived-added` line holds, and with `exact derived`
/// the derived differences are exactly those lines.
pub fn demand(dir: &Path) -> Result<(), String> {
    let expect = expect(dir)?;
    let exact = exact_derived(dir)?;
    let a = digest_side(dir, Side::A)?;
    let b = digest_side(dir, Side::B)?;
    judge_demand(&expect, exact, &a, &b)
}

/// [`demand`] over two digested sides.
pub fn judge_demand(
    expect: &BTreeSet<Expect>,
    exact: bool,
    a: &Digested,
    b: &Digested,
) -> Result<(), String> {
    let mut problems = hard(a, b);
    for (k, ua) in &a.units {
        if let Some(ub) = b.units.get(k)
            && ua != ub
        {
            problems.push(format!("differs: {}", a.name(k)));
        }
    }
    if let Some(e) =
        expect.iter().find(|e| !matches!(e, Expect::Added(_) | Expect::DerivedAdded(_)))
    {
        return Err(format!("{e:?}: a demand test has `added` and `derived-added` lines only"));
    }
    let added: BTreeSet<Expect> =
        expect.iter().filter(|e| matches!(e, Expect::Added(_))).cloned().collect();
    let declared = |d: &Digested, other: &Digested| -> BTreeSet<String> {
        d.units
            .iter()
            .filter(|(k, v)| {
                !other.units.contains_key(*k) && v.iter().any(|u| u.class == GroupClass::Declared)
            })
            .map(|(k, _)| d.name(k))
            .collect()
    };
    let got: BTreeSet<Expect> = declared(b, a).into_iter().map(Expect::Added).collect();
    if got != added {
        problems.push(format!("declared keys only B has: expected {added:?}, saw {got:?}"));
    }
    let only_a = declared(a, b);
    if !only_a.is_empty() {
        problems.push(format!("declared keys only A has: {only_a:?}"));
    }
    let want_derived: BTreeSet<Expect> =
        expect.iter().filter(|e| matches!(e, Expect::DerivedAdded(_))).cloned().collect();
    let dd = derived_differences(a, b);
    let got_derived: BTreeSet<Expect> =
        if exact { dd } else { want_derived.iter().filter(|e| dd.contains(e)).cloned().collect() };
    if got_derived != want_derived {
        let missing: Vec<_> = want_derived.difference(&got_derived).collect();
        let extra: Vec<_> = got_derived.difference(&want_derived).collect();
        problems.push(format!(
            "derived: expected but not seen: {missing:?}\nseen but not expected: {extra:?}"
        ));
    }
    let (fa, fb) = (declared_files(a), declared_files(b));
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
    let sums = |d: &Digested| -> BTreeMap<Vec<u8>, (u64, u64, Digest)> {
        d.output
            .digests
            .groups
            .iter()
            .filter(|g| g.class == GroupClass::Declared && !one_side.contains(&g.file))
            .map(|g| (g.file.clone(), (g.count, g.indeterminate, g.digest)))
            .collect()
    };
    if sums(a) != sums(b) {
        problems.push("a declared group's sum differs".into());
    }
    if problems.is_empty() { Ok(()) } else { Err(problems.join("\n")) }
}

/// The file of the declared group that holds each declared key.
fn declared_files(d: &Digested) -> BTreeMap<Digest, Vec<u8>> {
    let mut out = BTreeMap::new();
    for g in d.output.digests.groups.iter().filter(|g| g.class == GroupClass::Declared) {
        for u in g.units.iter().flatten() {
            out.insert(u.key, g.file.clone());
        }
    }
    out
}

/// The error of an `equal` or `indeterminate` test whose `expect.txt`
/// has a `derived-*` line or `exact derived`: every unit both sides of
/// an equal test have is equal already, and a derived unit only one side
/// has is a failure, not something to name (`spec/digest.md` §14).
pub fn no_derived_lines(dir: &Path, what: &str) -> Result<(), String> {
    if exact_derived(dir)? {
        return Err(format!("`exact derived`: {what} has no derived lines"));
    }
    if let Some(e) = expect(dir)?.iter().find(|e| e.is_derived()) {
        return Err(format!("`{}`: {what} has no derived lines", e.line()));
    }
    Ok(())
}

/// A must-equal test (`tests/equal.rs`, `spec/digest.md` §14): the two
/// sides' `Digests` are equal with `Detail::All`. A unit, declared or
/// derived, that only one side has makes them differ.
pub fn equal(dir: &Path) -> Result<(), String> {
    no_derived_lines(dir, "an equal test")?;
    let a = digest_side(dir, Side::A)?;
    let b = digest_side(dir, Side::B)?;
    judge_equal(&a, &b)
}

/// [`equal`] over two digested sides.
pub fn judge_equal(a: &Digested, b: &Digested) -> Result<(), String> {
    let mut problems = hard(a, b);
    if a.output.digests != b.output.digests {
        for (k, ua) in &a.units {
            match b.units.get(k) {
                Some(ub) if ua == ub => {}
                Some(_) => problems.push(format!("differs: {}", a.name(k))),
                None => problems.push(format!("only in A: {}", a.name(k))),
            }
        }
        for k in b.units.keys() {
            if !a.units.contains_key(k) {
                problems.push(format!("only in B: {}", b.name(k)));
            }
        }
        if problems.is_empty() {
            problems.push("the digests differ (groups or files)".into());
        }
    }
    if problems.is_empty() { Ok(()) } else { Err(problems.join("\n")) }
}
