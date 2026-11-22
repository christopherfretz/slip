//! Slip's data model and on-disk formats (`spec/model.md`).
//!
//! Everything the phases exchange is defined here: file and observer
//! identity, entity keys, ranges, edits, facts, shards, plans and reports,
//! with their versioned binary encoding and JSON rendering. This crate
//! depends on nothing (not on Kiln, not on I/O policy), so that reduce,
//! apply and verify can be tested with hand-written inputs and
//! re-implemented from the specification.

#![forbid(unsafe_code)]

pub mod container;
mod digest;
pub mod digests;
pub mod formats;
pub mod hash;
pub mod json;
mod plan;
mod run;
mod shard;
pub mod verdict;
pub mod wire;

pub use digest::Digest;
pub use plan::{
    Command, Configuration, EnvVar, Excluded, FileRef, Language, Observer, Origin, Plan, Reason,
};
pub use run::{End, Job, Run, RunRecord};
pub use shard::{Counter, Outcome, Phase, Shard, Status};
