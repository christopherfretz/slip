//! Digest comparison under declared changes, and localisation of
//! mismatches (`spec/verify.md`). Works on model types only; no Kiln.
//!
//! [`compare`] is §3, §4 and §4.1: given the `digests` sections of the pre
//! and post shard, [`comparable`] checks they can be compared, [`at_sums`]
//! compares group sums and gives the groups `G` that differ or hold an
//! indeterminate unit, and [`compare_groups`] applies §4 to the unit lists
//! of those groups (or of all groups: the result is the same). The functions [`verdict`],
//! [`verified`] and [`unverifiable`] make the `Verdict` of §6. The
//! preconditions are §4.2 and the interface §4.3.
//!
//! [`run`] is §7: the [`run::State`] of each selected observer from its
//! verify shard and the status of its map shard, the run's verdict, and
//! the report (§7.1) as JSON and text; the interface is §7.3. Nothing it
//! cannot read with certainty (a shard that breaks its format, a verdict
//! whose counters are missing or disagree with it, a shard of another
//! observer or round) is `verified`.

#![forbid(unsafe_code)]

pub mod compare;
pub mod run;

pub use compare::{
    AtSums, CompareError, Comparison, Exemptions, at_sums, comparable, compare_groups,
    unverifiable, verdict, verified,
};
