//! Key uniqueness (WP-039 item 1; `spec/digest.md` §3.4) and display-name
//! uniqueness (WP-039 item 2; `verify.md` §6 `name`), checked on every
//! output the determinism test walks first (`determinism.rs`
//! `keys_and_names_are_unique`): both sides of every directory of the
//! roots `determinism.rs` walks (`ROOTS`: the rules pairs too, since the
//! WP-158 fix round), pending ones included.
//!
//! Two different entities with one key are the worst defect the digest
//! can have (a false "same"). The walk marks such a key `Collision`
//! (`units::mark_collisions`) and counts it (`digest.collisions`); this
//! fails for every counted collision, and for a key that holds two
//! different digests in one output whatever the walk says. Two different
//! keys with one display name make a verify report ambiguous and an
//! `expect.txt` line name two units; that fails too, except for the
//! content-keyed units whose content has no printed form ([`OPAQUE`]).
//!
//! What is known and accepted is listed, per directory and unit, with its
//! reason ([`SAME_KEY`], [`SAME_NAME`]), and so is every side that does not
//! digest ([`UNPARSED`]); an entry that no longer matches fails, so the
//! lists only shrink. The sweep walks at least [`MIN_DIRS`] directories.

use super::common::{Digested, Side};
use slip_digest::Cause;
use std::collections::{BTreeMap, BTreeSet};

/// Directories whose outputs are known to hold a key collision:
/// (directory, the display names of the units marked colliding, the
/// count `digest.collisions`, reason). An entry matches a side only when
/// both are exactly these, so a new collision in a listed directory still
/// fails. (A unit that was indeterminate before the collision keeps its
/// first cause, so it is not marked and only the count names it.)
/// Each is a collision the walk marks (the units are indeterminate),
/// caused by an area still pending or by what the test is about.
pub const SAME_KEY: &[(&str, &[&str], u64, &str)] = &[
    (
        "indeterminate/DeclFlags/0-invalid",
        &[],
        1,
        "an invalid redeclaration is an entity of its own in Kiln; the test is about it",
    ),
    // A lambda in a signature makes each declaration a template of its
    // own in Kiln ([temp.over.link]/5: never equivalent); the digest
    // writes one key for both, whose bytes are the lambda's spelling
    // (digest §4.4 "Kinds walked in full"). The walk marks the collision:
    // the test is about it.
    (
        "indeterminate/_/lambda-in-signature-twice",
        &[],
        1,
        "a lambda in a signature: two templates, one key; the test is about it",
    ),
    // Walked since the WP-158 fix round (F2), with what the pairs expect.
    (
        "rules/inert/unnamed-ordinal",
        &[],
        1,
        "the ordinal of an unnamed type of a class template is not found (a follow-up of \
         WP-131); the pair expects the collision on both sides",
    ),
    (
        "rules/wp158/friend-twice-one-class-rev",
        &["S::friend fr(X, D *)"],
        1,
        "a friend defined and then declared again in one class template: the two \
         declarations collide and fail closed (spec/digest.md §2.3, a friend definition \
         not instantiated); the pair is about it",
    ),
];

/// Display names two different units of one output are known to share:
/// (directory, name, how many keys share it, reason). Names print a
/// trailing requires-clause and a typed null pointer argument's type
/// (`spec/verify.md` §6.1), which separated the shared names of the other
/// roots; the rules pairs, walked since the WP-158 fix round (F2), had
/// these.
pub const SAME_NAME: &[(&str, &str, usize, &str)] = &[
    (
        "rules/disc/function-template-relocated-concept",
        "f<int>(int)",
        2,
        TWO_SPECS,
    ),
    (
        "rules/disc/function-template-relocated-concept-renamed",
        "f<int>(int)",
        2,
        TWO_SPECS,
    ),
];

/// Why the `disc` pairs share a name: found when the rules pairs were first
/// walked here (WP-158 fix round, F2), pre-existing, not yet ruled on.
const TWO_SPECS: &str = "two function templates `f`, one with a type-constraint: deduction \
     made a specialisation of each for `int`, and a specialisation's name prints its \
     signature, not its template's constraint";

/// Sides of the pair tests that are known not to digest: (directory,
/// side, reason). Any other side that does not digest fails the sweep,
/// which cannot check what it cannot see; an entry that digests fails
/// too, so the list only shrinks.
pub const UNPARSED: &[(&str, Side, &str)] = &[
    // WP-048's `# pending` tests of module units: a side whose header
    // names one (`// module:`) is refused until the snippet front end can
    // import a named module from memory (docs/requests/KF5).
    ("differ/Owner/unit", Side::A, KF5),
    ("differ/Owner/unit", Side::B, KF5),
    ("differ/UnitInfo/imports", Side::A, KF5),
    ("differ/UnitInfo/imports", Side::B, KF5),
    ("differ/UnitKind/Header", Side::A, KF5),
    ("differ/UnitKind/Header", Side::B, KF5),
    ("differ/UnitKind/Implementation", Side::A, KF5),
    ("differ/UnitKind/Implementation", Side::B, KF5),
    ("differ/_/module-explicit-def-across-units", Side::A, KF5),
    ("differ/_/module-explicit-def-across-units", Side::B, KF5),
    ("differ/_/module-import-added", Side::A, KF5),
    ("differ/_/module-import-added", Side::B, KF5),
    ("differ/_/module-imported-unit-as-main", Side::A, KF5),
    ("equal/_/module-linkage-one-name-two-modules", Side::A, KF5),
    ("equal/_/module-linkage-one-name-two-modules", Side::B, KF5),
];

/// Why WP-048's pending module tests do not digest.
const KF5: &str = "a module unit from memory needs Kiln (docs/requests/KF5)";

