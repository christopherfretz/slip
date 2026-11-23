//! Key uniqueness (WP-039 item 1; `spec/digest.md` §3.4) and display-name
//! uniqueness (WP-039 item 2; `verify.md` §6 `name`), checked on every
//! output the determinism test walks first (`determinism.rs`
//! `keys_and_names_are_unique`): both sides of every directory of
//! `differ`, `equal`, `demand` and `indeterminate`, pending ones included.
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
    // The five below differ only in an expression the key profiles
    // (§3.4: `function_disc`, `spec_disc`), and the expression area is
    // pending (`expr_dep::profile_expr`): both entities profile as
    // Pending(Expr) and get one key, which the walk marks a collision.
    // Each test is `# pending`, `# needs: expr`.
    (
        "differ/_/key-constraint-only",
        &["template <class> void f(type-parameter-0-0)"],
        1,
        "a trailing requires-clause is Pending(Expr)",
    ),
    (
        "differ/_/key-decltype-return-templates",
        &[
            "template <class> decltype(t.a) f(type-parameter-0-0)",
            "template <class> decltype(t.b) f(type-parameter-0-0)",
        ],
        1,
        "a decltype operand is Pending(Expr)",
    ),
    (
        "differ/_/key-dependent-array-bound-templates",
        &["template <int> void f(int (&)[N + 1])", "template <int> void f(int (&)[N])"],
        1,
        "a dependent array bound is Pending(Expr)",
    ),
    (
        "differ/_/key-partial-specs-by-constraint",
        &["S<type-parameter-0-0 *>"],
        1,
        "a requires-clause is Pending(Expr)",
    ),
    (
        "differ/_/key-type-constraint-only",
        &["template <class> void f(type-parameter-0-0)"],
        1,
        "a type-constraint is Pending(Expr)",
    ),
];

/// Display names two different units of one output are known to share:
/// (directory, name, how many keys share it, reason).
pub const SAME_NAME: &[(&str, &str, usize, &str)] = &[
    // Names leave a requires-clause out (it is an expression, and names
    // print no expressions): two members that differ in it alone, or
    // whose instantiated signatures meet, share a name.
    ("differ/FnSpec/0-not-selected", "X::~X<T>()", 2, "prospective destructors differ in requires"),
    (
        "differ/FnSpec/0-not-selected",
        "X<int>::~X()",
        2,
        "prospective destructors differ in requires",
    ),
    (
        "differ/_/key-member-of-template-overloads",
        "A<int>::f(int)",
        2,
        "f(T) requires C<T> and f(int) at T = int",
    ),
    // Kiln's argument printer (`write_arg_list`) writes a null pointer
    // argument as `nullptr` whatever its type.
    ("differ/_/key-null-pointer-typed", "N<nullptr>", 2, "N<nullptr> against N<(int *)nullptr>"),
];

/// Sides of the pair tests that are known not to digest: (directory,
/// side, reason). Any other side that does not digest fails the sweep,
/// which cannot check what it cannot see; an entry that digests fails
/// too, so the list only shrinks.
pub const UNPARSED: &[(&str, Side, &str)] = &[
    // Three `# pending` tests of other packages whose sources do not
    // compile (their runners skip them, so nothing else sees it).
    (
        "differ/DefinitionBits/0-lambda-captures",
        Side::B,
        "`[x]` captures a namespace-scope variable (WP-027's test)",
    ),
    (
        "differ/_/local-class-of-instantiated-function",
        Side::A,
        "`f<int>()` used before `f` is declared (WP-027's test)",
    ),
    (
        "differ/_/local-class-of-instantiated-function",
        Side::B,
        "`f<int>()` used before `f` is declared (WP-027's test)",
    ),
    (
        "demand/_/friend-template-definition-instantiated-on-use",
        Side::B,
        "`use` defined twice on side B (WP-032/033's test)",
    ),
];

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
