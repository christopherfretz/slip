//! The names digest (`spec/digest.md` §9, WP-112 item 5) beside the pairs:
//! the three cases the pair runners cannot say by themselves, because they
//! speak of a names entry's presence on both sides, not only of a change.
//!
//! - a function renamed under its rule: units equal, names differ (the
//!   string holds the new name);
//! - an edit that changes no name-derived string: both sides have the
//!   entry, and it is equal although the units differ;
//! - a string read at compile time: written by value, so no side has an
//!   entry, and the units differ.

mod common;

use std::path::{Path, PathBuf};

use common::{Digested, Side, digest_side};
use slip_model::Digest;

fn pair(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join(rel)
}

fn sides(rel: &str) -> (Digested, Digested) {
    let dir = pair(rel);
    let a = digest_side(&dir, Side::A).unwrap_or_else(|e| panic!("{rel} a: {e}"));
    let b = digest_side(&dir, Side::B).unwrap_or_else(|e| panic!("{rel} b: {e}"));
    (a, b)
}

/// The key of the unit called `name` (one unit of that name).
fn key(d: &Digested, name: &str) -> Digest {
    let keys: Vec<_> = d
        .units
        .iter()
        .filter(|(_, us)| us.iter().any(|u| u.name == name))
        .map(|(k, _)| *k)
        .collect();
    assert_eq!(keys.len(), 1, "one unit called {name}: {:?}", d.units.values().collect::<Vec<_>>());
    keys[0]
}

/// The unit digests of `name`'s key.
fn units(d: &Digested, name: &str) -> Vec<Digest> {
    d.units[&key(d, name)].iter().map(|u| u.digest).collect()
}

/// The names digest of `name`'s key, if it has one.
fn names(d: &Digested, name: &str) -> Option<Digest> {
    let k = key(d, name);
    d.output.name_strings.iter().find(|n| n.key == k).map(|n| n.names)
}

#[test]
fn renamed_function_under_its_rule_names_differ() {
    let (a, b) = sides("equal/_/name-value-renamed-function");
    // Side B's `start` is digested under the rule renaming it from `f`:
    // it has side A's `f`'s key.
    let k = key(&a, "f(int)");
    let digests = |d: &Digested| d.units[&k].iter().map(|u| u.digest).collect::<Vec<_>>();
    assert_eq!(digests(&a), digests(&b));
    let names = |d: &Digested| d.output.name_strings.iter().find(|n| n.key == k).map(|n| n.names);
    let (na, nb) = (names(&a), names(&b));
    assert!(na.is_some() && nb.is_some(), "both sides have an entry");
    assert_ne!(na, nb, "the string holds the new name");
}

#[test]
fn unrelated_edit_names_equal() {
    let (a, b) = sides("differ/_/name-value-names-unrelated-edit");
    for f in ["f(int)", "g(int)"] {
        assert_ne!(units(&a, f), units(&b, f), "{f}: the body changed");
        let (na, nb) = (names(&a, f), names(&b, f));
        assert!(na.is_some(), "{f}: an entry");
        assert_eq!(na, nb, "{f}: no name-derived string changed");
    }
}

/// A string passed on through an `initializer_list`'s backing array (a
/// pointer to its first element, which holds `__PRETTY_FUNCTION__`): the
/// entry exists on both sides and holds the new name. (Written to reach
/// a constant's own names digest, the `uv 4` entry; it does not: Kiln
/// gives this pointer no anonymous-constant base. See the landed file,
/// mutation M8.)
#[test]
fn passed_through_a_constant_names_differ() {
    let (a, b) = sides("equal/_/name-value-pointer-into-backing-array");
    let k = key(&a, "legacy::f()");
    let digests = |d: &Digested| d.units[&k].iter().map(|u| u.digest).collect::<Vec<_>>();
    assert_eq!(digests(&a), digests(&b));
    let names = |d: &Digested| d.output.name_strings.iter().find(|n| n.key == k).map(|n| n.names);
    let (na, nb) = (names(&a), names(&b));
    assert!(na.is_some() && nb.is_some(), "both sides have an entry: {na:?} {nb:?}");
    assert_ne!(na, nb, "the string holds the new name");
}

#[test]
fn read_at_compile_time_no_entry() {
    let (a, b) = sides("differ/_/name-value-static-assert-characters");
    assert_ne!(units(&a, "legacy::f()"), units(&b, "acme::f()"));
    assert_eq!(names(&a, "legacy::f()"), None);
    assert_eq!(names(&b, "acme::f()"), None);
    assert!(a.output.name_strings.is_empty() && b.output.name_strings.is_empty());
}

/// The keys of a pair noted by the names digest (an entry on both sides
/// that differs), each with whether its units are equal.
fn notes(a: &Digested, b: &Digested) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    for x in &a.output.name_strings {
        let Some(y) = b.output.name_strings.iter().find(|y| y.key == x.key) else { continue };
        if x.names != y.names {
            let ua: Vec<Digest> = a.units[&x.key].iter().map(|u| u.digest).collect();
            let ub: Vec<Digest> = b.units[&x.key].iter().map(|u| u.digest).collect();
            out.push((a.units[&x.key][0].name.clone(), ua == ub));
        }
    }
    out.sort();
    out
}

// WP-115's six pending cost pairs (`spec/landed/digest/WP-115.md` item 1),
// as the names digest sees them. A string the cost moves to its bytes is
// written by value and not fed, so the holder's names digest changes with
// its digest: a note that may say nothing changed, but only beside a
// `changed` of the same key (units differ: `false`). A note on a key whose
// units are equal (`true`) is a real change of the string, by the rule.
// No string change goes unseen: a string written as bytes is in the unit's
// digest.
#[test]
fn wp115_costs_note_beside_a_finding_or_a_real_change() {
    let cases: [(&str, &[(&str, bool)]); 6] = [
        // `f(int)` renamed from `g`: "f" to "g" (and the cost); `f(long)`'s
        // "f" moves between bytes and its own form: beside its `changed`.
        ("equal/_/name-value-cost-overload-plain-rename", &[("f(int)", false), ("f(long)", false)]),
        ("equal/_/name-value-plain-default-argument-holder", &[("x::h()", false)]),
        ("equal/_/name-value-holder-own-lambda", &[("legacy::f()", false)]),
        // The relocation changes both pretty names; the units are equal.
        (
            "equal/_/name-value-cost-namespace-lambdas-shared-pretty",
            &[("legacy::G", true), ("legacy::H", true)],
        ),
        ("differ/_/name-value-cost-plain-added-earlier", &[("b::get()", false)]),
        // `f(int)` renamed to `q`: a real change, units equal; `f(long)`
        // beside its `changed`.
        (
            "equal/_/name-value-cost-shared-plain-rename-other",
            &[("f(int)", true), ("f(long)", false)],
        ),
    ];
    for (rel, want) in cases {
        let (a, b) = sides(rel);
        let want: Vec<(String, bool)> = want.iter().map(|(n, e)| ((*n).to_owned(), *e)).collect();
        assert_eq!(notes(&a, &b), want, "{rel}");
    }
}
