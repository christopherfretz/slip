//! Digest comparison under declared changes, and localisation of
//! mismatches (`spec/verify.md`). Works on model types only; no Kiln.
//!
//! [`compare`] is §3, §4 and §4.1: given the `digests` sections of the pre
//! and post shard, [`comparable`] checks they can be compared, [`at_sums`]
//! compares group sums and gives the groups `G` that differ or hold an
//! indeterminate unit, and [`compare_groups`] applies §4 to the unit lists
//! of those groups (or of all groups: the result is the same). The functions [`verdict`],
//! [`verified`] and [`unverifiable`] make the `Verdict` of §6. Behaviour
//! the spec leaves open is in `spec/landed/verify/WP-016.md`.

#![forbid(unsafe_code)]

pub mod compare;

pub use compare::{
    AtSums, CompareError, Comparison, Exemptions, at_sums, comparable, compare_groups,
    unverifiable, verdict, verified,
};
