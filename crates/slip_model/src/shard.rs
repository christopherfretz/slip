//! The shard: one observer's output for one phase (`spec/model.md` §8).
//!
//! Plain data. The binary and JSON forms are implemented in `formats`
//! (work package WP-006).

use crate::{Digest, FileRef};

/// A parallel phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Phase {
    Map = 1,
    Verify = 2,
}

/// How an observer's work ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Status {
    /// The front end finished with no error.
    Ok = 0,
    /// The front end reported errors.
    Diagnosed = 1,
    /// The front end refused the command line.
    Rejected = 2,
    /// The worker could not produce a result; written by the coordinator.
    Failed = 3,
}

/// Status and diagnostic totals.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub status: Status,
    /// Diagnostics of severity error or fatal.
    pub errors: u64,
    pub warnings: u64,
    /// Empty for `Ok`; otherwise the first error, at most 4096 bytes.
    pub message: String,
}

/// A deterministic quantity, named `word.word` in lower-case ASCII.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Counter {
    pub name: String,
    pub value: u64,
}

/// A shard (`spec/model.md` §8). Deterministic content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shard {
    pub observer: Digest,
    pub phase: Phase,
    pub round: u64,
    pub outcome: Outcome,
    /// Every file the front end read, sorted by path; absent when unknown or failed.
    pub inputs: Option<Vec<FileRef>>,
    /// Sorted by name.
    pub counters: Vec<Counter>,
}
