//! The coordinator: scheduling and supervision (`spec/exec.md` §5–§7).
//! Work package WP-009.
//!
//! [`run`] runs one phase over a list of observers. It starts workers
//! (`WorkerCommand`), hands each a `Job` over the protocol of §5.2, checks
//! every answer, applies §5.4 (including the shard the coordinator writes
//! for a worker that could not), feeds the sampled resident sizes to
//! [`Admission`] and acts on its decisions, and publishes the run record.
//!
//! **How the loop waits.** Every source of progress is a message on one
//! channel that the loop alone receives from: each worker has a thread
//! reading its standard output (one message per frame, then end of
//! output), a thread copying its standard error to the attempt's log (then
//! end of output), and a thread writing to its standard input (so a worker
//! that does not read can never block the loop); a token is delivered into
//! the channel by the jobserver ([`jobserver::Pool::request`]); the signal
//! path's cleanup ([`cleanup_on_signal`]) sends a cancellation. The loop
//! blocks in `recv_timeout` until the next sampling instant
//! ([`Options::sample_interval`]), at which it samples the running workers,
//! checks the time limit and reaps exited children (there is no portable
//! safe way to be woken by a child's exit; its end of output usually
//! arrives first). While a worker is known to be ending (its output ended
//! but it is not reaped, or its input was closed) the wait is at most
//! 5 ms. Nothing else blocks: the loop never reads, writes or waits on a
//! child directly. The rules the spec leaves open are in
//! `spec/landed/exec/WP-009.md`.

use std::collections::HashMap;
use std::ffi::OsString;
use std::fmt;
use std::fs::File;
use std::io::{self, Read as _, Seek as _, SeekFrom, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use slip_model::formats::{MESSAGE_MAX, clip_message, content_id};
use slip_model::{
    Digest, End, Job as JobRecord, Observer, Outcome, Phase, Run, RunRecord, Shard, Status,
};

use crate::admission::{self, Admission, Decision, JobKey, Pick};
use crate::jobserver::{self, Token};
use crate::process::{self, Child, Io, Killed, Limits, Sampler};
use crate::signals;
use crate::store::{Store, StoreError};
use crate::worker::{FRAME_MAX, Job, PROTOCOL, ToCoordinator, ToWorker, read_frame, write_frame};

/// `--observer-timeout` by default (§5.4).
pub const DEFAULT_OBSERVER_TIMEOUT: Duration = Duration::from_hours(1);

/// How long after a worker is reaped the coordinator waits for its output
/// and error streams to end (a descendant may hold them) before it
/// finishes the attempt with what it has.
const PIPE_GRACE: Duration = Duration::from_secs(1);

/// How long a worker whose input was closed may take to exit before it is
/// killed.
const EXIT_GRACE: Duration = Duration::from_secs(5);

/// How long the cancellation waits for the workers to be reaped.
const CANCEL_REAP: Duration = Duration::from_secs(10);

/// How long [`cleanup_on_signal`] waits for the runs in progress to write
/// their run records.
const CANCEL_WAIT: Duration = Duration::from_secs(3);

/// The poll while a worker is ending.
const ENDING_POLL: Duration = Duration::from_millis(5);

/// How a worker is started (§5.1): `program args…` with the variables of
/// `env_remove` removed from the inherited environment. The arguments carry
/// the worker's own `--worker-jobs` and stack size; the protocol does not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerCommand {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    /// Variables the worker must not inherit.
    pub env_remove: Vec<OsString>,
}

/// The parameters of a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// `-j`: at most this many jobs run at once (at least 1; also bounded
    /// by the token pool's own `jobs()`).
    pub jobs: usize,
    pub admission: admission::Config,
    /// `--observer-timeout`; `None` disables it.
    pub observer_timeout: Option<Duration>,
    /// Jobs given to one worker process before it is replaced (at least 1).
    /// Must not exceed the worker's own limit carried in
    /// [`WorkerCommand::args`].
    pub worker_jobs: u64,
    /// §4 sampling interval (at least 1 ms).
    pub sample_interval: Duration,
    /// The run record's `tool_slip` and `tool_kiln`.
    pub tool_slip: String,
    pub tool_kiln: String,
}

impl Options {
    /// `jobs` and `admission` with every other parameter at its default.
    pub fn new(jobs: usize, admission: admission::Config) -> Self {
        Self {
            jobs,
            admission,
            observer_timeout: Some(DEFAULT_OBSERVER_TIMEOUT),
            worker_jobs: 1,
            sample_interval: process::SAMPLE_INTERVAL,
            tool_slip: String::new(),
            tool_kiln: String::new(),
        }
    }
}

/// What a finished run did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    /// The run record as published.
    pub run: RunRecord,
    pub run_path: PathBuf,
    /// Final shards by status (`Status as usize`).
    pub statuses: [u64; 4],
    /// Run-record jobs by end (`End as usize`).
    pub ends: [u64; 5],
}

