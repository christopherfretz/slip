//! `spec/exec.md` §7 end to end. Ported from Kiln's
//! `xtask/src/jobs/signal_tests.rs` (WP-1436): each test re-executes this
//! test binary as a probe that installs the signal path, connects to a
//! private fifo of [`POOL`] tokens made by the test, and holds tokens and
//! `sh` stub children in one phase; the test signals the probe (a process it
//! started) and asserts that the probe died of that signal within the
//! deadline, that no stub is left running, and that the fifo holds exactly
//! [`POOL`] tokens again. The probe never sees `MAKEFLAGS`, so it cannot
//! reach the host's pool. Synchronisation is by polling with a generous
//! deadline.
#![cfg(target_os = "linux")]

mod common;

use slip_exec::jobserver::{self, Pool};
use slip_exec::process::{Child, Io, Limits};
use slip_exec::signals::{self, Timing};
use std::io::Read as _;
use std::io::Write as _;
use std::os::unix::process::ExitStatusExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

const PROBE: &str = "SLIP_EXEC_SIGNAL_PROBE";
const PROBE_DIR: &str = "SLIP_EXEC_SIGNAL_PROBE_DIR";
const PROBE_MS: &str = "SLIP_EXEC_SIGNAL_PROBE_MS";
const SIGINT: i32 = 2;
const SIGTERM: i32 = 15;

/// The probe's pool.
const POOL: usize = 4;

/// How long any one wait of these tests may take (a loaded machine).
const DEADLINE: Duration = Duration::from_mins(1);

/// The re-executed side: does nothing in a normal test run (no [`PROBE`]).
#[test]
fn probe() {
    let (Ok(phase), Some(dir)) = (std::env::var(PROBE), std::env::var_os(PROBE_DIR)) else {
        return;
    };
    let dir = PathBuf::from(dir);
    let timing = std::env::var(PROBE_MS).ok().map_or_else(Timing::default, |v| {
        let ms: Vec<u64> = v.split(',').map(|s| s.parse().unwrap()).collect();
        Timing {
            reap: Duration::from_millis(ms[0]),
            bound: Duration::from_millis(ms[1]),
            duplicate: Duration::from_millis(ms[2]),
        }
    });
    let fifo = dir.join("fifo");
    let pool = Pool::from_fifo(&fifo).unwrap();
    let report = dir.join("cleanup");
    let cleanup: Box<dyn FnOnce() + Send> = match phase.as_str() {
        // The store's cleanup cannot finish (a lock, a full disk).
        "stuck" => Box::new(|| std::thread::sleep(Duration::from_mins(10))),
        "panicking" => Box::new(|| panic!("the cleanup callback failed")),
        // Records what it finds: the tokens are back by now.
        _ => Box::new(move || {
            let idle = jobserver::status(&fifo).map_or(usize::MAX, |n| n);
            let _ = std::fs::write(&report, idle.to_string());
        }),
    };
    assert!(signals::install_with(cleanup, timing));
    assert!(!signals::install(Box::new(|| {})), "installed twice");
    let workers = if phase == "between" { 2 } else { 6 };
    let (dir, pool, phase): (&'static Path, &'static Pool, &'static str) = (
        Box::leak(dir.into_boxed_path()),
        Box::leak(Box::new(pool)),
        Box::leak(phase.into_boxed_str()),
    );
    for i in 0..workers {
        std::thread::spawn(move || {
            if phase == "between" {
                // Short children back to back: the signal lands between
                // a child's exit and the next spawn, or in a spawn.
                let script = format!("echo $$ >> {}", dir.join("spawned").display());
                loop {
                    let _t = pool.acquire();
                    let mut c = Command::new("sh");
                    c.args(["-c", &script, &marker(dir)]);
                    let Ok(mut c) = Child::spawn(&c, Io::null(), &Limits::default()) else {
                        return;
                    };
                    let _ = c.wait();
                }
            }
            // One job per worker: 5 run (implicit + 4), the 6th waits.
            let t = pool.acquire();
            let Ok(mut c) = Child::spawn(&stub(dir, i), Io::null(), &Limits::default()) else {
                return;
            };
            let _ = c.wait();
            // A scheduler that never gets to drop its tokens: only the
            // signal path can return them.
            loop {
                std::thread::park();
                let _ = &t;
            }
        });
    }
    // What `main` does once its command returns (here: once interrupted).
    while signals::interrupted().is_none() {
        std::thread::sleep(Duration::from_millis(10));
    }
    let _ = signals::finish(std::process::ExitCode::SUCCESS);
}

