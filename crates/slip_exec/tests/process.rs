//! `spec/exec.md` §4: process groups, caps, kill reasons, sampling, the
//! table. Ported in part from the tests of Kiln's `xtask/src/util/process.rs`.
//! Every child is a `sh`/`sleep` started here; each is killed and reaped on
//! every exit path by `Child`'s `Drop`, and only those are ever signalled.
#![cfg(target_os = "linux")]

use slip_exec::process::{
    self, Child, Io, Killed, Limits, ProcSampler, SAMPLE_INTERVAL, Sample, Sampler,
};
use std::io::Read as _;
use std::os::unix::process::ExitStatusExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// A temporary directory, removed on every exit path.
struct Scratch(PathBuf);

impl std::ops::Deref for Scratch {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn scratch(tag: &str) -> Scratch {
    static N: AtomicUsize = AtomicUsize::new(0);
    let n = N.fetch_add(1, Ordering::SeqCst);
    let d =
        std::env::temp_dir().join(format!("slip-exec-process-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    Scratch(d)
}

fn sh(script: &str) -> Command {
    let mut c = Command::new("sh");
    c.args(["-c", script]);
    c
}

fn stdout_only() -> Io {
    Io { stdin: Stdio::null(), stdout: Stdio::piped(), stderr: Stdio::null() }
}

/// Runs `cmd` to completion and returns its stdout and status.
fn output(cmd: &Command, limits: &Limits) -> (String, std::process::ExitStatus) {
    let mut c = Child::spawn(cmd, stdout_only(), limits).unwrap();
    let mut out = String::new();
    c.take_stdout().unwrap().read_to_string(&mut out).unwrap();
    (out, c.wait().unwrap())
}

/// `/proc/<pid>/stat` field 5 (the process group), and whether the process
/// is a zombie.
fn stat(pid: u32) -> Option<(u32, bool)> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = text.rsplit_once(')')?.1;
    let mut f = rest.split_whitespace();
    let state = f.next()?;
    let _ppid = f.next()?;
    Some((f.next()?.parse().ok()?, state == "Z"))
}

/// `pid` is a zombie whose parent is this process (an unreaped child).
fn our_zombie(pid: u32) -> bool {
    let Ok(text) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else { return false };
    let Some((_, rest)) = text.rsplit_once(')') else { return false };
    let mut f = rest.split_whitespace();
    f.next() == Some("Z")
        && f.next().and_then(|p| p.parse::<u32>().ok()) == Some(std::process::id())
}

/// A process we started (identified by `marker` in its command line) is
/// still running.
fn alive(pid: u32, marker: &str) -> bool {
    let cmdline = std::fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
    String::from_utf8_lossy(&cmdline).contains(marker) && stat(pid).is_some_and(|(_, z)| !z)
}

fn eventually(what: &str, mut cond: impl FnMut() -> bool) {
    let t = Instant::now();
    while !cond() {
        assert!(t.elapsed() < Duration::from_secs(20), "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// A sampler reporting a fixed size.
struct Fixed(u64);
impl Sampler for Fixed {
    fn sample(&self, _: u32) -> Sample {
        Sample { rss: self.0, hwm: self.0 }
    }
}

/// §4 "Own process group": the child leads a new group (pgid = pid), with
/// and without the address-space wrapper (whose `exec` keeps the pid).
#[test]
fn s4_child_leads_its_own_group() {
    for limits in [Limits::default(), Limits { address_space: Some(4 << 30), ..Limits::default() }]
    {
        let mut c = Child::spawn(&sh("exec sleep 30"), Io::null(), &limits).unwrap();
        let pid = c.id();
        eventually("the child to start", || stat(pid).is_some());
        let (pgrp, _) = stat(pid).unwrap();
        assert_eq!(pgrp, pid, "{limits:?}");
        assert_ne!(pgrp, stat(std::process::id()).unwrap().0);
        c.kill(Killed::Time);
        assert_eq!(c.wait().unwrap().signal(), Some(9));
    }
}

/// §4 "Killing": `SIGKILL` reaches the whole group, the grandchild too.
#[test]
fn s4_kill_reaches_the_whole_group() {
    let dir = scratch("group");
    let pidfile = dir.join("grandchild");
    let marker = format!("slip-group-{}", std::process::id());
    let script = format!("sh -c 'sleep 30; : {marker}' & echo $! > '{}'; wait", pidfile.display());
    let mut c = Child::spawn(&sh(&script), Io::null(), &Limits::default()).unwrap();
    eventually("the grandchild", || {
        std::fs::read_to_string(&pidfile).is_ok_and(|t| t.ends_with('\n'))
    });
    let g: u32 = std::fs::read_to_string(&pidfile).unwrap().trim().parse().unwrap();
    eventually("the grandchild to run", || alive(g, &marker));
    c.kill(Killed::Memory);
    let s = c.wait().unwrap();
    assert_eq!(s.signal(), Some(9));
    assert_eq!(c.killed(), Some(Killed::Memory));
    eventually("the grandchild to die", || !alive(g, &marker));
}

/// §4 "Killing": a child that ends by itself, even by a signal, has no kill
/// reason; one the supervisor kills has the reason recorded; the first
/// reason wins; killing a reaped child does nothing.
#[test]
fn s4_kill_reasons_are_recorded() {
    let mut c = Child::spawn(&sh("exit 3"), Io::null(), &Limits::default()).unwrap();
    assert_eq!(c.wait().unwrap().code(), Some(3));
    assert_eq!(c.killed(), None);
    c.kill(Killed::Time);
    assert_eq!(c.killed(), None, "a reaped child is not killed");

    let mut c = Child::spawn(&sh("kill -9 $$"), Io::null(), &Limits::default()).unwrap();
    assert_eq!(c.wait().unwrap().signal(), Some(9));
    assert_eq!(c.killed(), None, "died by itself");

    let mut c = Child::spawn(&sh("sleep 30"), Io::null(), &Limits::default()).unwrap();
    c.kill(Killed::Memory);
    c.kill(Killed::Time);
    assert_eq!(c.wait().unwrap().signal(), Some(9));
    assert_eq!(c.killed(), Some(Killed::Memory));
}

/// §4, §5.4 `killed_time`: the supervision loop kills at the time limit.
#[test]
fn s4_timeout_kills_with_reason_time() {
    let limits = Limits { timeout: Some(Duration::from_millis(200)), ..Limits::default() };
    let mut c = Child::spawn(&sh("sleep 30"), Io::null(), &limits).unwrap();
    let e = c.wait_supervised(&ProcSampler, SAMPLE_INTERVAL).unwrap();
    assert_eq!(e.killed, Some(Killed::Time));
    assert_eq!(e.status.signal(), Some(9));
    assert!(
        e.elapsed >= Duration::from_millis(200) && e.elapsed < Duration::from_secs(10),
        "{e:?}"
    );
}

/// §4 "Sampling", §6 "Caps": a sampled size over the cap kills with reason
/// memory; a sampler that knows nothing (0) never does.
#[test]
fn s4_rss_cap_on_sampled_values() {
    let limits = Limits { rss_cap: Some(1 << 20), ..Limits::default() };
    let mut c = Child::spawn(&sh("sleep 30"), Io::null(), &limits).unwrap();
    let e = c.wait_supervised(&Fixed(2 << 20), SAMPLE_INTERVAL).unwrap();
    assert_eq!(e.killed, Some(Killed::Memory));
    assert_eq!(e.peak, Sample { rss: 2 << 20, hwm: 2 << 20 });

    // At the cap is not over it; unknown (0) is never over it.
    let mut c = Child::spawn(&sh("sleep 0.3"), Io::null(), &limits).unwrap();
    let e = c.wait_supervised(&Fixed(1 << 20), SAMPLE_INTERVAL).unwrap();
    assert_eq!((e.killed, e.status.code()), (None, Some(0)));
    let mut c = Child::spawn(&sh("sleep 0.3"), Io::null(), &limits).unwrap();
    let e = c.wait_supervised(&Fixed(0), SAMPLE_INTERVAL).unwrap();
    assert_eq!((e.killed, e.status.code()), (None, Some(0)));
    assert_eq!(e.peak, Sample::default());
}

/// §4 "Sampling": `/proc/<pid>/status` of the child itself (after the
/// wrapper's `exec`), in bytes; 0 for a process that is gone.
#[test]
fn s4_proc_sampler_reads_the_child() {
    let limits = Limits { address_space: Some(4 << 30), ..Limits::default() };
    let mut c = Child::spawn(&sh("exec sleep 30"), Io::null(), &limits).unwrap();
    eventually("a sample", || {
        let s = c.sample(&ProcSampler);
        s.rss > 0 && s.hwm >= s.rss
    });
    assert!(c.peak().rss > 0);
    c.kill(Killed::Time);
    c.wait().unwrap();
    assert_eq!(c.sample(&ProcSampler), Sample::default(), "reaped: nothing to sample");
    assert_eq!(ProcSampler.sample(u32::MAX), Sample::default());
    let parsed = Sample::from_proc_status("Name:\tx\nVmHWM:\t  2048 kB\nVmRSS:\t  1024 kB\n");
    assert_eq!(parsed, Sample { rss: 1 << 20, hwm: 2 << 20 });
    assert_eq!(Sample::from_proc_status("VmRSS: lots\n"), Sample::default());
}

/// §4 "Address-space cap": the wrapper applies the cap in KiB, turns core
/// dumps off, and keeps arguments (quoting included), working directory
/// and environment. Without a cap the command runs as is.
#[test]
fn s4_address_space_cap_wraps_transparently() {
    let dir = scratch("wrap");
    let mut c = sh("printf '%s|' \"$0\" \"$1\" \"$V\" \"$(pwd)\" \"$(ulimit -c)\"; ulimit -v");
    c.args(["a b", "it's"]).current_dir(&*dir).env("V", "v=1");
    let capped = Limits { address_space: Some(300 << 20), ..Limits::default() };
    let (out, status) = output(&c, &capped);
    assert!(status.success());
    let canon = std::fs::canonicalize(&*dir).unwrap();
    assert_eq!(out, format!("a b|it's|v=1|{}|0|307200\n", canon.display()));
    let (out, _) = output(&c, &Limits::default());
    assert!(out.ends_with("|unlimited\n"), "{out}");
    // Rounded down to whole KiB.
    let odd = Limits { address_space: Some((300 << 20) + 1023), ..Limits::default() };
    assert!(output(&c, &odd).0.ends_with("|307200\n"));
}

/// §4 "Address-space cap": a program that needs more address space than the
/// cap cannot run under it, and runs without it.
#[test]
fn s4_address_space_cap_stops_a_hog() {
    if Command::new("python3").arg("--version").output().map_or(true, |o| !o.status.success()) {
        eprintln!("python3 not installed; skipping the hog test");
        return;
    }
    let mut hog = Command::new("python3");
    hog.args(["-c", "b = bytearray(1 << 30); print(len(b))"]);
    let small = Limits { address_space: Some(256 << 20), ..Limits::default() };
    let (_, status) = output(&hog, &small);
    assert!(!status.success(), "{status:?}");
    let large = Limits { address_space: Some(8 << 30), ..Limits::default() };
    let (out, status) = output(&hog, &large);
    assert!(status.success(), "{status:?}");
    assert_eq!(out, "1073741824\n");
}

/// A cap of 0 bytes is an error, not a child that cannot start.
#[test]
fn s4_zero_cap_is_rejected() {
    let zero = Limits { address_space: Some(0), ..Limits::default() };
    let e = Child::spawn(&sh("true"), Io::null(), &zero).unwrap_err();
    assert_eq!(e.kind(), std::io::ErrorKind::InvalidInput);
}

/// §4 "Address-space cap": without `sh` the child starts directly (no
/// cap), keeping its streams.
#[test]
fn s4_without_sh_starts_directly() {
    let dir = scratch("no-sh");
    let mut c = Command::new("/bin/sh");
    c.args(["-c", "ulimit -v"]).env("PATH", &*dir);
    let capped = Limits { address_space: Some(300 << 20), ..Limits::default() };
    let (out, status) = output(&c, &capped);
    assert!(status.success());
    assert_eq!(out, "unlimited\n");
}

/// §4 "Table": a child is in the table from spawn to reap, and dropping an
/// unreaped child kills and reaps it.
#[test]
fn s4_table_from_spawn_to_reap() {
    let mut c = Child::spawn(&sh("sleep 30"), Io::null(), &Limits::default()).unwrap();
    let pid = c.id();
    assert!(process::live_children().contains(&pid));
    c.kill(Killed::Time);
    // Killed but not reaped: still in the table.
    std::thread::sleep(Duration::from_millis(50));
    assert!(process::live_children().contains(&pid));
    c.wait().unwrap();
    assert!(!process::live_children().contains(&pid));

    let marker = format!("slip-drop-{}", std::process::id());
    let c = Child::spawn(&sh(&format!("sleep 30; : {marker}")), Io::null(), &Limits::default())
        .unwrap();
    let pid = c.id();
    eventually("the child to run", || alive(pid, &marker));
    drop(c);
    assert!(!process::live_children().contains(&pid));
    // Reaped, not a zombie (the pid may already be reused, hence the marker).
    assert!(!alive(pid, &marker), "dropped child not killed");
    assert!(!our_zombie(pid), "dropped child not reaped");
    assert!(!process::shutting_down());
}

/// A spawn that fails is an error of its own kind.
#[test]
fn s4_failed_spawn_is_an_error() {
    let e = Child::spawn(&Command::new("/nonexistent/slip-x"), Io::null(), &Limits::default());
    assert_eq!(e.unwrap_err().kind(), std::io::ErrorKind::NotFound);
}
