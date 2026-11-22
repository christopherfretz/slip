//! `spec/exec.md` §5–§7 end to end: [`pool::run`] drives `slip-fake-worker`
//! (whose behaviour per job is the observer's `argv[1]`) under a
//! [`FakeSampler`], against a store and, where tokens matter, a fifo made by
//! the test. The signal tests re-execute this binary as a probe that runs
//! the pool and is then signalled. Every run is bounded ([`run_bounded`]),
//! every wait is on a condition with a deadline, and every test removes its
//! directory. The rules beyond the spec are in `spec/landed/exec/WP-009.md`.
#![cfg(target_os = "linux")]

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::fs;
use std::io::{Read as _, Write as _};
use std::os::unix::process::ExitStatusExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use slip_exec::admission::{Config, GIB};
use slip_exec::jobserver::{self, Pool};
use slip_exec::pool::{self, Options, PoolError, Summary, WorkerCommand};
use slip_exec::process::{Sample, Sampler};
use slip_exec::signals;
use slip_exec::store::{Access, Store};
use slip_model::formats::content_id;
use slip_model::{
    Command as Cmd, Digest, End, FileRef, Language, Observer, Origin, Phase, RunRecord, Shard,
    Status,
};

const MIB: u64 = 1 << 20;

/// How long any one run or wait of these tests may take (a loaded machine).
const DEADLINE: Duration = Duration::from_mins(2);

/// The fake worker's stack (`--stack`), also the admission's worker stack.
const STACK: u64 = 16 * MIB;

// ---------------------------------------------------------------------------
// Helpers

struct Scratch(PathBuf);

