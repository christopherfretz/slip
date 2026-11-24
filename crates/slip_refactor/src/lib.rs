//! The transforms Slip implements in Rust: Level A of
//! `docs/design/01-transform-api.md` §6. The first is the native rename of
//! functions and variables (`spec/edits.md` §9; work package WP-071,
//! `spec/landed/edits/WP-071.md`).
//!
//! The rename reads the subject through four interfaces and nothing else
//! of the front end's tables (§9.7): `slip_digest::query::Keys` (key ids,
//! keys, rules), `slip_kiln::written` (the names of declarations and the
//! written references), `slip_kiln::ranges` (through `sites`) and
//! `slip_kiln::sites` (where a name token is written and why it cannot be
//! edited). The AST itself is read only where `docs/wp/WP-071.md` says:
//! the enclosing scopes of a candidate (§9.1), the declarations a scope
//! holds (`name_taken`), and what the conservative hooks look at
//! (`rename::families`, `followers`, `spec_members`), and, by
//! `docs/wp/WP-127.md`, a class's lookup table for what a
//! using-declaration names (`followers`) and the expression arena for
//! what an instantiation binds a dependent name to (`dependent`). Every
//! range in a
//! claim is an answer of `sites` for a span Kiln gave; nothing here
//! computes an offset, lexes, or compares identifiers by the bytes of a
//! file.

#![forbid(unsafe_code)]

mod rename;

use slip_kiln::Unit;
use slip_kiln::kiln::source::FileId;
use slip_model::edits::{Edits, Stance};
use slip_model::{Counter, FileRef, Transform};

/// `spec/edits.md` §9.7: the native rename over one parsed observer.
///
/// `files` gives the `FileRef` of a buffer that is one of the shard's
/// inputs (the worker passes `Sites::file_ref`'s answer). `keys` is a
/// `Keys` over the same unit with no rules (the pre-image's key space).
///
/// `Err` is a failure of Slip, never a fact about the subject: a request
/// that is not a rename, a request that does not satisfy `spec/edits.md`
/// §3, a unit parsed without `ParseOptions::written` or without
/// `ParseOptions::pp_record`, a claim in a buffer
/// `files` does not know, a unit without a main file. Everything about
/// the subject is in the `Edits`, which has passed `Edits::normalise`,
/// satisfies §4.7, and has `failure`, `groups`, `reports` and `renamed`
/// empty (§9.6).
///
/// `run` is a function of the unit: two parses of the same inputs give
/// equal `Edits`.
///
/// # Errors
///
/// As said.
pub fn run(
    transform: &Transform,
    unit: &Unit<'_>,
    keys: &mut slip_digest::query::Keys<'_>,
    files: &dyn Fn(FileId) -> Option<FileRef>,
) -> Result<Edits, String> {
    run_counted(transform, unit, keys, files).map(|(edits, _)| edits)
}

/// [`run`], with the counters a map shard of a transform carries
/// (`spec/edits.md` §6.3), sorted by name: `edits.claims` and one per
/// stance, `edits.subjects`, `edits.links`, `edits.groups`,
/// `edits.reports`, `edits.changed`, `edits.files`, and `edits.refs`, the
/// written references the pass looked at (`docs/wp/WP-071.md`, "Cost").
///
/// # Errors
///
/// As [`run`].
pub fn run_counted(
    transform: &Transform,
    unit: &Unit<'_>,
    keys: &mut slip_digest::query::Keys<'_>,
    files: &dyn Fn(FileId) -> Option<FileRef>,
) -> Result<(Edits, Vec<Counter>), String> {
    let (edits, refs) = rename::run(transform, unit, keys, files)?;
    let counters = counters(&edits, refs);
    Ok((edits, counters))
}

/// The `edits.*` counters of §6.3 for `edits`, with `edits.refs` given;
/// sorted by name.
fn counters(edits: &Edits, refs: u64) -> Vec<Counter> {
    let stance = |s: Stance| edits.claims.iter().filter(|c| c.stance == s).count() as u64;
    let mut out = vec![
        ("edits.changed", edits.changed.len() as u64),
        ("edits.claims", edits.claims.len() as u64),
        ("edits.claims.keep", stance(Stance::Keep)),
        ("edits.claims.refuse", stance(Stance::Refuse)),
        ("edits.claims.replace", stance(Stance::Replace)),
        ("edits.claims.unresolved", stance(Stance::Unresolved)),
        ("edits.files", edits.files.len() as u64),
        ("edits.groups", edits.groups.len() as u64),
        ("edits.links", edits.links.len() as u64),
        ("edits.refs", refs),
        ("edits.reports", edits.reports.len() as u64),
        ("edits.subjects", edits.subjects.len() as u64),
    ];
    out.sort_by(|a, b| a.0.cmp(b.0));
    out.into_iter().map(|(name, value)| Counter { name: name.to_owned(), value }).collect()
}