/// Why [`run`] did not finish.
#[derive(Debug)]
pub enum PoolError {
    Store(StoreError),
    Io(io::Error),
    /// Interrupted by `SIGINT` or `SIGTERM` (§7). The run record with the
    /// remaining jobs `cancelled` was published (best effort).
    Cancelled,
}

impl fmt::Display for PoolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PoolError::Store(e) => write!(f, "{e}"),
            PoolError::Io(e) => write!(f, "{e}"),
            PoolError::Cancelled => write!(f, "interrupted"),
        }
    }
}

impl std::error::Error for PoolError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PoolError::Store(e) => Some(e),
            PoolError::Io(e) => Some(e),
            PoolError::Cancelled => None,
        }
    }
}

impl From<StoreError> for PoolError {
    fn from(e: StoreError) -> Self {
        PoolError::Store(e)
    }
}

impl From<io::Error> for PoolError {
    fn from(e: io::Error) -> Self {
        PoolError::Io(e)
    }
}

// ----- cancellation from the signal path -------------------------------------

/// A run in progress, as the signal path sees it.
#[derive(Debug)]
struct Active {
    wake: Mutex<Option<Sender<Event>>>,
    done: Mutex<bool>,
    finished: Condvar,
}

static ACTIVE: Mutex<Vec<Arc<Active>>> = Mutex::new(Vec::new());

/// Registered for the life of a [`run`]; marks it done on every path.
struct ActiveGuard(Arc<Active>);

impl ActiveGuard {
    fn register(wake: Sender<Event>) -> Self {
        let a = Arc::new(Active {
            wake: Mutex::new(Some(wake)),
            done: Mutex::new(false),
            finished: Condvar::new(),
        });
        ACTIVE.lock().unwrap_or_else(PoisonError::into_inner).push(Arc::clone(&a));
        Self(a)
    }
}

impl Drop for ActiveGuard {
    fn drop(&mut self) {
        self.0.wake.lock().unwrap_or_else(PoisonError::into_inner).take();
        *self.0.done.lock().unwrap_or_else(PoisonError::into_inner) = true;
        self.0.finished.notify_all();
        ACTIVE.lock().unwrap_or_else(PoisonError::into_inner).retain(|a| !Arc::ptr_eq(a, &self.0));
    }
}

/// The callback for [`signals::install`] of a process that runs phases on
/// `store` (§7): tells every [`run`] in progress that it is cancelled,
/// waits (at most 3 s) for each to write its run record with the remaining
/// jobs `cancelled`, then removes the store's `tmp/` files and lock
/// ([`crate::store::cleanup_on_signal`]). Never panics.
pub fn cleanup_on_signal(store: &Store) -> impl FnOnce() + Send + 'static {
    let store_cleanup = crate::store::cleanup_on_signal(store);
    move || {
        wait_for_cancelled_runs(CANCEL_WAIT);
        store_cleanup();
    }
}

fn wait_for_cancelled_runs(bound: Duration) {
    let active: Vec<Arc<Active>> = ACTIVE.lock().unwrap_or_else(PoisonError::into_inner).clone();
    for a in &active {
        if let Some(tx) = a.wake.lock().unwrap_or_else(PoisonError::into_inner).as_ref() {
            let _ = tx.send(Event::Cancel);
        }
    }
    let deadline = Instant::now() + bound;
    for a in &active {
        let mut done = a.done.lock().unwrap_or_else(PoisonError::into_inner);
        while !*done {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return;
            }
            done = a.finished.wait_timeout(done, left).unwrap_or_else(PoisonError::into_inner).0;
        }
    }
}

// ----- the loop ---------------------------------------------------------------

/// What the loop receives.
#[derive(Debug)]
enum Event {
    Token(Token),
    /// One frame of a worker's output, decoded, or why it could not be.
    Message {
        worker: u64,
        message: Result<ToCoordinator, String>,
    },
    /// The worker's output ended (or nothing more will be read from it).
    Stdout {
        worker: u64,
    },
    /// The worker's error stream ended.
    Stderr {
        worker: u64,
    },
    Cancel,
}

/// Where a worker's standard error goes: the current attempt's log.
type LogSink = Arc<Mutex<Option<File>>>;

// Independent facts about one process, each set once by its own event.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug)]
struct Worker {
    serial: u64,
    child: Child,
    /// Frames for its standard input; `None` once closed.
    input: Option<Sender<Vec<u8>>>,
    log: LogSink,
    ready: bool,
    /// Jobs given to it.
    served: u64,
    job: Option<Attempt>,
    stdout_done: bool,
    stderr_done: bool,
    status: Option<ExitStatus>,
    reaped_at: Option<Instant>,
    /// Its input was closed while it had no job.
    closed_at: Option<Instant>,
    /// It violated the protocol or failed: never given another job.
    spent: bool,
}

