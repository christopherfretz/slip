//! Memory admission (`spec/exec.md` §6). Work package WP-009.
//!
//! [`Admission`] is a pure state machine: no clock, no processes, no I/O.
//! It owns the queue of pending jobs (in plan order, with requeued jobs put
//! back at its head), the reservations of the running ones, the skip counts
//! of the 64-skip guard, the pressure requeues and the "alone" modes. The
//! coordinator ([`crate::pool`]) asks it which job to start
//! ([`Admission::pick`]), tells it when a job ends ([`Admission::finished`])
//! and feeds it the sampled resident sizes ([`Admission::on_samples`]),
//! acting on the [`Decision`]s it returns. Everything is decided on sampled
//! values only, so the tests drive it with numbers. The rules the spec
//! leaves open are in `spec/landed/exec/WP-009.md`.

use std::collections::{HashMap, VecDeque};
use std::hash::BuildHasher;

use slip_model::{Digest, Observer};

/// One GiB.
pub const GIB: u64 = 1 << 30;

/// `--estimate-base` by default (§6 "Estimate").
pub const DEFAULT_ESTIMATE_BASE: u64 = 512 << 20;

/// `--estimate-per-byte` by default.
pub const DEFAULT_ESTIMATE_PER_BYTE: u64 = 64;

/// `--worker-stack` by default (§5.2).
pub const DEFAULT_WORKER_STACK: u64 = GIB;

/// A pending job passed over this many times blocks all later jobs.
pub const SKIP_LIMIT: u32 = 64;

/// A job requeued under pressure this many times runs alone from then on.
pub const ALONE_AFTER_REQUEUES: u32 = 3;

/// The admission parameters (§6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    /// `B`, bytes. 0 disables admission and caps.
    pub budget: u64,
    /// `--estimate-base`, bytes.
    pub estimate_base: u64,
    /// `--estimate-per-byte`.
    pub estimate_per_byte: u64,
    /// `--observer-memory`, bytes: the resident-size cap (default `B`).
    pub observer_memory: Option<u64>,
    /// `--worker-stack`, bytes, for the address-space cap.
    pub worker_stack: u64,
}

impl Config {
    /// `budget` with every other parameter at its default.
    pub fn with_budget(budget: u64) -> Self {
        Self {
            budget,
            estimate_base: DEFAULT_ESTIMATE_BASE,
            estimate_per_byte: DEFAULT_ESTIMATE_PER_BYTE,
            observer_memory: None,
            worker_stack: DEFAULT_WORKER_STACK,
        }
    }
}

/// §6 "Budget", second sentence: the smaller of the cgroup limit (when it
/// is a number) and the machine's total memory, minus a reserve of the
/// larger of 2 GiB and one tenth of that amount. Where the reserve would
/// leave nothing, only the one-tenth reserve is taken. 0 (no admission) if
/// neither amount is known.
pub fn budget_from_system(cgroup_max: Option<u64>, total: u64) -> u64 {
    let memory = match (cgroup_max, total) {
        (Some(c), 0) => c,
        (Some(c), t) => c.min(t),
        (None, t) => t,
    };
    let tenth = memory / 10;
    let reserve = tenth.max(2 * GIB);
    if memory > reserve { memory - reserve } else { memory - tenth }
}

/// The amounts [`budget_from_system`] takes: the cgroup v2 `memory.max` of
/// this process (the smallest number along its cgroup and the cgroup's
/// ancestors; `None` if every one is `max` or nothing can be read), and
/// `MemTotal` of `/proc/meminfo` in bytes (0 if unknown).
pub fn read_system() -> (Option<u64>, u64) {
    let total =
        std::fs::read_to_string("/proc/meminfo").ok().and_then(|t| meminfo_total(&t)).unwrap_or(0);
    let cgroup = std::fs::read_to_string("/proc/self/cgroup").ok().and_then(|t| {
        let rel = cgroup_v2_path(&t)?;
        let root = std::path::Path::new("/sys/fs/cgroup");
        let mut dir = root.join(rel.trim_start_matches('/'));
        let mut best: Option<u64> = None;
        loop {
            if let Some(v) = std::fs::read_to_string(dir.join("memory.max"))
                .ok()
                .and_then(|t| cgroup_memory_max(&t))
            {
                best = Some(best.map_or(v, |b| b.min(v)));
            }
            if dir == root || !dir.pop() || !dir.starts_with(root) {
                break;
            }
        }
        best
    });
    (cgroup, total)
}

