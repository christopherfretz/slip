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
//! What is known and accepted is listed, per directory, with its reason
//! ([`SAME_KEY`], [`SAME_NAME`]); an entry that no longer matches fails,
//! so the lists only shrink.

use super::common::{Digested, Side};
use slip_digest::Cause;
use std::collections::{BTreeMap, BTreeSet};

/// Directories whose outputs are known to hold a key collision, with the
/// reason: each is a collision the walk marks (the units are
/// indeterminate), caused by an area still pending or by what the test
/// is about.
pub const SAME_KEY: &[(&str, &str)] = &[(
    "indeterminate/DeclFlags/0-invalid",
    "an invalid redeclaration is an entity of its own in Kiln; the test is about it",
)];

/// Display names two different units of one output are known to share:
/// (directory, name, reason).
pub const SAME_NAME: &[(&str, &str, &str)] = &[];

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
}

/// The findings of one digested side of directory `rel`.
pub fn check(rel: &str, side: Side, d: &Digested) -> Vec<Finding> {
    let mut problems = Vec::new();
    let names: BTreeMap<_, Vec<&String>> =
        d.output.names.iter().fold(BTreeMap::new(), |mut m, (k, n)| {
            m.entry(*k).or_insert_with(Vec::new).push(n);
            m
        });
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
            .map(|(k, _)| names.get(k))
            .collect();
        problems.push(format!("digest.collisions = {counted}; marked: {marked:?}"));
    }
    for (k, units) in &d.units {
        let digests: BTreeSet<_> = units.iter().map(|u| u.digest).collect();
        let marked = d.output.causes.iter().any(|(x, c)| x == k && *c == Cause::Collision);
        if digests.len() > 1 && !marked {
            problems.push(format!(
                "one key, {} digests, not marked a collision: {:?}",
                digests.len(),
                names.get(k)
            ));
        }
    }
    let mut out = Vec::new();
    if !problems.is_empty() {
        match SAME_KEY.iter().position(|(r, _)| *r == rel) {
            Some(i) => out.push(Finding::KnownKey(i)),
            None => out.extend(
                problems.into_iter().map(|p| Finding::Problem(format!("{rel} side {side:?}: {p}"))),
            ),
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
        match SAME_NAME.iter().position(|(r, s, _)| *r == rel && *s == n) {
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
    let (mut keys, mut names) = (BTreeSet::new(), BTreeSet::new());
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
        }
    }
    for (i, (r, _)) in SAME_KEY.iter().enumerate() {
        if !keys.contains(&i) {
            out.push(format!("SAME_KEY lists {r}, whose outputs no longer hold a collision"));
        }
    }
    for (i, (r, n, _)) in SAME_NAME.iter().enumerate() {
        if !names.contains(&i) {
            out.push(format!("SAME_NAME lists `{n}` in {r}, which no longer has it twice"));
        }
    }
    out
}