/// One attempt at one observer, running on a worker.
#[derive(Debug)]
struct Attempt {
    key: JobKey,
    number: u64,
    /// The job's token: dropped when the attempt is finished.
    token: Token,
    started: Instant,
    queued_ns: u64,
    estimate: u64,
    /// Largest sampled resident size while it ran.
    peak: u64,
    /// The worker's first job (its `VmHWM` is the job's own).
    first: bool,
    log: PathBuf,
    /// How it ends, once the coordinator knows and waits for the process.
    ending: Option<Ending>,
}

#[derive(Debug, Clone)]
enum Ending {
    /// A crash (§5.4 row 2): the message is the reason; the log's end is
    /// added when the attempt is finished.
    Crash(String),
    /// Killed at its cap; retried (alone) or not.
    Cap { retry: bool, reason: String },
    /// Killed for pressure: requeued, not an attempt.
    Pressure,
    /// Killed for the time limit.
    Time(String),
}

/// How an attempt is finished.
#[derive(Debug)]
enum Finish {
    Done {
        shard: Digest,
        status: Status,
        cpu_ns: u64,
        peak_rss: u64,
    },
    /// The worker answered `Failed`.
    Failed(String),
    Ended(Ending),
}

#[derive(Debug, Clone, Copy, Default)]
struct JobState {
    /// The attempt number of the next run.
    attempt: u64,
    crash_retried: bool,
    /// The final shard's status, once there is one.
    status: Option<Status>,
}

struct Coordinator<'a> {
    store: &'a Store,
    plan: Digest,
    observers: &'a [Observer],
    phase: Phase,
    round: u64,
    worker: &'a WorkerCommand,
    options: &'a Options,
    tokens: &'a jobserver::Pool,
    sampler: &'a dyn Sampler,
    jobs: usize,
    interval: Duration,
    worker_jobs: u64,

    tx: Sender<Event>,
    admission: Admission,
    state: Vec<JobState>,
    workers: Vec<Worker>,
    next_serial: u64,
    request: Option<jobserver::Request>,
    running: usize,
    cancelled: bool,

    t0: Instant,
    started_unix_ns: u64,
    records: Vec<JobRecord>,
    workers_max: u64,
    peak_rss_sum: u64,
}

/// Runs `phase`, round `round`, of plan `plan_id` over `observers` (in plan
/// order) and publishes the run record (§5.3, §5.4, §6, §7). Every observer
/// ends with a shard in the store, unless the run is cancelled. A
/// `PoolError::Store` or `Io` is an error of the store's file system (a log
/// or a coordinator-written shard that cannot be written); the workers are
/// killed and a run record with the remaining jobs `cancelled` is
/// published (best effort) before it is returned.
#[allow(clippy::too_many_arguments)]
pub fn run(
    store: &Store,
    plan_id: &Digest,
    observers: &[Observer],
    phase: Phase,
    round: u64,
    worker: &WorkerCommand,
    options: &Options,
    tokens: &jobserver::Pool,
    sampler: &dyn Sampler,
) -> Result<Summary, PoolError> {
    let t0 = Instant::now();
    let started_unix_ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX));
    let previous = previous_peaks(store, plan_id, phase)?;
    let (tx, rx) = mpsc::channel();
    let _active = ActiveGuard::register(tx.clone());
    let mut c = Coordinator {
        store,
        plan: *plan_id,
        observers,
        phase,
        round,
        worker,
        options,
        tokens,
        sampler,
        jobs: options.jobs.max(1).min(tokens.jobs().max(1)),
        interval: options.sample_interval.max(Duration::from_millis(1)),
        worker_jobs: options.worker_jobs.max(1),
        tx,
        admission: Admission::new(options.admission, &previous, observers),
        state: vec![JobState { attempt: 1, ..JobState::default() }; observers.len()],
        workers: Vec::new(),
        next_serial: 0,
        request: None,
        running: 0,
        cancelled: false,
        t0,
        started_unix_ns,
        records: Vec::new(),
        workers_max: 0,
        peak_rss_sum: 0,
    };
    match c.main_loop(&rx) {
        Ok(()) => c.summary(),
        Err(e) => {
            c.abandon();
            Err(e)
        }
    }
}

/// The peak resident size of each observer in the most recent run record
/// of `phase` (§6 "Estimate"); empty if there is none or it is not valid.
fn previous_peaks(
    store: &Store,
    plan: &Digest,
    phase: Phase,
) -> Result<HashMap<Digest, u64>, PoolError> {
    let mut out = HashMap::new();
    let Some(bytes) = store.latest_run(plan, phase)? else { return Ok(out) };
    let Ok(record) = RunRecord::from_bytes(&bytes) else { return Ok(out) };
    for j in &record.jobs {
        let e = out.entry(j.observer).or_insert(0);
        *e = (*e).max(j.peak_rss);
    }
    Ok(out)
}