/// `MemTotal:` of `/proc/meminfo`, in bytes.
pub fn meminfo_total(text: &str) -> Option<u64> {
    let v = text.lines().find_map(|l| l.strip_prefix("MemTotal:"))?;
    let kb: u64 = v.trim().strip_suffix("kB")?.trim().parse().ok()?;
    kb.checked_mul(1024)
}

/// The content of a `memory.max` file: a number of bytes, or `None` for
/// `max` (no limit) or anything else.
pub fn cgroup_memory_max(text: &str) -> Option<u64> {
    text.trim().parse().ok()
}

/// The cgroup v2 path of `/proc/self/cgroup` (the `0::<path>` line).
pub fn cgroup_v2_path(text: &str) -> Option<&str> {
    text.lines().find_map(|l| l.strip_prefix("0::"))
}

/// A job: its index in the observers given to [`Admission::new`].
pub type JobKey = usize;

/// What [`Admission::on_samples`] asks the coordinator to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Kill the job: its sampled resident size exceeds the cap (§6
    /// "Caps"). `retry`: other jobs were running and it had not been
    /// retried for this yet, so it is retried once, alone (§6 "Retry");
    /// otherwise it fails.
    KillAtCap { job: JobKey, retry: bool },
    /// Kill the job: it started most recently and the running jobs'
    /// samples sum to more than `B` (§6 "Pressure"). It goes back to the
    /// head of the queue with `estimate`; not an attempt, not a failure.
    KillForPressure { job: JobKey, estimate: u64 },
}

/// A job [`Admission::pick`] started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pick {
    pub job: JobKey,
    /// Its estimate `e`, the reservation it was admitted with.
    pub estimate: u64,
    /// Nothing else is admitted while it runs.
    pub alone: bool,
}

#[derive(Debug, Clone, Default)]
struct JobState {
    estimate: u64,
    skips: u32,
    requeues: u32,
    /// Requeued under pressure [`ALONE_AFTER_REQUEUES`] times.
    alone_always: bool,
    /// The next run is the cap retry (alone).
    alone_next: bool,
    /// Already retried after a cap kill.
    cap_retried: bool,
}

#[derive(Debug, Clone)]
struct Running {
    job: JobKey,
    /// Start order (larger: started more recently).
    seq: u64,
    estimate: u64,
    sample: u64,
    alone: bool,
    /// A decision to kill it was returned; it still reserves memory until
    /// [`Admission::finished`].
    killing: bool,
    /// What happens to it when it finishes, if a decision was taken.
    requeue: Option<Requeue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Requeue {
    Pressure,
    CapRetry,
    Fail,
}

/// The admission controller (§6). See the module documentation.
#[derive(Debug, Clone)]
pub struct Admission {
    config: Config,
    jobs: Vec<JobState>,
    pending: VecDeque<JobKey>,
    running: Vec<Running>,
    seq: u64,
}

impl Admission {
    /// A controller for `observers` (in plan order, all pending).
    /// `previous` is the peak resident size of each observer in the most
    /// recent run record of the phase (§6 "Estimate", rule 1).
    pub fn new<S: BuildHasher>(
        config: Config,
        previous: &HashMap<Digest, u64, S>,
        observers: &[Observer],
    ) -> Self {
        let jobs = observers
            .iter()
            .map(|o| JobState { estimate: estimate(&config, previous, o), ..JobState::default() })
            .collect();
        Self { config, jobs, pending: (0..observers.len()).collect(), running: Vec::new(), seq: 0 }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    /// The current estimate `e` of `job` (§6 "Estimate", or `2 ×` its last
    /// sample after a pressure kill).
    pub fn estimate(&self, job: JobKey) -> u64 {
        self.jobs.get(job).map_or(0, |j| j.estimate)
    }

    /// The resident-size cap: `--observer-memory`, default `B`; none when
    /// `B` is 0.
    pub fn rss_cap(&self) -> Option<u64> {
        rss_cap(&self.config)
    }

    /// The address-space cap: twice the resident-size cap plus the worker
    /// stack plus 1 GiB; none when `B` is 0.
    pub fn address_space_cap(&self) -> Option<u64> {
        self.rss_cap().map(|c| {
            c.saturating_mul(2).saturating_add(self.config.worker_stack).saturating_add(GIB)
        })
    }

    /// `R`: the sum of the running jobs' reservations `max(e, sample)`.
    pub fn reserved(&self) -> u64 {
        self.running.iter().fold(0u64, |r, j| r.saturating_add(j.estimate.max(j.sample)))
    }

    /// The pending jobs, in the order they would be considered.
    pub fn pending(&self) -> impl Iterator<Item = JobKey> + '_ {
        self.pending.iter().copied()
    }

    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// The running jobs, in start order.
    pub fn running(&self) -> impl Iterator<Item = JobKey> + '_ {
        self.running.iter().map(|r| r.job)
    }

