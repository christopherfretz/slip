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
//!
//! M2 (WP-070): [`run`] also checks agreement between observers (§9.2 to
//! §9.5) and folds the spelled entities over the run ([`spelling`], §10.3,
//! §10.6); [`guards`] is the overlay's guards O1 and O2 (§5.3); and
//! [`conclude`] is the state of a transform's result and what each state
//! permits (`spec/edits.md` §8.5). Every function is pure over model types
//! and fails closed: what it cannot read with certainty permits no write.

#![forbid(unsafe_code)]

pub mod compare;
pub mod conclude;
pub mod guards;
pub mod run;
pub mod spelling;

pub use compare::{
    AtSums, CompareError, Comparison, Exemptions, at_sums, comparable, compare_groups,
    unverifiable, verdict, verified,
};