/// The fewest directories a sweep walks: a floor, so that it cannot pass
/// on nothing (a moved root, a broken directory filter).
pub const MIN_DIRS: usize = 2100;

/// The content-keyed units whose display name is the word alone
/// (`names.rs` `content_word`): two of them in one scope share it.
pub const OPAQUE: &[&str] = &["<static_assert>", "<asm>", "<consteval_block>"];

/// What [`check`] finds in one output.
#[derive(Clone, Debug)]
pub enum Finding {
    /// A problem.
    Problem(String),
    /// An entry of [`SAME_KEY`] (by index) that matched.
    KnownKey(usize),
    /// An entry of [`SAME_NAME`] (by index) that matched.
    KnownName(usize),
    /// An entry of [`UNPARSED`] (by index) that matched.
    KnownUnparsed(usize),
}

/// The finding for side `side` of directory `rel`, which did not digest
/// (`error`: why).
pub fn unparsed(rel: &str, side: Side, error: &str) -> Finding {
    match UNPARSED.iter().position(|(r, s, _)| *r == rel && *s == side) {
        Some(i) => Finding::KnownUnparsed(i),
        None => Finding::Problem(format!(
            "{rel} side {side:?} does not digest, and UNPARSED does not list it: {}",
            error.lines().take(2).collect::<Vec<_>>().join(" ")
        )),
    }
}

/// The findings of one digested side of directory `rel`.
pub fn check(rel: &str, side: Side, d: &Digested) -> Vec<Finding> {
    let mut problems = Vec::new();
    let names: BTreeMap<_, Vec<&String>> =
        d.output.names.iter().fold(BTreeMap::new(), |mut m, (k, n)| {
            m.entry(*k).or_insert_with(Vec::new).push(n);
            m
        });
    // The display names of the colliding units, whichever way found.
    let mut colliding: BTreeSet<&str> = BTreeSet::new();
    let mut add = |k| {
        for n in names.get(k).into_iter().flatten() {
            colliding.insert(n.as_str());
        }
    };
    // The counter counts every colliding key, whatever cause its units
    // report first (a unit already indeterminate keeps its first cause).
    let counted =
        d.output.counters.iter().find(|c| c.name == "digest.collisions").map_or(0, |c| c.value);
    if counted > 0 {
        let marked: Vec<_> = d
            .output
            .causes
            .iter()
            .filter(|(_, c)| *c == Cause::Collision)
            .map(|(k, _)| k)
            .collect();
        for k in &marked {
            add(*k);
        }
        let marked: Vec<_> = marked.iter().map(|k| names.get(*k)).collect();
        problems.push(format!("digest.collisions = {counted}; marked: {marked:?}"));
    }
    for (k, units) in &d.units {
        let digests: BTreeSet<_> = units.iter().map(|u| u.digest).collect();
        let marked = d.output.causes.iter().any(|(x, c)| x == k && *c == Cause::Collision);
        if digests.len() > 1 && !marked {
            add(k);
            problems.push(format!(
                "one key, {} digests, not marked a collision: {:?}",
                digests.len(),
                names.get(k)
            ));
        }
    }
    let mut out = Vec::new();
    if !problems.is_empty() {
        let known = SAME_KEY.iter().position(|(r, n, c, _)| {
            *r == rel && *c == counted && n.iter().copied().collect::<BTreeSet<_>>() == colliding
        });
        match known {
            Some(i) => out.push(Finding::KnownKey(i)),
            None => out.extend(problems.into_iter().map(|p| {
                Finding::Problem(format!("{rel} side {side:?}: {p}; colliding names {colliding:?}"))
            })),
        }
    }
    let mut by_name: BTreeMap<&str, BTreeSet<_>> = BTreeMap::new();
    for (k, n) in &d.output.names {
        by_name.entry(n.as_str()).or_default().insert(*k);
    }
    for (n, keys) in by_name {
        if keys.len() < 2 || OPAQUE.iter().any(|w| n.ends_with(w)) {
            continue;
        }
        match SAME_NAME.iter().position(|(r, s, c, _)| *r == rel && *s == n && *c == keys.len()) {
            Some(i) => out.push(Finding::KnownName(i)),
            None => out.push(Finding::Problem(format!(
                "{rel} side {side:?}: {} units named `{n}`",
                keys.len()
            ))),
        }
    }
    out
}

/// The problems of a whole sweep: every finding that is one, and every
/// entry of the lists that matched nothing.
pub fn problems<'a>(all: impl Iterator<Item = &'a Finding>) -> Vec<String> {
    let (mut keys, mut names, mut unparsed) = (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
    let mut out = Vec::new();
    for f in all {
        match f {
            Finding::Problem(p) => out.push(p.clone()),
            Finding::KnownKey(i) => {
                keys.insert(*i);
            }
            Finding::KnownName(i) => {
                names.insert(*i);
            }
            Finding::KnownUnparsed(i) => {
                unparsed.insert(*i);
            }
        }
    }
    for (i, (r, n, _, _)) in SAME_KEY.iter().enumerate() {
        if !keys.contains(&i) {
            out.push(format!(
                "SAME_KEY lists {r} ({n:?}), whose outputs no longer hold that collision"
            ));
        }
    }
    for (i, (r, n, c, _)) in SAME_NAME.iter().enumerate() {
        if !names.contains(&i) {
            out.push(format!("SAME_NAME lists `{n}` ×{c} in {r}, which no longer has it so"));
        }
    }
    for (i, (r, s, _)) in UNPARSED.iter().enumerate() {
        if !unparsed.contains(&i) {
            out.push(format!("UNPARSED lists {r} side {s:?}, which digests"));
        }
    }
    out
}