    pub fn running_len(&self) -> usize {
        self.running.len()
    }

    /// How often `job` has been passed over while pending.
    pub fn skips(&self, job: JobKey) -> u32 {
        self.jobs.get(job).map_or(0, |j| j.skips)
    }

    /// How often `job` was requeued under pressure.
    pub fn requeues(&self, job: JobKey) -> u32 {
        self.jobs.get(job).map_or(0, |j| j.requeues)
    }

    /// Whether `job`'s next run is alone.
    pub fn runs_alone(&self, job: JobKey) -> bool {
        self.jobs.get(job).is_some_and(|j| j.alone_always || j.alone_next)
    }

    /// The job [`Admission::pick`] would start now, without changing
    /// anything.
    pub fn peek(&self) -> Option<JobKey> {
        self.select().map(|(i, _)| self.pending[i])
    }

    /// The pending job to start now (§6 "Admission"), marked running, or
    /// `None`. With `B = 0`, the head of the queue. Otherwise: nothing while
    /// a job runs alone; then the first job in queue order that is
    /// admitted (`R + e ≤ B`, or nothing running), where a job that must
    /// run alone is admitted only when nothing runs and blocks the jobs
    /// after it until then, and a job passed over [`SKIP_LIMIT`] times
    /// blocks the jobs after it. Each job before the one started is passed
    /// over once more.
    pub fn pick(&mut self) -> Option<Pick> {
        let (i, alone) = self.select()?;
        for &k in self.pending.iter().take(i) {
            self.jobs[k].skips = self.jobs[k].skips.saturating_add(1);
        }
        let job = self.pending.remove(i)?;
        let state = &mut self.jobs[job];
        state.alone_next = false;
        let estimate = state.estimate;
        self.seq += 1;
        self.running.push(Running {
            job,
            seq: self.seq,
            estimate,
            sample: 0,
            alone,
            killing: false,
            requeue: None,
        });
        Some(Pick { job, estimate, alone })
    }

    /// (index into `pending`, alone) of the job to start.
    fn select(&self) -> Option<(usize, bool)> {
        if self.pending.is_empty() {
            return None;
        }
        if self.config.budget == 0 {
            return Some((0, false));
        }
        if self.running.iter().any(|r| r.alone) {
            return None;
        }
        let reserved = self.reserved();
        let idle = self.running.is_empty();
        for (i, &k) in self.pending.iter().enumerate() {
            let j = &self.jobs[k];
            if j.alone_always || j.alone_next {
                return idle.then_some((i, true));
            }
            if idle || reserved.saturating_add(j.estimate) <= self.config.budget {
                return Some((i, false));
            }
            if j.skips >= SKIP_LIMIT {
                return None;
            }
        }
        None
    }

