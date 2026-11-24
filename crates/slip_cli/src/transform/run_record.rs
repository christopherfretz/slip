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

/// What the signal path waits on: the command has published what an
/// interruption must leave behind (the result, the record), and whether a
/// write of the tree has begun.
#[derive(Debug, Default)]
struct Progress {
    published: bool,
    writing: bool,
}

static PROGRESS: std::sync::Mutex<Progress> =
    std::sync::Mutex::new(Progress { published: false, writing: false });
static PROGRESS_CHANGED: std::sync::Condvar = std::sync::Condvar::new();

fn progress() -> std::sync::MutexGuard<'static, Progress> {
    PROGRESS.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// How long the signal path waits for [`Published`] before it cleans the
/// store anyway, unless a write has begun ([`begin_write`]). The result
/// and the record are published within milliseconds of the phase's end;
/// this bounds a coordinator that is slow to get there (a report written
/// to a slow file) and keeps the rest of `spec/exec.md` §7's 15 s for
/// the cleanup. At 0 an interrupted run publishes nothing: the test
/// `s10_6_rename_interrupted_after_its_verify_phase` fails.
const PUBLISH_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

/// Dropped when the command has published its result and its record, or
/// given up: from then the signal path may clean the store
/// ([`install_signals`]).
#[derive(Debug)]
pub struct Published;

impl Drop for Published {
    fn drop(&mut self) {
        progress().published = true;
        PROGRESS_CHANGED.notify_all();
    }
}

/// Read immediately before a write of the tree begins (WP-076 fix round):
/// `false` if the command was interrupted, and then nothing may be
/// written. `true` marks the write begun: from then the signal path waits
/// for [`Published`] with no limit of its own, so the write is finished
/// and recorded before the process ends by the signal (only `spec/exec.md`
/// §7's 15 s bound of the whole cleanup, or a `SIGKILL`, cuts it; the
/// journal is for those). The check and the mark are one step under the
/// lock the signal path takes after it records the signal: a signal is
/// either seen here or sees the mark.
pub fn begin_write() -> bool {
    let mut p = progress();
    gate(&mut p, interrupted())
}

/// [`begin_write`]'s decision, under the lock: no write once interrupted;
/// otherwise the write is marked begun.
fn gate(p: &mut Progress, interrupted: bool) -> bool {
    if interrupted {
        return false;
    }
    p.writing = true;
    true
}

/// The signal path's wait: until `published`, at most `wait` unless a
/// write has begun, and then for as long as it takes.
fn wait_published(
    progress: &std::sync::Mutex<Progress>,
    changed: &std::sync::Condvar,
    wait: std::time::Duration,
) {
    let deadline = Instant::now() + wait;
    let mut p = progress.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    while !p.published {
        if p.writing {
            p = changed.wait(p).unwrap_or_else(std::sync::PoisonError::into_inner);
            continue;
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        p = changed.wait_timeout(p, left).unwrap_or_else(std::sync::PoisonError::into_inner).0;
    }
}

/// Installs the signal path of a command that runs phases on `store`
/// (`spec/exec.md` §7) so that an interrupted run still publishes its
/// result and record: on `SIGINT` or `SIGTERM` the path waits, at most
/// [`PUBLISH_WAIT`] (with no limit once a write has begun), for the
/// [`Published`] returned here to be dropped, then runs `before` and the
/// store's cleanup (`slip_exec::pool::cleanup_on_signal`).
pub fn install_signals(store: &Store, before: fn()) -> Published {
    let clean = slip_exec::pool::cleanup_on_signal(store);
    slip_exec::signals::install(Box::new(move || {
        wait_published(&PROGRESS, &PROGRESS_CHANGED, PUBLISH_WAIT);
        before();
        clean();
    }));
    Published
}

/// Whether a signal has interrupted the command.
pub fn interrupted() -> bool {
    slip_exec::signals::interrupted().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    // WP-076 decision 6: nothing is written once interrupted; a write that
    // begins is marked, so the signal path waits for it to be recorded.
    #[test]
    fn a_write_begins_only_before_an_interruption() {
        let mut p = Progress::default();
        assert!(!gate(&mut p, true));
        assert!(!p.writing);
        assert!(gate(&mut p, false));
        assert!(p.writing);
    }

    /// `wait_published` over a progress of `writing`, with a wait of 50
    /// ms and publication 400 ms in: how long it waited.
    fn waited(writing: bool) -> std::time::Duration {
        use std::sync::{Arc, Condvar, Mutex};
        let shared = Arc::new((Mutex::new(Progress { published: false, writing }), Condvar::new()));
        let other = Arc::clone(&shared);
        let publisher = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(400));
            other.0.lock().unwrap().published = true;
            other.1.notify_all();
        });
        let start = Instant::now();
        wait_published(&shared.0, &shared.1, std::time::Duration::from_millis(50));
        let took = start.elapsed();
        publisher.join().unwrap();
        took
    }

    // WP-076 decision 6: before a write the signal path waits at most its
    // limit; once a write has begun it waits until the run is published.
    #[test]
    fn the_signal_path_waits_out_a_write_in_progress() {
        assert!(waited(false) < std::time::Duration::from_millis(400));
        assert!(waited(true) >= std::time::Duration::from_millis(400));
    }
}