fn ns(d: Duration) -> u64 {
    u64::try_from(d.as_nanos()).unwrap_or(u64::MAX)
}

impl Coordinator<'_> {
    fn is_cancelled(&self) -> bool {
        self.cancelled || signals::interrupted().is_some()
    }

    fn main_loop(&mut self, rx: &Receiver<Event>) -> Result<(), PoolError> {
        let mut next_tick = Instant::now() + self.interval;
        loop {
            if self.is_cancelled() {
                return Err(PoolError::Cancelled);
            }
            self.close_idle_workers();
            self.maybe_request();
            if self.admission.pending_len() == 0 && self.running == 0 && self.workers.is_empty() {
                return Ok(());
            }
            let now = Instant::now();
            let mut wait = next_tick.saturating_duration_since(now);
            if self.workers.iter().any(Worker::ending) {
                wait = wait.min(ENDING_POLL);
            }
            match rx.recv_timeout(wait) {
                Ok(ev) => {
                    self.handle(ev)?;
                    while let Ok(ev) = rx.try_recv() {
                        self.handle(ev)?;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    unreachable!("pool: the coordinator holds a sender of its own channel")
                }
            }
            if self.is_cancelled() {
                return Err(PoolError::Cancelled);
            }
            if Instant::now() >= next_tick {
                self.tick();
                next_tick = Instant::now() + self.interval;
            }
            self.reap()?;
        }
    }

    fn handle(&mut self, ev: Event) -> Result<(), PoolError> {
        match ev {
            Event::Token(token) => {
                self.request = None;
                if self.is_cancelled() || self.running >= self.jobs {
                    return Ok(());
                }
                if let Some(pick) = self.admission.pick() {
                    self.start(pick, token)?;
                }
            }
            Event::Message { worker, message } => self.on_message(worker, message)?,
            Event::Stdout { worker } => {
                if let Some(w) = self.workers.iter_mut().find(|w| w.serial == worker) {
                    w.stdout_done = true;
                }
            }
            Event::Stderr { worker } => {
                if let Some(w) = self.workers.iter_mut().find(|w| w.serial == worker) {
                    w.stderr_done = true;
                }
            }
            Event::Cancel => self.cancelled = true,
        }
        Ok(())
    }

    /// Asks for a token when a job could start: fewer than `-j` running and
    /// admission would accept one now. At most one request is outstanding.
    fn maybe_request(&mut self) {
        if self.request.is_some()
            || self.running >= self.jobs
            || self.admission.peek().is_none()
            || self.is_cancelled()
        {
            return;
        }
        let tx = self.tx.clone();
        self.request = Some(self.tokens.request(move |t| {
            // A send to a loop that has ended drops (releases) the token.
            let _ = tx.send(Event::Token(t));
        }));
    }

    /// Starts `pick` on an idle worker, or a new one, holding `token`.
    fn start(&mut self, pick: Pick, token: Token) -> Result<(), PoolError> {
        let key = pick.job;
        let attempt = self.state[key].attempt;
        let observer = &self.observers[key];
        let log = self.store.log_path(&self.plan, self.phase, self.round, &observer.id, attempt)?;
        let file = File::create(&log)?;
        let job = Job {
            store: self.store.root().as_os_str().as_encoded_bytes().to_vec(),
            plan: self.plan,
            phase: self.phase,
            round: self.round,
            observer: observer.clone(),
        };
        let frame = ToWorker::Job(Box::new(job)).encode();
        let now = Instant::now();
        let mut attempt = Attempt {
            key,
            number: attempt,
            token,
            started: now,
            queued_ns: ns(now - self.t0),
            estimate: pick.estimate,
            peak: 0,
            first: true,
            log,
            ending: None,
        };
        self.running += 1;
        self.workers_max = self.workers_max.max(self.running as u64);
        let idle = self.workers.iter().position(Worker::idle);
        let index = match idle {
            Some(i) => i,
            None => match self.spawn() {
                Ok(w) => {
                    self.workers.push(w);
                    self.workers.len() - 1
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted || process::shutting_down() => {
                    self.cancelled = true;
                    self.running -= 1;
                    // Not started: back where it was, for the cancelled record.
                    self.admission.finished(key, true);
                    return Ok(());
                }
                Err(e) => {
                    let _ = (&file)
                        .write_all(format!("slip: cannot start the worker: {e}\n").as_bytes());
                    drop(file);
                    let reason =
                        format!("cannot start the worker {}: {e}", self.worker.program.display());
                    attempt.ending = Some(Ending::Crash(reason.clone()));
                    return self.finish(attempt, Finish::Ended(Ending::Crash(reason)));
                }
            },
        };
        let w = &mut self.workers[index];
        attempt.first = w.served == 0;
        *w.log.lock().unwrap_or_else(PoisonError::into_inner) = Some(file);
        w.served += 1;
        if let Some(input) = &w.input {
            let _ = input.send(frame);
        }
        if w.served >= self.worker_jobs {
            // Its last job: end of input after it.
            w.input = None;
        }
        w.job = Some(attempt);
        Ok(())
    }

    fn spawn(&mut self) -> io::Result<Worker> {
        let mut cmd = Command::new(&self.worker.program);
        cmd.args(&self.worker.args);
        for v in &self.worker.env_remove {
            cmd.env_remove(v);
        }
        let limits = Limits {
            address_space: self.admission.address_space_cap(),
            timeout: None,
            rss_cap: None,
        };
        let mut child = Child::spawn(&cmd, Io::piped(), &limits)?;
        let (Some(mut stdin), Some(mut stdout), Some(mut stderr)) =
            (child.take_stdin(), child.take_stdout(), child.take_stderr())
        else {
            return Err(io::Error::other("the worker's standard streams are not piped"));
        };
        self.next_serial += 1;
        let serial = self.next_serial;
        let log: LogSink = Arc::new(Mutex::new(None));

        let (input_tx, input_rx) = mpsc::channel::<Vec<u8>>();
        std::thread::Builder::new().name("slip-worker-in".into()).spawn(move || {
            for frame in input_rx {
                if write_frame(&mut stdin, &frame).is_err() {
                    break;
                }
            }
            // Dropping stdin is the worker's end of input.
        })?;

        let tx = self.tx.clone();
        std::thread::Builder::new().name("slip-worker-out".into()).spawn(move || {
            loop {
                match read_frame(&mut stdout, FRAME_MAX) {
                    Ok(Some(payload)) => {
                        let message = ToCoordinator::decode(&payload)
                            .map_err(|e| format!("a message that does not decode: {e}"));
                        let bad = message.is_err();
                        if tx.send(Event::Message { worker: serial, message }).is_err() || bad {
                            break;
                        }
                    }
                    Ok(None) => break,
                    Err(e) => {
                        let message = Err(format!("unreadable output: {e}"));
                        let _ = tx.send(Event::Message { worker: serial, message });
                        break;
                    }
                }
            }
            let _ = tx.send(Event::Stdout { worker: serial });
        })?;

        let tx = self.tx.clone();
        let sink = Arc::clone(&log);
        std::thread::Builder::new().name("slip-worker-err".into()).spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match stderr.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if let Some(f) =
                            sink.lock().unwrap_or_else(PoisonError::into_inner).as_mut()
                        {
                            let _ = f.write_all(&buf[..n]);
                        }
                    }
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                    Err(_) => break,
                }
            }
            let _ = tx.send(Event::Stderr { worker: serial });
        })?;

        Ok(Worker {
            serial,
            child,
            input: Some(input_tx),
            log,
            ready: false,
            served: 0,
            job: None,
            stdout_done: false,
            stderr_done: false,
            status: None,
            reaped_at: None,
            closed_at: None,
            spent: false,
        })
    }

    fn on_message(
        &mut self,
        serial: u64,
        message: Result<ToCoordinator, String>,
    ) -> Result<(), PoolError> {
        let Some(i) = self.workers.iter().position(|w| w.serial == serial) else { return Ok(()) };
        let w = &self.workers[i];
        let ready = w.ready;
        // (job, being ended already)
        let job = w.job.as_ref().map(|a| (a.key, a.ending.is_some()));
        if w.spent && job.is_none_or(|j| j.1) {
            // Already judged; what it says now changes nothing.
            return Ok(());
        }
        let message = match message {
            Err(why) => {
                self.violation(i, &why);
                return Ok(());
            }
            Ok(m) => m,
        };
        match message {
            ToCoordinator::Ready { protocol, .. } => {
                if ready {
                    self.violation(i, "a second Ready");
                } else if protocol != PROTOCOL {
                    self.violation(i, &format!("Ready for protocol {protocol}, not {PROTOCOL}"));
                } else {
                    self.workers[i].ready = true;
                }
            }
            _ if !ready => self.violation(i, "a message before Ready"),
            ToCoordinator::Done { observer, shard, cpu_ns, peak_rss } => {
                let Some((key, ending)) = job else {
                    self.violation(i, "Done while it had no job");
                    return Ok(());
                };
                if ending {
                    // Being killed: that decision stands.
                    return Ok(());
                }
                if observer != self.observers[key].id {
                    self.violation(i, &format!("Done for observer {observer}, not its job's"));
                    return Ok(());
                }
                match self.check_shard(key, &shard) {
                    Ok(status) => {
                        let a = self.take_job(i);
                        self.finish(a, Finish::Done { shard, status, cpu_ns, peak_rss })?;
                    }
                    Err(why) => self.violation(i, &why),
                }
            }
            ToCoordinator::Failed { observer, message } => {
                let Some((key, ending)) = job else {
                    self.violation(i, "Failed while it had no job");
                    return Ok(());
                };
                if ending {
                    return Ok(());
                }
                if observer != self.observers[key].id {
                    self.violation(i, &format!("Failed for observer {observer}, not its job's"));
                    return Ok(());
                }
                // Not reused: its state is suspect.
                let w = &mut self.workers[i];
                w.spent = true;
                w.input = None;
                let a = self.take_job(i);
                self.finish(a, Finish::Failed(message))?;
            }
        }
        Ok(())
    }

    fn take_job(&mut self, i: usize) -> Attempt {
        self.workers[i]
            .job
            .take()
            .unwrap_or_else(|| unreachable!("pool: the attempt just looked at is gone"))
    }

    /// The shard named by a `Done` is in the store, is a valid `SHRD` with
    /// that content id, for the job's observer, phase and round (§5.2).
    fn check_shard(&self, key: JobKey, id: &Digest) -> Result<Status, String> {
        let observer = &self.observers[key].id;
        let bytes = match self.store.read_shard(&self.plan, self.phase, self.round, observer) {
            Ok(Some(b)) => b,
            Ok(None) => return Err(format!("Done names shard {id}, which is not in the store")),
            Err(e) => return Err(format!("Done names shard {id}, which cannot be read: {e}")),
        };
        match content_id(&bytes) {
            Ok(got) if got == *id => {}
            Ok(got) => return Err(format!("Done names shard {id}, but the store holds {got}")),
            Err(e) => return Err(format!("Done names shard {id}, which is not valid: {e}")),
        }
        let shard =
            Shard::from_bytes(&bytes).map_err(|e| format!("shard {id} is not valid: {e}"))?;
        if shard.observer != *observer || shard.phase != self.phase || shard.round != self.round {
            return Err(format!("shard {id} is not for this observer, phase and round"));
        }
        Ok(shard.outcome.status)
    }

    /// A protocol violation (§5.2): treated as a crash of its job, if it
    /// has one; the worker is killed and never reused.
    fn violation(&mut self, i: usize, why: &str) {
        let w = &mut self.workers[i];
        w.spent = true;
        w.input = None;
        w.child.kill(Killed::Signal);
        if let Some(a) = &mut w.job
            && a.ending.is_none()
        {
            a.ending = Some(Ending::Crash(format!("protocol violation: {why}")));
            self.admission.stopping(a.key);
        }
    }

    /// Samples the running workers, applies the time limit and the
    /// admission decisions (§6).
    fn tick(&mut self) {
        let mut samples = Vec::new();
        let mut sum = 0u64;
        for w in &mut self.workers {
            if w.status.is_some() {
                continue;
            }
            let Some(a) = &mut w.job else { continue };
            if a.ending.is_some() {
                continue;
            }
            let s = w.child.sample(self.sampler);
            a.peak = a.peak.max(s.rss).max(if a.first { s.hwm } else { 0 });
            sum = sum.saturating_add(s.rss);
            if let Some(limit) = self.options.observer_timeout
                && a.started.elapsed() >= limit
            {
                w.child.kill(Killed::Time);
                a.ending = Some(Ending::Time(format!(
                    "killed: exceeded the observer timeout of {}s",
                    limit.as_secs_f64()
                )));
                self.admission.stopping(a.key);
                continue;
            }
            samples.push((a.key, s.rss));
        }
        self.peak_rss_sum = self.peak_rss_sum.max(sum);
        let cap = self.admission.rss_cap().unwrap_or(0);
        for d in self.admission.on_samples(&samples) {
            let (key, ending) = match d {
                Decision::KillAtCap { job, retry } => {
                    let rss = samples.iter().find(|s| s.0 == job).map_or(0, |s| s.1);
                    let reason = format!(
                        "killed: resident size {rss} bytes exceeded the cap of {cap} bytes"
                    );
                    (job, Ending::Cap { retry, reason })
                }
                Decision::KillForPressure { job, .. } => (job, Ending::Pressure),
            };
            for w in &mut self.workers {
                if let Some(a) = &mut w.job
                    && a.key == key
                    && a.ending.is_none()
                {
                    w.child.kill(Killed::Memory);
                    a.ending = Some(ending);
                    break;
                }
            }
        }
    }

    /// Reaps exited workers and finishes the attempts of the ones that are
    /// gone; kills a closed worker that does not exit.
    fn reap(&mut self) -> Result<(), PoolError> {
        let mut i = 0;
        while i < self.workers.len() {
            let w = &mut self.workers[i];
            if w.status.is_none() {
                if let Some(s) = w.child.try_wait()? {
                    w.status = Some(s);
                    w.reaped_at = Some(Instant::now());
                } else if w.closed_at.is_some_and(|t| t.elapsed() >= EXIT_GRACE) {
                    w.child.kill(Killed::Signal);
                }
            }
            let gone = w
                .reaped_at
                .is_some_and(|t| (w.stdout_done && w.stderr_done) || t.elapsed() >= PIPE_GRACE);
            if !gone {
                i += 1;
                continue;
            }
            let mut w = self.workers.swap_remove(i);
            *w.log.lock().unwrap_or_else(PoisonError::into_inner) = None;
            if let Some(a) = w.job.take() {
                let ending = a.ending.clone().unwrap_or_else(|| {
                    Ending::Crash(match w.status {
                        Some(s) => format!("the worker ended without answering ({s})"),
                        None => "the worker ended without answering".to_owned(),
                    })
                });
                self.finish(a, Finish::Ended(ending))?;
            }
        }
        Ok(())
    }

    /// Closes the input of idle workers once no pending job is left for
    /// them, so they exit, and starts the exit grace of every worker with
    /// no job whose input is closed.
    fn close_idle_workers(&mut self) {
        let none_pending = self.admission.pending_len() == 0;
        for w in &mut self.workers {
            if w.job.is_none() && w.closed_at.is_none() && (none_pending || w.input.is_none()) {
                w.input = None;
                w.closed_at = Some(Instant::now());
            }
        }
    }

    /// Records the attempt and applies §5.4: retry, requeue, or the final
    /// shard (the worker's, or a `failed` one written here).
    fn finish(&mut self, a: Attempt, how: Finish) -> Result<(), PoolError> {
        self.running -= 1;
        let key = a.key;
        let observer = self.observers[key].id;
        let wall_ns = ns(a.started.elapsed());
        let mut record = JobRecord {
            observer,
            attempt: a.number,
            end: End::Crashed,
            shard: None,
            queued_ns: a.queued_ns,
            wall_ns,
            cpu_ns: 0,
            peak_rss: a.peak,
            estimate: a.estimate,
        };
        // (requeue for a crash retry, message of a final failed shard)
        let (retry, failure) = match how {
            Finish::Done { shard, status, cpu_ns, peak_rss } => {
                record.end = End::Completed;
                record.shard = Some(shard);
                record.cpu_ns = cpu_ns;
                record.peak_rss = a.peak.max(peak_rss);
                self.state[key].status = Some(status);
                self.store.remove_log(&self.plan, self.phase, self.round, &observer, a.number)?;
                (false, None)
            }
            Finish::Failed(message) => {
                let message = if message.is_empty() {
                    "the worker failed without a message".to_owned()
                } else {
                    clip_message(&message).to_owned()
                };
                self.crash(key, message)
            }
            Finish::Ended(Ending::Crash(reason)) => {
                let message = with_log_tail(&reason, &a.log);
                self.crash(key, message)
            }
            Finish::Ended(Ending::Cap { retry, reason }) => {
                record.end = End::KilledMemory;
                if retry {
                    self.state[key].attempt += 1;
                    (false, None)
                } else {
                    (false, Some(with_log_tail(&reason, &a.log)))
                }
            }
            Finish::Ended(Ending::Pressure) => {
                // Not an attempt and not a failure: same attempt number.
                record.end = End::KilledMemory;
                self.store.remove_log(&self.plan, self.phase, self.round, &observer, a.number)?;
                (false, None)
            }
            Finish::Ended(Ending::Time(reason)) => {
                record.end = End::KilledTime;
                (false, Some(with_log_tail(&reason, &a.log)))
            }
        };
        if let Some(message) = failure {
            record.shard = Some(self.write_failed(key, &message)?);
            self.state[key].status = Some(Status::Failed);
        }
        self.records.push(record);
        self.admission.finished(key, retry);
        drop(a.token);
        Ok(())
    }

    /// A crashed attempt: retried once, then failed. Returns
    /// `(retry, message of the failed shard)`.
    fn crash(&mut self, key: JobKey, message: String) -> (bool, Option<String>) {
        let s = &mut self.state[key];
        if s.crash_retried {
            (false, Some(message))
        } else {
            s.crash_retried = true;
            s.attempt += 1;
            (true, None)
        }
    }

    /// Publishes the `failed` shard of §5.4 and returns its content id.
    fn write_failed(&self, key: JobKey, message: &str) -> Result<Digest, PoolError> {
        let observer = self.observers[key].id;
        let shard = Shard {
            observer,
            phase: self.phase,
            round: self.round,
            outcome: Outcome {
                status: Status::Failed,
                errors: 0,
                warnings: 0,
                message: clip_message(message).to_owned(),
            },
            inputs: None,
            counters: Vec::new(),
        };
        Ok(self.store.publish_shard(
            &self.plan,
            self.phase,
            self.round,
            &observer,
            &shard.to_bytes(),
        )?)
    }

    fn run_record(&self) -> RunRecord {
        RunRecord {
            run: Run {
                tool_slip: self.options.tool_slip.clone(),
                tool_kiln: self.options.tool_kiln.clone(),
                plan: self.plan,
                phase: self.phase,
                started_unix_ns: self.started_unix_ns,
                wall_ns: ns(self.t0.elapsed()),
                cores: jobserver::available_cores() as u64,
                workers_max: self.workers_max,
                memory_budget: self.options.admission.budget,
                peak_rss_sum: self.peak_rss_sum,
            },
            jobs: self.records.clone(),
        }
    }

    fn summary(&self) -> Result<Summary, PoolError> {
        let run = self.run_record();
        let run_path = self.store.publish_run(
            &self.plan,
            self.phase,
            self.round,
            self.started_unix_ns,
            &run.to_bytes(),
        )?;
        let mut statuses = [0u64; 4];
        for s in &self.state {
            match s.status {
                Some(st) => statuses[st as usize] += 1,
                None => unreachable!("pool: a finished run left an observer without a shard"),
            }
        }
        let mut ends = [0u64; 5];
        for j in &run.jobs {
            ends[j.end as usize] += 1;
        }
        Ok(Summary { run, run_path, statuses, ends })
    }

    /// The run did not finish (cancelled or an error): kills the workers,
    /// waits for them (bounded), records every remaining job `cancelled`
    /// and publishes the run record, ignoring errors.
    fn abandon(&mut self) {
        self.request = None;
        for w in &mut self.workers {
            w.input = None;
            w.child.kill(Killed::Signal);
        }
        let deadline = Instant::now() + CANCEL_REAP;
        for w in &mut self.workers {
            while w.status.is_none() {
                match w.child.try_wait() {
                    Ok(Some(s)) => w.status = Some(s),
                    Ok(None) if Instant::now() < deadline => std::thread::sleep(ENDING_POLL),
                    _ => break,
                }
            }
        }
        let elapsed = ns(self.t0.elapsed());
        let mut cancelled = Vec::new();
        for w in &mut self.workers {
            *w.log.lock().unwrap_or_else(PoisonError::into_inner) = None;
            if let Some(a) = w.job.take() {
                cancelled.push((
                    a.key,
                    a.number,
                    a.queued_ns,
                    ns(a.started.elapsed()),
                    a.peak,
                    a.estimate,
                ));
            }
        }
        for key in self.admission.pending() {
            let attempt = self.state[key].attempt;
            cancelled.push((key, attempt, elapsed, 0, 0, self.admission.estimate(key)));
        }
        for (key, attempt, queued_ns, wall_ns, peak_rss, estimate) in cancelled {
            let observer = self.observers[key].id;
            let _ = self.store.remove_log(&self.plan, self.phase, self.round, &observer, attempt);
            self.records.push(JobRecord {
                observer,
                attempt,
                end: End::Cancelled,
                shard: None,
                queued_ns,
                wall_ns,
                cpu_ns: 0,
                peak_rss,
                estimate,
            });
        }
        let run = self.run_record();
        let _ = self.store.publish_run(
            &self.plan,
            self.phase,
            self.round,
            self.started_unix_ns,
            &run.to_bytes(),
        );
        // The workers (and their tokens) are dropped with the coordinator.
    }
}