/// The `$0` of every stub shell, so a pid is only taken for ours.
fn marker(dir: &Path) -> String {
    format!("slip-signal-probe-{}", dir.file_name().unwrap().to_string_lossy())
}

/// `sh -c 'echo $$ > <dir>/stub<i>.pid; while :; do sleep 0.1; done'`.
fn stub(dir: &Path, i: usize) -> Command {
    let pid_file = dir.join(format!("stub{i}.pid"));
    let script = format!("echo $$ > {}; while :; do sleep 0.1; done", pid_file.display());
    let mut c = Command::new("sh");
    c.args(["-c", &script, &marker(dir)]);
    c
}

#[derive(Default)]
struct Case {
    phase: &'static str,
    signal: i32,
    /// stderr is a pipe whose reader is gone when the signal arrives.
    dead_reader: bool,
    /// (reap, bound, duplicate) in milliseconds.
    timing: Option<(u64, u64, u64)>,
    /// Deliver the signal a second time, once the cleanup is under way.
    twice: bool,
}

struct Outcome {
    status: ExitStatus,
    stderr: String,
    /// From the (last) signal to the probe's end.
    after_signal: Duration,
    /// What the cleanup callback found, if it ran.
    cleanup: Option<String>,
}

/// Kills and reaps the probe and any stub still running on every exit path
/// of a test (they are all processes this test started, directly or
/// through the probe), then removes the directory.
struct Guard {
    probe: Option<std::process::Child>,
    dir: PathBuf,
}