impl std::ops::Deref for Scratch {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn scratch(tag: &str) -> Scratch {
    static N: AtomicUsize = AtomicUsize::new(0);
    let n = N.fetch_add(1, Ordering::SeqCst);
    let d = std::env::temp_dir().join(format!("slip-exec-pool-{}-{tag}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    Scratch(d)
}

const PLAN: Digest = Digest([0x55; 32]);

/// Observer `i` with the fake behaviour `argv[1..]` (unique by directory).
fn observer(i: usize, args: &[&str]) -> Observer {
    let mut words = vec![b"fake".to_vec()];
    words.extend(args.iter().map(|a| a.as_bytes().to_vec()));
    let command = Cmd { directory: format!("/d{i}").into_bytes(), argv: words, env: vec![] };
    Observer {
        id: command.observer_id(),
        command,
        language: Language::C,
        main: FileRef {
            path: format!("/d{i}/m.c").into_bytes(),
            size: 100,
            content: Digest([7; 32]),
        },
        cost_hint: 100,
        origins: vec![Origin { configuration: 0, entry: i as u64 }],
    }
}

fn observers(behaviours: &[&str]) -> Vec<Observer> {
    behaviours.iter().enumerate().map(|(i, b)| observer(i, &[b])).collect()
}

fn fake_worker(jobs: u64) -> WorkerCommand {
    WorkerCommand {
        program: PathBuf::from(env!("CARGO_BIN_EXE_slip-fake-worker")),
        args: vec![
            "--jobs".into(),
            jobs.to_string().into(),
            "--stack".into(),
            STACK.to_string().into(),
        ],
        env_remove: vec!["MAKEFLAGS".into()],
    }
}

/// [`fake_worker`] with `--scratch 1`: every job leaves `tmp/` entries
/// behind unless the worker ends normally.
fn scratch_worker(jobs: u64) -> WorkerCommand {
    let mut w = fake_worker(jobs);
    w.args.extend(["--scratch".into(), "1".into()]);
    w
}

/// Admission with estimates of 1 MiB per job.
fn config(budget: u64, observer_memory: Option<u64>) -> Config {
    Config {
        budget,
        estimate_base: MIB,
        estimate_per_byte: 0,
        observer_memory,
        worker_stack: STACK,
    }
}

fn options(jobs: usize, admission: Config) -> Options {
    let mut o = Options::new(jobs, admission);
    o.sample_interval = Duration::from_millis(10);
    o.observer_timeout = Some(Duration::from_mins(1));
    o.tool_slip = "slip-test".into();
    o.tool_kiln = "kiln-test".into();
    o
}

/// What the sampler reports.
#[derive(Debug, Clone)]
enum Mode {
    /// `/proc/<pid>/status`, as the real sampler.
    Real,
    /// Every process has this resident size.
    Constant(u64),
    /// 1 MiB until the process's scratch directory `<dir>/<pid>.d` exists,
    /// this resident size from then on (for `--scratch 1` workers that must
    /// make their `tmp/` entries before they are killed).
    AfterScratch(PathBuf, u64),
}

/// A sampler that reports [`Mode`] and remembers, per pid sampled, the
/// process's `limits` and `environ` (read at its first sample).
#[derive(Debug)]
struct FakeSampler {
    mode: Mode,
    seen: Mutex<HashMap<u32, (String, Vec<u8>)>>,
}

impl FakeSampler {
    fn new(mode: Mode) -> Arc<Self> {
        Arc::new(Self { mode, seen: Mutex::new(HashMap::new()) })
    }

    fn pids(&self) -> usize {
        self.seen.lock().unwrap().len()
    }
}

impl Sampler for FakeSampler {
    fn sample(&self, pid: u32) -> Sample {
        self.seen.lock().unwrap().entry(pid).or_insert_with(|| {
            (
                fs::read_to_string(format!("/proc/{pid}/limits")).unwrap_or_default(),
                fs::read(format!("/proc/{pid}/environ")).unwrap_or_default(),
            )
        });
        match &self.mode {
            Mode::Real => slip_exec::process::ProcSampler.sample(pid),
            Mode::Constant(v) => Sample { rss: *v, hwm: *v },
            Mode::AfterScratch(dir, v) => {
                let v = if dir.join(format!("{pid}.d")).exists() { *v } else { MIB };
                Sample { rss: v, hwm: v }
            }
        }
    }
}

struct Run {
    store: Store,
    result: Result<Summary, PoolError>,
}

/// [`pool::run`] on a thread, failing the test if it does not return within
/// [`DEADLINE`].
fn run_bounded(
    store: Store,
    obs: &[Observer],
    worker: &WorkerCommand,
    options: &Options,
    tokens: &Pool,
    sampler: &Arc<FakeSampler>,
) -> Run {
    run_bounded_round(store, obs, 0, worker, options, tokens, sampler)
}

/// [`run_bounded`] at round `round`.
fn run_bounded_round(
    store: Store,
    obs: &[Observer],
    round: u64,
    worker: &WorkerCommand,
    options: &Options,
    tokens: &Pool,
    sampler: &Arc<FakeSampler>,
) -> Run {
    let (obs, worker, options, tokens, sampler) =
        (obs.to_vec(), worker.clone(), options.clone(), tokens.clone(), Arc::clone(sampler));
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = pool::run(
            &store,
            &PLAN,
            &obs,
            Phase::Map,
            round,
            &worker,
            &options,
            &tokens,
            &*sampler,
        );
        let _ = tx.send(Run { store, result });
    });
    rx.recv_timeout(DEADLINE).expect("pool::run did not return within the deadline")
}

fn records_of(run: &RunRecord, o: &Observer) -> Vec<slip_model::Job> {
    run.jobs.iter().filter(|j| j.observer == o.id).cloned().collect()
}

fn shard_of(store: &Store, o: &Observer) -> Shard {
    let bytes = store.read_shard(&PLAN, Phase::Map, 0, &o.id).unwrap().expect("a shard");
    Shard::from_bytes(&bytes).unwrap()
}

fn log_of(store: &Store, o: &Observer, attempt: u64) -> PathBuf {
    store.log_path(&PLAN, Phase::Map, 0, &o.id, attempt).unwrap()
}

/// The checks every finished run passes (§1 principle 5, §5.4, `spec/model.md`
/// §9): the published record is the summary's and validates; every
/// observer's records are its attempts in order, only the last carrying the
/// shard, which is the one in the store; the counts add up; no more than
/// `jobs` attempts overlap.
fn check_run(s: &Summary, store: &Store, obs: &[Observer], jobs: usize) {
    let bytes = fs::read(&s.run_path).unwrap();
    let published = RunRecord::from_bytes(&bytes).unwrap();
    assert_eq!(published, s.run);
    published.validate().unwrap();
    assert_eq!(s.run.run.plan, PLAN);
    assert_eq!(s.run.run.phase, Phase::Map);
    assert_eq!(s.run.run.round, 0);
    assert_eq!(s.run.run.tool_slip, "slip-test");
    assert!(s.run_path.starts_with(store.root().join("runs")));
    for o in obs {
        let rs = records_of(&s.run, o);
        assert!(!rs.is_empty(), "observer {} has no record", o.id);
        let (last, earlier) = rs.split_last().unwrap();
        for w in rs.windows(2) {
            assert!(w[0].attempt <= w[1].attempt && w[1].attempt <= w[0].attempt + 1, "{rs:?}");
        }
        assert_eq!(rs[0].attempt, 1);
        assert!(earlier.iter().all(|j| j.shard.is_none()), "{rs:?}");
        let id = last.shard.expect("the last attempt carries the shard");
        let stored = store.read_shard(&PLAN, Phase::Map, 0, &o.id).unwrap().unwrap();
        assert_eq!(content_id(&stored).unwrap(), id);
    }
    // WP-018: one outcome per observer, in the order given, saying what
    // its final shard in the store says.
    assert_eq!(s.outcomes.len(), obs.len());
    for (o, outcome) in obs.iter().zip(&s.outcomes) {
        let shard = shard_of(store, o);
        assert_eq!(outcome.observer, o.id);
        assert_eq!(outcome.status, shard.outcome.status);
        let first = shard.outcome.message.lines().next().unwrap_or_default();
        assert_eq!(outcome.message, first);
    }
    assert_eq!(s.run.jobs.len(), s.ends.iter().sum::<u64>() as usize);
    for (i, n) in s.ends.iter().enumerate() {
        assert_eq!(*n, s.run.jobs.iter().filter(|j| j.end as usize == i).count() as u64);
    }
    assert_eq!(s.statuses.iter().sum::<u64>() as usize, obs.len());
    assert_eq!(s.ends[End::Cancelled as usize], 0);
    // At most `jobs` at once, from the records' own intervals.
    let mut edges: Vec<(u64, i64)> = Vec::new();
    for j in &s.run.jobs {
        edges.push((j.queued_ns, 1));
        edges.push((j.queued_ns + j.wall_ns, -1));
    }
    edges.sort_unstable();
    let (mut now, mut most) = (0i64, 0i64);
    for (_, d) in edges {
        now += d;
        most = most.max(now);
    }
    assert!(most as usize <= jobs, "{most} attempts overlapped with -j {jobs}");
    assert!(s.run.run.workers_max as usize <= jobs);
}

/// A fifo in `dir` holding `tokens` bytes, and the descriptor that keeps it
/// open.
fn fifo(dir: &Path, tokens: usize) -> (PathBuf, fs::File) {
    let path = dir.join("fifo");
    assert!(Command::new("mkfifo").arg(&path).status().unwrap().success(), "mkfifo");
    let mut holder = fs::OpenOptions::new().read(true).write(true).open(&path).unwrap();
    holder.write_all(&vec![b'+'; tokens]).unwrap();
    (path, holder)
}

fn open(dir: &Path) -> Store {
    Store::open(&dir.join("store"), Access::Exclusive).unwrap()
}

// ---------------------------------------------------------------------------
// §5.4: every row, the retry, the logs

/// §5.4 every row with the fake worker: `Done` → completed with the
/// worker's shard; `Failed`, an exit, a signal, a protocol violation →
/// crashed, retried once (two `Job` records), then a `failed` shard written
/// by the coordinator with the worker's message or the end of its log;
/// the time limit → `killed_time`, failed, no retry. §2/§5.2: logs are kept
/// only for failed attempts.
#[test]
fn s5_4_every_row_retry_and_logs() {
    let dir = scratch("rows");
    let behaviours = [
        "ok",
        "diagnosed",
        "fail:boom",
        "panic",
        "exit:3",
        "abort",
        "garbage",
        "wrong-observer",
        "no-shard",
        "hang",
        "late-output",
    ];
    let obs = observers(&behaviours);
    let mut opts = options(4, config(0, None));
    opts.observer_timeout = Some(Duration::from_secs(3));
    let sampler = FakeSampler::new(Mode::Real);
    let r =
        run_bounded(open(&dir), &obs, &fake_worker(1), &opts, &Pool::none().with_jobs(4), &sampler);
    let s = r.result.unwrap();
    let store = &r.store;
    check_run(&s, store, &obs, 4);
    let by: HashMap<&str, &Observer> = behaviours.iter().copied().zip(obs.iter()).collect();
    let ends = |b: &str| -> Vec<(u64, End)> {
        records_of(&s.run, by[b]).iter().map(|j| (j.attempt, j.end)).collect()
    };
    let message = |b: &str| shard_of(store, by[b]).outcome.message;

    for b in ["ok", "late-output"] {
        assert_eq!(ends(b), [(1, End::Completed)], "{b}");
        assert_eq!(shard_of(store, by[b]).outcome.status, Status::Ok);
        assert_eq!(shard_of(store, by[b]).counters.len(), 1, "the worker's shard");
        assert!(
            !log_of(store, by[b], 1).exists(),
            "{b}: the log of a completed attempt is removed"
        );
    }
    assert_eq!(ends("diagnosed"), [(1, End::Completed)]);
    assert_eq!(shard_of(store, by["diagnosed"]).outcome.status, Status::Diagnosed);

    let crashes =
        ["fail:boom", "panic", "exit:3", "abort", "garbage", "wrong-observer", "no-shard"];
    for b in crashes {
        assert_eq!(ends(b), [(1, End::Crashed), (2, End::Crashed)], "{b}: retried once");
        let shard = shard_of(store, by[b]);
        assert_eq!(shard.outcome.status, Status::Failed, "{b}");
        assert_eq!(shard.inputs, None);
        assert!(shard.counters.is_empty());
        for attempt in [1, 2] {
            let log = fs::read_to_string(log_of(store, by[b], attempt)).unwrap();
            assert!(log.contains(&format!("slip-fake-worker: {b}")), "{b} log {attempt}: {log:?}");
        }
    }
    assert_eq!(message("fail:boom"), "boom", "the worker's message");
    assert_eq!(message("panic"), "panicked: fake worker panic");
    let exit = message("exit:3");
    assert!(exit.starts_with("the worker ended without answering (exit status: 3)"), "{exit}");
    assert!(exit.ends_with("slip-fake-worker: exit:3"), "the end of its log: {exit}");
    let abort = message("abort");
    assert!(abort.contains("signal: 9"), "{abort}");
    assert!(abort.ends_with("slip-fake-worker: abort"), "{abort}");
    assert!(message("garbage").starts_with("protocol violation: unreadable output"));
    let wrong = message("wrong-observer");
    assert!(wrong.starts_with("protocol violation: Done for observer"), "{wrong}");
    assert!(message("no-shard").contains("which is not in the store"), "{}", message("no-shard"));

    assert_eq!(ends("hang"), [(1, End::KilledTime)], "no retry");
    let hang = message("hang");
    assert!(hang.starts_with("killed: exceeded the observer timeout of 3s"), "{hang}");
    assert!(hang.ends_with("slip-fake-worker: hang"), "{hang}");
    assert!(log_of(store, by["hang"], 1).exists());
    let hang_job = &records_of(&s.run, by["hang"])[0];
    assert!(hang_job.wall_ns >= 3_000_000_000, "{}", hang_job.wall_ns);

    assert_eq!(s.statuses, [2, 1, 0, 8]);
    assert_eq!(s.ends, [3, 14, 0, 1, 0]);
    // WP-018: the outcomes hold the first line of a message of several.
    let outcome = |b: &str| s.outcomes.iter().find(|x| x.observer == by[b].id).unwrap().clone();
    assert!(message("exit:3").contains('\n'));
    assert_eq!(outcome("exit:3").message, "the worker ended without answering (exit status: 3)");
    assert_eq!(outcome("exit:3").status, Status::Failed);
    assert_eq!(outcome("diagnosed").message, "fake diagnostic");
    assert_eq!(outcome("diagnosed").status, Status::Diagnosed);
    assert_eq!(outcome("ok").message, "");
    let completed = &records_of(&s.run, by["ok"])[0];
    assert!(completed.peak_rss > 0, "the worker's own VmHWM at least");
    drop(r.store);
}

/// §5.2 "protocol violation": a `Ready` for another protocol, and a worker
/// that exits 0 without answering, are crashes; a worker that cannot be
/// started is one too (landed).
#[test]
fn s5_2_ready_protocol_and_silent_exit_are_crashes() {
    let dir = scratch("ready");
    let obs = observers(&["ok"]);
    let opts = options(1, config(0, None));
    let sampler = FakeSampler::new(Mode::Real);
    let cases: [(WorkerCommand, &str); 3] = [
        (
            WorkerCommand {
                program: "sh".into(),
                args: vec![
                    "-c".into(),
                    r"printf '\004\000\000\000\001\002\001t'; cat >/dev/null".into(),
                ],
                env_remove: vec![],
            },
            "protocol violation: Ready for protocol 2, not 1",
        ),
        (
            WorkerCommand {
                program: "sh".into(),
                args: vec!["-c".into(), "exit 0".into()],
                env_remove: vec![],
            },
            "the worker ended without answering (exit status: 0)",
        ),
        (
            WorkerCommand { program: dir.join("no-such-worker"), args: vec![], env_remove: vec![] },
            "cannot start the worker",
        ),
    ];
    let mut store = open(&dir);
    for (worker, want) in &cases {
        let r = run_bounded(store, &obs, worker, &opts, &Pool::none().with_jobs(1), &sampler);
        store = r.store;
        let s = r.result.unwrap();
        check_run(&s, &store, &obs, 1);
        let rs: Vec<(u64, End)> =
            records_of(&s.run, &obs[0]).iter().map(|j| (j.attempt, j.end)).collect();
        assert_eq!(rs, [(1, End::Crashed), (2, End::Crashed)], "{want}");
        let shard = shard_of(&store, &obs[0]);
        assert_eq!(shard.outcome.status, Status::Failed);
        assert!(shard.outcome.message.starts_with(want), "{:?}", shard.outcome.message);
    }
}

// ---------------------------------------------------------------------------
// §5.3, §3: -j, tokens, plan order

/// §5.3, §3: never more than `-j` jobs; the implicit token is used (a pool
/// with no tokens still runs one job at a time); every token is back in the
/// fifo afterwards, after success and after failures.
#[test]
fn s5_3_jobs_bound_implicit_token_and_tokens_returned() {
    // (fifo tokens, -j, expected most at once)
    for (tokens, jobs, most) in [(8, 3, 3), (1, 3, 2), (0, 3, 1)] {
        let dir = scratch("tokens");
        let (path, holder) = fifo(&dir, tokens);
        let pool = Pool::from_fifo(&path).unwrap().with_jobs(8);
        let obs = observers(&["sleep:1000"; 5]);
        let sampler = FakeSampler::new(Mode::Real);
        let r = run_bounded(
            open(&dir),
            &obs,
            &fake_worker(1),
            &options(jobs, config(0, None)),
            &pool,
            &sampler,
        );
        let s = r.result.unwrap();
        check_run(&s, &r.store, &obs, jobs);
        assert_eq!(s.run.run.workers_max, most, "{tokens} tokens, -j {jobs}");
        assert_eq!(s.statuses[Status::Ok as usize], 5);
        assert_eq!(jobserver::status(&path).unwrap(), tokens, "every token is back");
        drop(holder);
    }
}

/// §3 "Release … on every path": tokens come back after crashes, kills and
/// coordinator-written shards too.
#[test]
fn s5_3_tokens_returned_after_failures() {
    let dir = scratch("tokens-fail");
    let (path, holder) = fifo(&dir, 2);
    let pool = Pool::from_fifo(&path).unwrap();
    let obs = observers(&["fail:x", "exit:9", "abort", "garbage", "hang", "ok"]);
    let mut opts = options(3, config(0, None));
    opts.observer_timeout = Some(Duration::from_secs(2));
    let sampler = FakeSampler::new(Mode::Real);
    let r = run_bounded(open(&dir), &obs, &fake_worker(1), &opts, &pool, &sampler);
    let s = r.result.unwrap();
    check_run(&s, &r.store, &obs, 3);
    assert_eq!(s.statuses, [1, 0, 0, 5]);
    assert_eq!(jobserver::status(&path).unwrap(), 2);
    drop(holder);
}

/// §5.3: pending jobs start in plan order (the order given).
#[test]
fn s5_3_plan_order() {
    let dir = scratch("order");
    let obs: Vec<Observer> = (0..6).map(|i| observer(i, &["ok"])).collect();
    let sampler = FakeSampler::new(Mode::Real);
    let r = run_bounded(
        open(&dir),
        &obs,
        &fake_worker(1),
        &options(1, config(0, None)),
        &Pool::none(),
        &sampler,
    );
    let s = r.result.unwrap();
    check_run(&s, &r.store, &obs, 1);
    let order: Vec<Digest> = s.run.jobs.iter().map(|j| j.observer).collect();
    assert_eq!(order, obs.iter().map(|o| o.id).collect::<Vec<_>>());
    assert!(s.run.jobs.windows(2).all(|w| w[0].queued_ns < w[1].queued_ns));
}

/// §5.2 `--worker-jobs` above 1: a worker serves that many jobs, then a new
/// process takes over; a worker that answered `Failed` is not reused
/// (landed); every observer still gets its shard.
#[test]
fn s5_2_worker_jobs_above_one() {
    let dir = scratch("reuse");
    let obs = observers(&["sleep:300"; 7]);
    let mut opts = options(1, config(0, None));
    opts.worker_jobs = 3;
    let sampler = FakeSampler::new(Mode::Real);
    let r = run_bounded(open(&dir), &obs, &fake_worker(3), &opts, &Pool::none(), &sampler);
    let s = r.result.unwrap();
    check_run(&s, &r.store, &obs, 1);
    assert_eq!(s.statuses[Status::Ok as usize], 7);
    assert_eq!(sampler.pids(), 3, "7 jobs, 3 per process");

    let obs = observers(&["ok", "fail:once", "ok", "ok", "sleep:10", "ok"]);
    let sampler = FakeSampler::new(Mode::Real);
    let r = run_bounded(r.store, &obs, &fake_worker(3), &opts, &Pool::none(), &sampler);
    let s = r.result.unwrap();
    check_run(&s, &r.store, &obs, 1);
    assert_eq!(s.statuses, [5, 0, 0, 1]);
    assert_eq!(shard_of(&r.store, &obs[1]).outcome.message, "once");
}

// ---------------------------------------------------------------------------
// §6 through the pool

/// §6 "Caps", "Retry": killed at its cap alone: `killed_memory`, a failed
/// shard, no retry.
#[test]
fn s6_cap_alone_fails() {
    let dir = scratch("cap-alone");
    let obs = observers(&["sleep:30000"]);
    let sampler = FakeSampler::new(Mode::Constant(200 * MIB));
    let opts = options(1, config(GIB, Some(100 * MIB)));
    let r = run_bounded(open(&dir), &obs, &fake_worker(1), &opts, &Pool::none(), &sampler);
    let s = r.result.unwrap();
    check_run(&s, &r.store, &obs, 1);
    let rs = records_of(&s.run, &obs[0]);
    assert_eq!(rs.iter().map(|j| (j.attempt, j.end)).collect::<Vec<_>>(), [(1, End::KilledMemory)]);
    assert_eq!(rs[0].peak_rss, 200 * MIB);
    let shard = shard_of(&r.store, &obs[0]);
    assert_eq!(shard.outcome.status, Status::Failed);
    let m = &shard.outcome.message;
    assert!(
        m.starts_with(&format!(
            "killed: resident size {} bytes exceeded the cap of {} bytes",
            200 * MIB,
            100 * MIB
        )),
        "{m}"
    );
    assert!(log_of(&r.store, &obs[0], 1).exists(), "a failed attempt keeps its log");
}

/// §6 "Retry": killed at the cap while others run: retried once, alone
/// (it overlaps nothing), then fails alone.
#[test]
fn s6_cap_retry_alone() {
    let dir = scratch("cap-retry");
    let obs = observers(&["sleep:30000", "sleep:30000"]);
    let sampler = FakeSampler::new(Mode::Constant(200 * MIB));
    let opts = options(2, config(GIB, Some(100 * MIB)));
    let r = run_bounded(open(&dir), &obs, &fake_worker(1), &opts, &Pool::none(), &sampler);
    let s = r.result.unwrap();
    check_run(&s, &r.store, &obs, 2);
    for o in &obs {
        let rs = records_of(&s.run, o);
        assert_eq!(
            rs.iter().map(|j| (j.attempt, j.end)).collect::<Vec<_>>(),
            [(1, End::KilledMemory), (2, End::KilledMemory)]
        );
        let retry = &rs[1];
        let is_retry = |j: &slip_model::Job| j.observer == o.id && j.attempt == 2;
        for other in s.run.jobs.iter().filter(|j| !is_retry(j)) {
            let (a0, a1) = (retry.queued_ns, retry.queued_ns + retry.wall_ns);
            let (b0, b1) = (other.queued_ns, other.queued_ns + other.wall_ns);
            assert!(a1 <= b0 || b1 <= a0, "the retry ran beside {other:?}: {:#?}", s.run.jobs);
        }
        assert_eq!(shard_of(&r.store, o).outcome.status, Status::Failed);
    }
    assert_eq!(s.statuses, [0, 0, 0, 2]);
}

/// §6 "Pressure": samples summing to more than `B` with two jobs running:
/// the newer is killed (`killed_memory`, no shard, same attempt number),
/// requeued with `e = 2 ×` its sample, which no longer fits beside the
/// other, so it runs after it and completes.
#[test]
fn s6_pressure_requeues_newest() {
    let dir = scratch("pressure");
    let obs = observers(&["sleep:1500", "sleep:300"]);
    let sampler = FakeSampler::new(Mode::Constant(200 * MIB));
    let opts = options(2, config(300 * MIB, Some(GIB)));
    let r = run_bounded(open(&dir), &obs, &fake_worker(1), &opts, &Pool::none(), &sampler);
    let s = r.result.unwrap();
    check_run(&s, &r.store, &obs, 2);
    let a = records_of(&s.run, &obs[0]);
    let b = records_of(&s.run, &obs[1]);
    assert_eq!(a.iter().map(|j| (j.attempt, j.end)).collect::<Vec<_>>(), [(1, End::Completed)]);
    assert_eq!(
        b.iter().map(|j| (j.attempt, j.end, j.shard.is_some())).collect::<Vec<_>>(),
        [(1, End::KilledMemory, false), (1, End::Completed, true)],
        "not an attempt, not a failure"
    );
    assert_eq!(b[0].estimate, MIB);
    assert_eq!(b[1].estimate, 400 * MIB, "2 × its last sample");
    assert!(b[1].queued_ns >= a[0].queued_ns + a[0].wall_ns, "it waited for the other");
    assert!(!log_of(&r.store, &obs[1], 1).exists(), "not a failure: no log kept");
    assert_eq!(s.statuses, [2, 0, 0, 0]);
    assert_eq!(s.run.run.memory_budget, 300 * MIB);
    assert_eq!(s.run.run.peak_rss_sum, 400 * MIB);
}

/// §6 `--memory 0`: no caps, no pressure, whatever the samples.
#[test]
fn s6_budget_zero_never_kills() {
    let dir = scratch("budget0");
    let obs = observers(&["sleep:200", "sleep:200", "sleep:200"]);
    let sampler = FakeSampler::new(Mode::Constant(1 << 50));
    let r = run_bounded(
        open(&dir),
        &obs,
        &fake_worker(1),
        &options(3, config(0, Some(MIB))),
        &Pool::none(),
        &sampler,
    );
    let s = r.result.unwrap();
    check_run(&s, &r.store, &obs, 3);
    assert_eq!(s.statuses, [3, 0, 0, 0]);
    // No address-space cap either: the worker has this process's limit.
    let address_space = |limits: &str| -> String {
        limits.lines().find(|l| l.starts_with("Max address space")).unwrap().to_owned()
    };
    let ours = address_space(&fs::read_to_string("/proc/self/limits").unwrap());
    let seen = sampler.seen.lock().unwrap();
    assert!(!seen.is_empty());
    for (limits, _) in seen.values() {
        assert_eq!(address_space(limits), ours);
    }
}

/// §6 "Estimate" rule 1 through the store: the second run's estimates are
/// the first run's peaks × 1.25; with no previous record, rule 2.
#[test]
fn s6_estimate_from_previous_run_record() {
    let dir = scratch("estimate");
    let obs = observers(&["ok", "ok"]);
    let sampler = FakeSampler::new(Mode::Real);
    let opts = options(1, config(0, None));
    let r = run_bounded(open(&dir), &obs, &fake_worker(1), &opts, &Pool::none(), &sampler);
    let first = r.result.unwrap();
    assert!(first.run.jobs.iter().all(|j| j.estimate == MIB), "rule 2: base + 0 × size");
    let r = run_bounded(r.store, &obs, &fake_worker(1), &opts, &Pool::none(), &sampler);
    let second = r.result.unwrap();
    for o in &obs {
        let peak = records_of(&first.run, o)[0].peak_rss;
        assert!(peak > 0);
        assert_eq!(records_of(&second.run, o)[0].estimate, peak + peak / 4);
    }
    assert_ne!(first.run_path, second.run_path);
}

/// `spec/model.md` §9, `spec/exec.md` §2: the run record names the round
/// it ran, and is published under that round.
#[test]
fn s2_run_record_carries_its_round() {
    let dir = scratch("round");
    let obs = observers(&["ok"]);
    let sampler = FakeSampler::new(Mode::Real);
    let opts = options(1, config(0, None));
    let r = run_bounded_round(open(&dir), &obs, 3, &fake_worker(1), &opts, &Pool::none(), &sampler);
    let s = r.result.unwrap();
    assert_eq!(s.run.run.round, 3);
    let published = RunRecord::from_bytes(&fs::read(&s.run_path).unwrap()).unwrap();
    assert_eq!(published.run.round, 3);
    let started = published.run.started_unix_ns;
    assert_eq!(s.run_path, r.store.run_path(&PLAN, Phase::Map, 3, started));
    assert!(r.store.read_shard(&PLAN, Phase::Map, 3, &obs[0].id).unwrap().is_some());
}

/// §4, §6 "Caps": the worker runs under the address-space cap (twice the
/// resident cap plus the stack plus 1 GiB); §5.1: `env_remove` is applied.
#[test]
fn s6_address_space_cap_and_env_remove() {
    let dir = scratch("limits");
    let obs = observers(&["sleep:400"]);
    let sampler = FakeSampler::new(Mode::Constant(MIB));
    let mut worker = fake_worker(1);
    let var = "CARGO_MANIFEST_DIR";
    assert!(std::env::var_os(var).is_some(), "cargo sets {var} for tests");
    worker.env_remove.push(var.into());
    let r = run_bounded(
        open(&dir),
        &obs,
        &worker,
        &options(1, config(GIB, Some(100 * MIB))),
        &Pool::none(),
        &sampler,
    );
    let s = r.result.unwrap();
    check_run(&s, &r.store, &obs, 1);
    let seen = sampler.seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    let (limits, environ) = seen.values().next().unwrap();
    let cap = 2 * 100 * MIB + STACK + GIB;
    let line = limits.lines().find(|l| l.starts_with("Max address space")).unwrap();
    assert!(line.contains(&format!(" {cap} ")), "{line} (want {cap})");
    let vars: Vec<&[u8]> = environ.split(|&b| b == 0).collect();
    assert!(!vars.is_empty());
    assert!(
        !vars.iter().any(|v| v.starts_with(format!("{var}=").as_bytes())),
        "{var} was inherited"
    );
}

// ---------------------------------------------------------------------------
// §2 "Cleaning": the pool removes the tmp/ entries of the workers it reaps
// (WP-018)

/// The entries every `scratch` line of the attempt's log names: they were
/// made (the line is written after them), and none exists any more.
fn assert_scratch_removed(store: &Store, o: &Observer, attempt: u64) {
    let log = fs::read_to_string(log_of(store, o, attempt)).unwrap();
    let lines: Vec<&str> =
        log.lines().filter_map(|l| l.strip_prefix("slip-fake-worker: scratch ")).collect();
    assert_eq!(lines.len(), 1, "attempt {attempt} made its scratch: {log:?}");
    for path in lines[0].split(' ') {
        assert!(Path::new(path).starts_with(store.root().join("tmp")), "{path}");
        assert!(!Path::new(path).exists(), "{path} is left after the run");
    }
}

/// The store's `tmp/` is empty.
fn assert_tmp_empty(store: &Store) {
    let tmp: Vec<OsString> =
        fs::read_dir(store.root().join("tmp")).unwrap().flatten().map(|e| e.file_name()).collect();
    assert!(tmp.is_empty(), "tmp/ is clean once run returns: {tmp:?}");
}

/// §2 "Cleaning": a worker that crashed (died by a signal, exited without
/// answering, panicked), violated the protocol or was killed for the time
/// limit leaves no `tmp/` entry once `run` returns: the pool removes them
/// when it reaps the worker.
#[test]
fn s2_cleaning_crashed_violating_and_timed_out_workers() {
    let dir = scratch("clean-crash");
    let behaviours = ["abort", "exit:3", "panic", "garbage", "hang", "ok"];
    let obs = observers(&behaviours);
    let mut opts = options(4, config(0, None));
    opts.observer_timeout = Some(Duration::from_secs(3));
    let sampler = FakeSampler::new(Mode::Real);
    let r = run_bounded(
        open(&dir),
        &obs,
        &scratch_worker(1),
        &opts,
        &Pool::none().with_jobs(4),
        &sampler,
    );
    let s = r.result.unwrap();
    check_run(&s, &r.store, &obs, 4);
    assert_eq!(s.statuses, [1, 0, 0, 5]);
    for (b, o) in behaviours.iter().zip(&obs) {
        let attempts: &[u64] = match *b {
            "ok" => &[],
            "hang" => &[1],
            _ => &[1, 2],
        };
        for &attempt in attempts {
            assert_scratch_removed(&r.store, o, attempt);
        }
    }
    assert_tmp_empty(&r.store);
}

/// §2 "Cleaning", §6 "Caps": a worker killed at its memory cap leaves no
/// `tmp/` entry.
#[test]
fn s2_cleaning_worker_killed_at_its_cap() {
    let dir = scratch("clean-cap");
    let store = open(&dir);
    let obs = observers(&["sleep:30000"]);
    let sampler = FakeSampler::new(Mode::AfterScratch(store.root().join("tmp"), 200 * MIB));
    let opts = options(1, config(GIB, Some(100 * MIB)));
    let r = run_bounded(store, &obs, &scratch_worker(1), &opts, &Pool::none(), &sampler);
    let s = r.result.unwrap();
    check_run(&s, &r.store, &obs, 1);
    let ends: Vec<End> = records_of(&s.run, &obs[0]).iter().map(|j| j.end).collect();
    assert_eq!(ends, [End::KilledMemory]);
    assert_scratch_removed(&r.store, &obs[0], 1);
    assert_tmp_empty(&r.store);
}

/// §2 "Cleaning", §6 "Pressure": a worker killed for pressure (and
/// requeued) leaves no `tmp/` entry.
#[test]
fn s2_cleaning_worker_killed_for_pressure() {
    let dir = scratch("clean-pressure");
    let store = open(&dir);
    let obs = observers(&["sleep:1500", "sleep:300"]);
    let sampler = FakeSampler::new(Mode::AfterScratch(store.root().join("tmp"), 200 * MIB));
    let opts = options(2, config(300 * MIB, Some(GIB)));
    let r = run_bounded(store, &obs, &scratch_worker(1), &opts, &Pool::none(), &sampler);
    let s = r.result.unwrap();
    check_run(&s, &r.store, &obs, 2);
    assert!(s.ends[End::KilledMemory as usize] >= 1, "a kill for pressure: {:#?}", s.run.jobs);
    assert_eq!(s.statuses, [2, 0, 0, 0]);
    assert_tmp_empty(&r.store);
}

// ---------------------------------------------------------------------------
// §7: signals delivered to a process running the pool

const PROBE: &str = "SLIP_EXEC_POOL_PROBE";
const SIGINT: i32 = 2;
const SIGTERM: i32 = 15;
/// The probe's fifo.
const POOL_TOKENS: usize = 4;
/// The probe's `-j`.
const PROBE_JOBS: usize = 3;

/// The re-executed side: runs the pool over six `hang` jobs with `-j 3`
/// under the signal path, with workers that make `tmp/` entries. Does
/// nothing in a normal test run.
#[test]
fn probe() {
    let Some(dir) = std::env::var_os(PROBE) else { return };
    let dir = PathBuf::from(dir);
    let store = open(&dir);
    let pool = Pool::from_fifo(&dir.join("fifo")).unwrap();
    assert!(signals::install(Box::new(pool::cleanup_on_signal(&store))));
    let obs = observers(&["hang"; 6]);
    let mut opts = options(PROBE_JOBS, config(0, None));
    opts.observer_timeout = None;
    let result = pool::run(
        &store,
        &PLAN,
        &obs,
        Phase::Map,
        0,
        &scratch_worker(1),
        &opts,
        &pool,
        &slip_exec::process::ProcSampler,
    );
    assert!(matches!(result, Err(PoolError::Cancelled)), "{result:?}");
    let _ = signals::finish(std::process::ExitCode::SUCCESS);
    unreachable!("finish ends an interrupted process");
}

/// Our fake worker with that pid is alive (not gone, not a zombie).
fn worker_alive(pid: u32) -> bool {
    let proc = PathBuf::from(format!("/proc/{pid}"));
    let stat = fs::read_to_string(proc.join("stat")).unwrap_or_default();
    let zombie = stat.rsplit_once(')').is_some_and(|(_, rest)| rest.trim_start().starts_with('Z'));
    let cmdline = fs::read(proc.join("cmdline")).unwrap_or_default();
    !zombie && String::from_utf8_lossy(&cmdline).contains("slip-fake-worker")
}

/// The fake workers whose parent is `parent`.
fn workers_of(parent: u32) -> Vec<u32> {
    let mut out = Vec::new();
    for e in fs::read_dir("/proc").unwrap().flatten() {
        let Some(pid) = e.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else {
            continue;
        };
        let stat = fs::read_to_string(e.path().join("stat")).unwrap_or_default();
        let parent_pid = stat
            .rsplit_once(')')
            .and_then(|(_, rest)| rest.split_whitespace().nth(1)?.parse::<u32>().ok());
        if parent_pid == Some(parent) && worker_alive(pid) {
            out.push(pid);
        }
    }
    out
}

/// Kills and reaps the probe on every exit path of a test.
struct Probe(Option<std::process::Child>);

impl Drop for Probe {
    fn drop(&mut self) {
        if let Some(mut p) = self.0.take() {
            let _ = p.kill();
            let _ = p.wait();
        }
    }
}

fn wait_for(what: &str, probe: &mut Probe, mut ready: impl FnMut() -> bool) {
    let start = Instant::now();
    while !ready() {
        if let Ok(Some(s)) = probe.0.as_mut().unwrap().try_wait() {
            panic!("the probe ended ({s}) before {what}");
        }
        assert!(start.elapsed() < DEADLINE, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// §7 for a process running the pool: it dies by the signal; its workers
/// are gone; every token is back; the lock and `tmp/` are clean (the
/// workers' entries included, §2 "Cleaning"); a run
/// record exists with every job `cancelled` (the three running and the
/// three pending), and no log is left.
fn interrupted_run(signal: i32) {
    let dir = scratch(&format!("signal-{signal}"));
    let (fifo_path, holder) = fifo(&dir, POOL_TOKENS);
    let mut cmd = Command::new(std::env::current_exe().unwrap());
    cmd.args(["--exact", "probe", "--nocapture", "--test-threads=1", "-q"])
        .env(PROBE, &*dir)
        .env_remove("MAKEFLAGS")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let mut probe = Probe(Some(cmd.spawn().unwrap()));
    let pid = probe.0.as_ref().unwrap().id();
    let mut stderr = probe.0.as_mut().unwrap().stderr.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        s
    });
    let store_dir = dir.join("store");
    let logs = store_dir.join("logs").join(PLAN.to_string()).join("map.0");
    let running = || {
        fs::read_dir(&logs).map_or(0, |d| {
            d.flatten()
                .filter(|e| {
                    fs::read_to_string(e.path()).is_ok_and(|t| t.contains("slip-fake-worker: hang"))
                })
                .count()
        })
    };
    wait_for("three running jobs", &mut probe, || running() == PROBE_JOBS);
    // Each running worker has its scratch directory and loose file (§2,
    // WP-018), which the signal path must remove.
    let entries = || fs::read_dir(store_dir.join("tmp")).map_or(0, |d| d.flatten().count());
    wait_for("the workers' tmp/ entries", &mut probe, || entries() >= 2 * PROBE_JOBS);
    let workers = workers_of(pid);
    assert_eq!(workers.len(), PROBE_JOBS, "{workers:?}");
    assert_eq!(
        jobserver::status(&fifo_path).unwrap(),
        POOL_TOKENS - (PROBE_JOBS - 1),
        "implicit + 2 held"
    );
    assert!(store_dir.join("lock").exists());

    assert!(
        Command::new("kill")
            .args([format!("-{signal}"), pid.to_string()])
            .status()
            .unwrap()
            .success()
    );
    let signalled = Instant::now();
    let status = loop {
        if let Some(s) = probe.0.as_mut().unwrap().try_wait().unwrap() {
            break s;
        }
        assert!(signalled.elapsed() < DEADLINE, "the probe outlived its signal");
        std::thread::sleep(Duration::from_millis(10));
    };
    probe.0 = None;
    let stderr = reader.join().unwrap();
    assert_eq!(status.signal(), Some(signal), "{status} ({stderr})");
    // One line (spec §7). How many children and tokens the signal path
    // itself found depends on whether the cancelled pool got there first;
    // the fifo below is the real check.
    let lines: Vec<&str> = stderr.lines().filter(|l| l.starts_with("slip:")).collect();
    assert_eq!(lines.len(), 1, "{stderr}");
    assert!(lines[0].starts_with(&format!("slip: signal {signal}: killed ")), "{stderr}");
    assert!(lines[0].ends_with(" jobserver token(s)"), "{stderr}");
    let start = Instant::now();
    while let Some(w) = workers.iter().find(|&&w| worker_alive(w)) {
        assert!(start.elapsed() < DEADLINE, "worker {w} survived the probe");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(jobserver::status(&fifo_path).unwrap(), POOL_TOKENS, "every token is back");
    assert!(!store_dir.join("lock").exists(), "the lock is removed");
    let tmp: Vec<_> =
        fs::read_dir(store_dir.join("tmp")).unwrap().flatten().map(|e| e.file_name()).collect();
    assert!(tmp.is_empty(), "tmp/ is clean: {tmp:?}");
    let runs: Vec<PathBuf> = fs::read_dir(store_dir.join("runs").join(PLAN.to_string()))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .collect();
    assert_eq!(runs.len(), 1, "{runs:?}");
    let record = RunRecord::from_bytes(&fs::read(&runs[0]).unwrap()).unwrap();
    let obs = observers(&["hang"; 6]);
    let mut seen = HashSet::new();
    for j in &record.jobs {
        assert_eq!((j.attempt, j.end, j.shard), (1, End::Cancelled, None), "{j:?}");
        assert!(seen.insert(j.observer), "one record per observer");
    }
    assert_eq!(seen, obs.iter().map(|o| o.id).collect::<HashSet<_>>());
    let left: Vec<OsString> =
        fs::read_dir(&logs).unwrap().flatten().map(|e| e.file_name()).collect();
    assert!(left.is_empty(), "the logs of cancelled attempts are removed: {left:?}");
    drop(holder);
}

#[test]
fn s7_sigint_while_the_pool_runs() {
    interrupted_run(SIGINT);
}

#[test]
fn s7_sigterm_while_the_pool_runs() {
    interrupted_run(SIGTERM);
}