impl Worker {
    /// Can be given a job now.
    fn idle(&self) -> bool {
        self.job.is_none()
            && self.input.is_some()
            && !self.spent
            && self.status.is_none()
            && !self.stdout_done
    }

    /// Known to be on its way out: the loop polls it more often.
    fn ending(&self) -> bool {
        self.reaped_at.is_some()
            || self.stdout_done
            || self.closed_at.is_some()
            || self.job.as_ref().is_some_and(|a| a.ending.is_some())
    }
}

/// `reason`, then as much of the end of the log as fits in a shard message.
fn with_log_tail(reason: &str, log: &Path) -> String {
    let mut message = clip_message(reason).to_owned();
    let tail = read_tail(log, MESSAGE_MAX as u64);
    let tail = tail.trim_end();
    if tail.is_empty() || message.len() + 1 >= MESSAGE_MAX {
        return message;
    }
    let room = MESSAGE_MAX - message.len() - 1;
    let mut start = tail.len().saturating_sub(room);
    while !tail.is_char_boundary(start) {
        start += 1;
    }
    message.push('\n');
    message.push_str(&tail[start..]);
    message
}

/// The last `n` bytes of the file, lossily decoded; empty if unreadable.
fn read_tail(path: &Path, n: u64) -> String {
    let Ok(mut f) = File::open(path) else { return String::new() };
    let len = f.metadata().map_or(0, |m| m.len());
    if f.seek(SeekFrom::Start(len.saturating_sub(n))).is_err() {
        return String::new();
    }
    let mut buf = Vec::new();
    let _ = f.take(n).read_to_end(&mut buf);
    String::from_utf8_lossy(&buf).into_owned()
}
