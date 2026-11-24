//! The transform run record (`RUNT`, `spec/edits.md` §10.6) of one `map`,
//! `rename` or `check` invocation: a clock per step and the record,
//! published when the command ends, also when it is interrupted.
//!
//! The coordinator's CPU time is `/proc/self/stat`'s, its peak resident
//! size `/proc/self/status`'s `VmHWM` at the end of the step (0 where
//! there is none), as the phases' own records count theirs.

use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use slip_exec::store::Store;
use slip_model::Digest;
use slip_model::formats::content_id;
use slip_model::transform_run::{Step, StepName, TransformRun};

use super::{CommandError, store_error};
use crate::commands::tool;

/// This process's CPU time so far, in nanoseconds.
fn cpu_now() -> Option<u64> {
    let text = std::fs::read_to_string("/proc/self/stat").ok()?;
    slip_exec::worker::cpu_ns_from_stat(&text)
}

/// This process's peak resident size so far, in bytes; 0 if unknown.
fn peak_now() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .map_or(0, |t| slip_exec::process::Sample::from_proc_status(&t).hwm)
}

fn ns(d: std::time::Duration) -> u64 {
    u64::try_from(d.as_nanos()).unwrap_or(u64::MAX)
}

/// One step being timed.
#[derive(Debug)]
pub struct Clock {
    name: StepName,
    start: Instant,
    cpu: Option<u64>,
}

impl Clock {
    pub fn start(name: StepName) -> Clock {
        Clock { name, start: Instant::now(), cpu: cpu_now() }
    }

    /// The step, ended now; `run` is the content id of its phase's run
    /// record (map, verify).
    pub fn end(self, run: Option<Digest>) -> Step {
        let cpu = match (self.cpu, cpu_now()) {
            (Some(a), Some(b)) => b.saturating_sub(a),
            _ => 0,
        };
        Step {
            name: self.name,
            wall_ns: ns(self.start.elapsed()),
            cpu_ns: cpu,
            peak_rss: peak_now(),
            run,
        }
    }
}

/// The content id of the phase run record at `path` (`None` if it cannot
/// be read: the step then names no record).
pub fn run_id(path: &Path) -> Option<Digest> {
    content_id(&std::fs::read(path).ok()?).ok()
}

/// What the command knows of its run so far; [`Record::publish`] makes the
/// record of it.
#[derive(Debug)]
pub struct Record {
    pub started_unix_ns: u64,
    start: Instant,
    pub steps: Vec<Step>,
    pub edits: Option<Digest>,
    pub declared: Option<Digest>,
    pub overlay: Option<Digest>,
    pub result: Option<Digest>,
}

impl Record {
    /// The command starts now.
    pub fn new() -> Record {
        let started_unix_ns = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, ns);
        Record {
            started_unix_ns,
            start: Instant::now(),
            steps: Vec::new(),
            edits: None,
            declared: None,
            overlay: None,
            result: None,
        }
    }

    /// Publishes the record of round `round` of plan `plan` and request
    /// `transform`; its path.
    pub fn publish(
        &self,
        store: &Store,
        plan: Digest,
        round: u64,
        transform: Digest,
    ) -> Result<PathBuf, CommandError> {
        let t = tool();
        let run = TransformRun {
            tool_slip: t.slip,
            tool_kiln: t.kiln,
            plan,
            round,
            transform,
            edits: self.edits,
            declared: self.declared,
            overlay: self.overlay,
            result: self.result,
            started_unix_ns: self.started_unix_ns,
            wall_ns: ns(self.start.elapsed()),
            steps: self.steps.clone(),
        };
        store
            .publish_transform_run(&plan, round, self.started_unix_ns, &run.to_bytes())
            .map_err(|e| store_error(store, &e))
    }
}

impl Default for Record {
    fn default() -> Self {
        Record::new()
    }
}

/// Whether the command has published what an interruption must leave
/// behind (the result, the record), for the signal path to wait on.
static PUBLISHED: std::sync::Mutex<bool> = std::sync::Mutex::new(false);
static PUBLISHED_CHANGED: std::sync::Condvar = std::sync::Condvar::new();

/// How long the signal path waits for [`Published`] before it cleans the
/// store anyway (`spec/exec.md` §7 bounds the whole cleanup at 15 s).
const PUBLISH_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

/// Dropped when the command has published its result and its record, or
/// given up: from then the signal path may clean the store
/// ([`install_signals`]).
#[derive(Debug)]
pub struct Published;

impl Drop for Published {
    fn drop(&mut self) {
        let mut done = PUBLISHED.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        *done = true;
        PUBLISHED_CHANGED.notify_all();
    }
}

/// Installs the signal path of a command that runs phases on `store`
/// (`spec/exec.md` §7) so that an interrupted run still publishes its
/// result and its record (§10.6): the cleanup waits, at most
/// [`PUBLISH_WAIT`], for the [`Published`] returned here to be dropped,
/// then runs `before` and `pool::cleanup_on_signal`, after which the store
/// creates no file.
pub fn install_signals(store: &Store, before: fn()) -> Published {
    let clean = slip_exec::pool::cleanup_on_signal(store);
    slip_exec::signals::install(Box::new(move || {
        let deadline = Instant::now() + PUBLISH_WAIT;
        let mut done = PUBLISHED.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        while !*done {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            done = PUBLISHED_CHANGED
                .wait_timeout(done, left)
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .0;
        }
        drop(done);
        before();
        clean();
    }));
    Published
}

/// Whether a signal has interrupted the command.
pub fn interrupted() -> bool {
    slip_exec::signals::interrupted().is_some()
}
