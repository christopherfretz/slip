//! The run record: what a parallel phase did and what it cost
//! (`spec/model.md` §9). Not deterministic.
//!
//! Plain data. The binary and JSON forms are implemented in `formats`
//! (work package WP-006).

use crate::{Digest, Phase};

/// Facts about one invocation of a parallel phase.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    pub tool_slip: String,
    pub tool_kiln: String,
    /// Content id of the plan.
    pub plan: Digest,
    pub phase: Phase,
    /// The round of the phase, 0 for the first (`spec/exec.md` §2).
    pub round: u64,
    pub started_unix_ns: u64,
    /// Wall time of the whole phase.
    pub wall_ns: u64,
    /// Cores available to the process.
    pub cores: u64,
    /// Most workers running at once.
    pub workers_max: u64,
    /// Bytes; 0 if unlimited.
    pub memory_budget: u64,
    /// Largest observed sum of the running workers' resident sizes, bytes; 0 if unknown.
    pub peak_rss_sum: u64,
}

/// How one attempt at an observer ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum End {
    Completed = 0,
    Crashed = 1,
    KilledMemory = 2,
    KilledTime = 3,
    Cancelled = 4,
}

/// One attempt at one observer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Job {
    pub observer: Digest,
    /// 1 for the first try.
    pub attempt: u64,
    pub end: End,
    /// Content id of the shard written, if any.
    pub shard: Option<Digest>,
    /// From phase start to the job being started.
    pub queued_ns: u64,
    pub wall_ns: u64,
    /// User plus system time of the worker for this job; 0 if unknown.
    pub cpu_ns: u64,
    /// Bytes; 0 if unknown.
    pub peak_rss: u64,
    /// Bytes the admission controller reserved for it.
    pub estimate: u64,
}

/// A run record (`spec/model.md` §9).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunRecord {
    pub run: Run,
    /// In completion order.
    pub jobs: Vec<Job>,
}