impl Drop for Guard {
    fn drop(&mut self) {
        if let Some(mut p) = self.probe.take() {
            let _ = p.kill();
            let _ = p.wait();
        }
        let marker = marker(&self.dir);
        for pid in pids(&self.dir) {
            if alive(pid, &marker) {
                let _ = Command::new("kill").args(["-KILL", &pid.to_string()]).status();
            }
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn wait_for(what: &str, g: &mut Guard, mut ready: impl FnMut() -> bool) {
    let start = Instant::now();
    while !ready() {
        if let Ok(Some(s)) = g.probe.as_mut().unwrap().try_wait() {
            panic!("the probe ended ({s}) before {what}");
        }
        assert!(start.elapsed() < DEADLINE, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn send(pid: u32, sig: i32) {
    let ok = Command::new("kill").args([format!("-{sig}"), pid.to_string()]).status();
    assert!(ok.is_ok_and(|s| s.success()), "kill -{sig} {pid}");
}

/// Our stub with that pid is still running (not gone, not a zombie).
fn alive(pid: u32, marker: &str) -> bool {
    let proc = PathBuf::from(format!("/proc/{pid}"));
    let state = std::fs::read_to_string(proc.join("stat")).unwrap_or_default();
    let zombie = state.rsplit_once(')').is_some_and(|(_, rest)| rest.trim_start().starts_with('Z'));
    let cmdline = std::fs::read(proc.join("cmdline")).unwrap_or_default();
    !zombie && String::from_utf8_lossy(&cmdline).contains(marker)
}

fn pids(dir: &Path) -> Vec<u32> {
    let mut v = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return v };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if e.path().extension().is_some_and(|x| x == "pid") || name == "spawned" {
            let text = std::fs::read_to_string(e.path()).unwrap_or_default();
            v.extend(text.lines().filter_map(|l| l.trim().parse::<u32>().ok()));
        }
    }
    v
}

/// The probe has a thread of that name (Linux keeps 15 bytes of it).
fn has_thread(pid: u32, name: &str) -> bool {
    let name = &name[..name.len().min(15)];
    let Ok(tasks) = std::fs::read_dir(format!("/proc/{pid}/task")) else { return false };
    tasks
        .flatten()
        .any(|t| std::fs::read_to_string(t.path().join("comm")).is_ok_and(|c| c.trim_end() == name))
}

fn run(case: &Case) -> Outcome {
    let tag = format!(
        "{}-{}-{}{}{}",
        std::process::id(),
        case.phase,
        case.signal,
        if case.dead_reader { "-dead" } else { "" },
        if case.twice { "-twice" } else { "" },
    );
    let dir = common::work_root().join(format!("slip-exec-signal-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut g = Guard { probe: None, dir: dir.clone() };
    let fifo = dir.join("fifo");
    assert!(Command::new("mkfifo").arg(&fifo).status().unwrap().success(), "mkfifo");
    let mut holder = std::fs::OpenOptions::new().read(true).write(true).open(&fifo).unwrap();
    holder.write_all(&[b'+'; POOL]).unwrap();

    let mut cmd = Command::new(std::env::current_exe().unwrap());
    cmd.args(["--exact", "probe", "--nocapture", "--test-threads=1", "-q"])
        .env(PROBE, case.phase)
        .env(PROBE_DIR, &dir)
        .env_remove("MAKEFLAGS")
        .env_remove(PROBE_MS)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    if let Some((reap, bound, dup)) = case.timing {
        cmd.env(PROBE_MS, format!("{reap},{bound},{dup}"));
    }
    g.probe = Some(cmd.spawn().unwrap());
    let pid = g.probe.as_ref().unwrap().id();
    let mut pipe = g.probe.as_mut().unwrap().stderr.take();

    let phase_ready = || match case.phase {
        "between" => {
            std::fs::read_to_string(dir.join("spawned")).is_ok_and(|t| t.lines().count() >= 20)
        }
        // Five stubs hold the implicit token and the four pool bytes.
        _ => (0..6).filter(|i| dir.join(format!("stub{i}.pid")).exists()).count() == 5,
    };
    wait_for(&format!("phase {}", case.phase), &mut g, phase_ready);
    if case.phase != "between" {
        assert_eq!(jobserver::status(&fifo).unwrap(), 0, "the probe holds the whole pool");
    }
    let reader = if case.dead_reader {
        drop(pipe.take());
        None
    } else {
        let mut p = pipe.take().unwrap();
        Some(std::thread::spawn(move || {
            let mut s = String::new();
            let _ = p.read_to_string(&mut s);
            s
        }))
    };
    send(pid, case.signal);
    let mut signalled = Instant::now();
    if case.twice {
        wait_for("the cleanup thread", &mut g, || has_thread(pid, signals::CLEANUP_THREAD));
        let seen = Instant::now();
        let window = Duration::from_millis(case.timing.map_or(500, |t| t.2) + 100);
        wait_for("the duplicate window", &mut g, || seen.elapsed() > window);
        send(pid, case.signal);
        signalled = Instant::now();
    }
    let status = loop {
        if let Some(s) = g.probe.as_mut().unwrap().try_wait().unwrap() {
            break s;
        }
        assert!(
            signalled.elapsed() < DEADLINE,
            "the probe ({}) outlived its signal by {DEADLINE:?}",
            case.phase
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    g.probe = None;
    let after_signal = signalled.elapsed();
    let stderr = reader.map(|r| r.join().unwrap()).unwrap_or_default();

    // No stub survives (a killed one may stay a zombie until reaped).
    let marker = marker(&dir);
    let ours = pids(&dir);
    assert!(!ours.is_empty(), "no stub pid recorded");
    let start = Instant::now();
    while let Some(p) = ours.iter().find(|&&p| alive(p, &marker)) {
        assert!(start.elapsed() < DEADLINE, "stub {p} survived the probe ({})", case.phase);
        std::thread::sleep(Duration::from_millis(10));
    }
    // Every token is back, exactly once.
    assert_eq!(jobserver::status(&fifo).unwrap(), POOL, "pool after {} ({stderr})", case.phase);
    let cleanup = std::fs::read_to_string(dir.join("cleanup")).ok();
    drop(holder);
    Outcome { status, stderr, after_signal, cleanup }
}

fn killed_by(o: &Outcome, sig: i32) {
    assert_eq!(o.status.signal(), Some(sig), "status {} ({})", o.status, o.stderr);
}

/// §7: children killed and reaped, tokens returned (before the callback,
/// which sees the pool whole), one line, the signal's default action.
#[test]
fn s7_sigterm_while_jobs_run() {
    let o = run(&Case { phase: "jobs", signal: SIGTERM, ..Case::default() });
    killed_by(&o, SIGTERM);
    assert_eq!(o.cleanup.as_deref(), Some("4"), "the callback ran after the tokens came back");
    let lines: Vec<&str> = o.stderr.lines().filter(|l| l.starts_with("slip:")).collect();
    assert_eq!(
        lines,
        ["slip: signal 15: killed 5 child process(es), returned 4 jobserver token(s)"],
        "{}",
        o.stderr
    );
}

#[test]
fn s7_sigint_while_jobs_run() {
    let o = run(&Case { phase: "jobs", signal: SIGINT, ..Case::default() });
    killed_by(&o, SIGINT);
    assert_eq!(o.cleanup.as_deref(), Some("4"));
}

/// §7 "No write to standard error on these paths may panic or block the
/// exit" (Kiln's WP-1436 hang: EPIPE on the message).
#[test]
fn s7_sigterm_with_stderr_reader_gone() {
    let o = run(&Case { phase: "jobs", signal: SIGTERM, dead_reader: true, ..Case::default() });
    killed_by(&o, SIGTERM);
    assert_eq!(o.cleanup.as_deref(), Some("4"));
}

#[test]
fn s7_sigint_with_stderr_reader_gone() {
    let o = run(&Case { phase: "jobs", signal: SIGINT, dead_reader: true, ..Case::default() });
    killed_by(&o, SIGINT);
}

/// §4 "Table", §7: a signal between a child's exit and the next spawn, or
/// during a spawn: the late child is killed, later spawns fail.
#[test]
fn s7_sigterm_between_spawns() {
    let o = run(&Case { phase: "between", signal: SIGTERM, ..Case::default() });
    killed_by(&o, SIGTERM);
}

/// §7: a panicking callback does not stop the exit or the line.
#[test]
fn s7_panicking_callback_still_exits() {
    let o = run(&Case { phase: "panicking", signal: SIGTERM, ..Case::default() });
    killed_by(&o, SIGTERM);
    assert!(o.stderr.contains("the cleanup callback panicked"), "{}", o.stderr);
}

/// §7: a second signal 0.5 s or more after the first ends the process at
/// once, with the cleanup stuck (60 s bound).
#[test]
fn s7_second_signal_exits_at_once() {
    let o = run(&Case {
        phase: "stuck",
        signal: SIGTERM,
        timing: Some((10_000, 60_000, 500)),
        twice: true,
        ..Case::default()
    });
    killed_by(&o, SIGTERM);
    assert!(o.after_signal < Duration::from_secs(30), "{:?}", o.after_signal);
    assert!(o.stderr.contains("exiting without waiting for cleanup"), "{}", o.stderr);
}

/// §7: "the process ends at the latest 15 s after the first signal" (here
/// with a 1 s bound): a stuck cleanup is cut short.
#[test]
fn s7_stuck_cleanup_is_bounded() {
    let o = run(&Case {
        phase: "stuck",
        signal: SIGINT,
        timing: Some((10_000, 1_000, 500)),
        ..Case::default()
    });
    killed_by(&o, SIGINT);
    assert!(o.after_signal < Duration::from_secs(30), "{:?}", o.after_signal);
    assert!(o.stderr.contains("cleanup still running after 1.0s; exiting anyway"), "{}", o.stderr);
}