    /// One round of samples `(job, resident size)` of running jobs (§6
    /// "Caps", "Pressure"). Records each sample (the reservation becomes
    /// `max(e, sample)`), then: every job over the cap is to be killed
    /// ([`Decision::KillAtCap`]); then, if the samples of the running jobs
    /// not being killed sum to more than `B` while more than one of them
    /// runs, the one started most recently is to be killed
    /// ([`Decision::KillForPressure`]). Jobs already being killed are not
    /// decided on again. Nothing happens with `B = 0`, and a sample of 0
    /// ("unknown") never exceeds a cap. Samples of jobs that are not running
    /// are ignored.
    pub fn on_samples(&mut self, samples: &[(JobKey, u64)]) -> Vec<Decision> {
        for &(job, rss) in samples {
            if let Some(r) = self.running.iter_mut().find(|r| r.job == job) {
                r.sample = rss;
            }
        }
        let mut out = Vec::new();
        if self.config.budget == 0 {
            return out;
        }
        let cap = self.rss_cap().unwrap_or(u64::MAX);
        let others_running = self.running.len() > 1;
        for r in &mut self.running {
            if r.killing || r.sample <= cap {
                continue;
            }
            let state = &mut self.jobs[r.job];
            let retry = others_running && !state.cap_retried;
            r.killing = true;
            r.requeue = Some(if retry { Requeue::CapRetry } else { Requeue::Fail });
            out.push(Decision::KillAtCap { job: r.job, retry });
        }
        let live = || self.running.iter().filter(|r| !r.killing);
        let sum = live().fold(0u64, |s, r| s.saturating_add(r.sample));
        if sum > self.config.budget && live().count() > 1 {
            let newest = live().max_by_key(|r| r.seq).map(|r| r.job);
            if let Some(job) = newest
                && let Some(r) = self.running.iter_mut().find(|r| r.job == job)
            {
                let estimate = if r.sample == 0 {
                    self.jobs[job].estimate
                } else {
                    r.sample.saturating_mul(2)
                };
                r.killing = true;
                r.requeue = Some(Requeue::Pressure);
                out.push(Decision::KillForPressure { job, estimate });
            }
        }
        out
    }

    /// `job` stopped running (its process is gone). A job a decision of
    /// [`Admission::on_samples`] killed is requeued as that decision said:
    /// at the head of the queue, with `2 ×` its last sample after pressure
    /// (and alone from its [`ALONE_AFTER_REQUEUES`]th requeue on), alone
    /// for its cap retry, or not at all when it failed at its cap.
    /// Otherwise `retry` puts it back at the head of the queue (the
    /// coordinator's retry of a crash, §5.4). Unknown or not running: no
    /// effect.
    pub fn finished(&mut self, job: JobKey, retry: bool) {
        let Some(i) = self.running.iter().position(|r| r.job == job) else { return };
        let r = self.running.remove(i);
        let state = &mut self.jobs[job];
        let back = match r.requeue {
            Some(Requeue::Pressure) => {
                if r.sample != 0 {
                    state.estimate = r.sample.saturating_mul(2);
                }
                state.requeues = state.requeues.saturating_add(1);
                if state.requeues >= ALONE_AFTER_REQUEUES {
                    state.alone_always = true;
                }
                true
            }
            Some(Requeue::CapRetry) => {
                state.cap_retried = true;
                state.alone_next = true;
                true
            }
            Some(Requeue::Fail) => false,
            None => retry,
        };
        if back {
            self.pending.push_front(job);
        }
    }

    /// The coordinator is ending `job` for a reason of its own (a time
    /// limit, a protocol violation): it keeps its reservation until
    /// [`Admission::finished`] but is no longer decided on, and its sample
    /// no longer counts towards pressure.
    pub fn stopping(&mut self, job: JobKey) {
        if let Some(r) = self.running.iter_mut().find(|r| r.job == job) {
            r.killing = true;
        }
    }

    /// Whether `job` is being killed (a decision was returned, or
    /// [`Admission::stopping`]) and has not finished yet.
    pub fn is_killing(&self, job: JobKey) -> bool {
        self.running.iter().any(|r| r.job == job && r.killing)
    }
}

fn rss_cap(config: &Config) -> Option<u64> {
    (config.budget != 0).then(|| config.observer_memory.unwrap_or(config.budget))
}

/// §6 "Estimate": the previous peak of the observer times 1.25 (rounded
/// down) when it is known and not 0, else `--estimate-base` plus
/// `--estimate-per-byte` times the main file's size. Saturating.
pub fn estimate<S: BuildHasher>(
    config: &Config,
    previous: &HashMap<Digest, u64, S>,
    observer: &Observer,
) -> u64 {
    match previous.get(&observer.id) {
        Some(&peak) if peak != 0 => peak.saturating_add(peak / 4),
        _ => config
            .estimate_base
            .saturating_add(config.estimate_per_byte.saturating_mul(observer.main.size)),
    }
}
