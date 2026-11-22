//! Child processes: groups, caps, sampling, the table (`spec/exec.md` §4).
//!
//! Ported from Kiln's `xtask/src/util/process.rs` (`kiln/spec/harness.md`
//! "Resource limits" and "Jobserver"); both projects are MIT OR Apache-2.0
//! with the same owner. Kept: every child leads its own process group and
//! is killed by group with `SIGKILL` (through the `kill` executable, as
//! `forbid(unsafe_code)` allows no `kill(2)`); the address-space cap through
//! a `sh` wrapper that `exec`s the program (so the pid is the program's);
//! the process-wide table of live children with spawns in flight counted,
//! the shutdown that kills and waits for them, and later spawns failing
//! with `Interrupted`. Changed: a [`Child`] is a supervised handle (the
//! caller polls it, so a scheduler can supervise many), the table can reap
//! a child itself during a shutdown, a kill records its reason first, and
//! resident size is sampled from `/proc` through [`Sampler`]. Left out:
//! output capture, GNU time measurement, the cargo-step `SIGTERM` grace,
//! the allocation-failure signatures and the stack-reservation helpers,
//! which are specific to Kiln's test runner.

use std::io;
use std::process::{ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use crate::jobserver::say;

/// The default sampling interval of §4.
pub const SAMPLE_INTERVAL: Duration = Duration::from_millis(50);

/// The exit status of the address-space wrapper when `ulimit -v` itself
/// fails (a hard limit lower than the requested cap).
pub const ULIMIT_FAILED: i32 = 125;

/// The `$0` of the address-space wrapper, which prefixes the shell's own
/// messages (`slip-mem-limit: exec: …: Cannot allocate memory`).
pub const WRAPPER_NAME: &str = "slip-mem-limit";

/// The limits a child is supervised under. `None` disables each.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Limits {
    /// Address-space cap (`RLIMIT_AS`) in bytes, applied by the `sh`
    /// wrapper; rounded down to whole KiB. `Some(0)` is rejected.
    pub address_space: Option<u64>,
    /// Wall-clock limit from spawn, enforced by [`Child::check`].
    pub timeout: Option<Duration>,
    /// Resident-size cap in bytes, enforced by [`Child::check`] on sampled
    /// values (never enforced when the sampler reports 0, "unknown").
    pub rss_cap: Option<u64>,
}

/// Why the supervisor killed a child, recorded before the signal is sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Killed {
    /// Its sampled resident size exceeded a cap (§6).
    Memory,
    /// It exceeded its time limit.
    Time,
    /// The coordinator was interrupted by a signal (§7).
    Signal,
}

/// One resident-size measurement, in bytes. 0 is "unknown".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Sample {
    /// Current resident size (`VmRSS`).
    pub rss: u64,
    /// Peak resident size (`VmHWM`).
    pub hwm: u64,
}

impl Sample {
    /// Parses the `VmRSS` and `VmHWM` lines of `/proc/<pid>/status` (in kB);
    /// a missing or malformed line is 0.
    pub fn from_proc_status(text: &str) -> Self {
        let field = |name: &str| -> u64 {
            text.lines()
                .find_map(|l| l.strip_prefix(name)?.strip_prefix(':'))
                .and_then(|v| v.trim().strip_suffix("kB")?.trim().parse::<u64>().ok())
                .map_or(0, |kb| kb.saturating_mul(1024))
        };
        Self { rss: field("VmRSS"), hwm: field("VmHWM") }
    }

    /// The larger of each field.
    #[must_use]
    pub fn max(self, other: Self) -> Self {
        Self { rss: self.rss.max(other.rss), hwm: self.hwm.max(other.hwm) }
    }
}

/// How a child's resident size is measured. The real one is
/// [`ProcSampler`]; tests drive supervision with a fake.
pub trait Sampler: Send + Sync {
    fn sample(&self, pid: u32) -> Sample;
}

/// Reads `/proc/<pid>/status`; every measurement is 0 where that cannot be
/// read (no `/proc`, the process is gone).
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcSampler;

impl Sampler for ProcSampler {
    fn sample(&self, pid: u32) -> Sample {
        std::fs::read_to_string(format!("/proc/{pid}/status"))
            .map_or_else(|_| Sample::default(), |t| Sample::from_proc_status(&t))
    }
}

/// The standard streams of a child.
#[derive(Debug)]
pub struct Io {
    pub stdin: Stdio,
    pub stdout: Stdio,
    pub stderr: Stdio,
}

impl Io {
    /// All three null.
    pub fn null() -> Self {
        Self { stdin: Stdio::null(), stdout: Stdio::null(), stderr: Stdio::null() }
    }

    /// All three piped.
    pub fn piped() -> Self {
        Self { stdin: Stdio::piped(), stdout: Stdio::piped(), stderr: Stdio::piped() }
    }
}

/// How a supervised child ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exit {
    pub status: ExitStatus,
    /// Why the supervisor killed it, if it did.
    pub killed: Option<Killed>,
    /// The largest values sampled while it ran.
    pub peak: Sample,
    /// From spawn to reap (as observed by the polling).
    pub elapsed: Duration,
}

/// A supervised child process: the leader of its own process group, in the
/// process-wide table from spawn to reap. Dropping an unreaped child kills
/// its group and reaps it.
#[derive(Debug)]
pub struct Child {
    entry: Arc<Entry>,
    table: &'static Table,
    limits: Limits,
    start: Instant,
    peak: Sample,
    stdin: Option<ChildStdin>,
    stdout: Option<ChildStdout>,
    stderr: Option<ChildStderr>,
}

#[derive(Debug)]
struct Entry {
    pid: u32,
    state: Mutex<EntryState>,
}

#[derive(Debug)]
struct EntryState {
    child: std::process::Child,
    status: Option<ExitStatus>,
    killed: Option<Killed>,
}

impl Entry {
    fn lock(&self) -> MutexGuard<'_, EntryState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Records `reason` (the first one wins) and kills the group, unless the
    /// child is already reaped (then its pid may be reused: no signal).
    /// Under the entry's lock, so it cannot be reaped in between.
    fn kill(&self, reason: Option<Killed>) {
        let mut st = self.lock();
        if st.status.is_some() {
            return;
        }
        if st.killed.is_none() {
            st.killed = reason;
        }
        kill_group(self.pid);
        let _ = st.child.kill();
    }

    /// Reaps the child if it has exited; `Ok(true)` if it was reaped by this
    /// call.
    fn reap(&self) -> io::Result<(Option<ExitStatus>, bool)> {
        let mut st = self.lock();
        if let Some(s) = st.status {
            return Ok((Some(s), false));
        }
        let s = st.child.try_wait()?;
        st.status = s;
        Ok((s, s.is_some()))
    }
}

/// Warned that `sh` is missing (§4: once).
static NO_SHELL_WARNED: AtomicBool = AtomicBool::new(false);

impl Child {
    /// Starts `cmd` (its program, arguments, working directory and explicit
    /// environment; stdio from `io`) as the leader of a new process group,
    /// under `limits.address_space` through the `sh` wrapper of §4 when
    /// given, and registers it in the table. Fails with `Interrupted` once
    /// [`kill_live_children`] has run.
    pub fn spawn(cmd: &Command, io: Io, limits: &Limits) -> io::Result<Self> {
        Self::spawn_in(&TABLE, cmd, io, limits)
    }

    fn spawn_in(table: &'static Table, cmd: &Command, io: Io, limits: &Limits) -> io::Result<Self> {
        if limits.address_space == Some(0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "an address-space cap of 0 bytes",
            ));
        }
        let Io { stdin, stdout, stderr } = io;
        let spawning = table.begin_spawn()?;
        let start = Instant::now();
        let mut final_cmd = match limits.address_space {
            Some(bytes) if shell_available(cmd) => wrap(cmd, (bytes / 1024).max(1)),
            Some(_) => {
                if !NO_SHELL_WARNED.swap(true, Ordering::SeqCst) {
                    say(format_args!(
                        "slip: warning: `sh` not found; children run without an address-space cap"
                    ));
                }
                copy_command(cmd, Command::new(cmd.get_program()))
            }
            None => copy_command(cmd, Command::new(cmd.get_program())),
        };
        let child = configure(&mut final_cmd).stdin(stdin).stdout(stdout).stderr(stderr).spawn();
        let mut child = match child {
            Ok(c) => c,
            Err(e) => {
                spawning.failed();
                return Err(e);
            }
        };
        let (stdin, stdout, stderr) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take());
        let entry = Arc::new(Entry {
            pid: child.id(),
            state: Mutex::new(EntryState { child, status: None, killed: None }),
        });
        spawning.spawned(&entry);
        Ok(Self {
            entry,
            table,
            limits: *limits,
            start,
            peak: Sample::default(),
            stdin,
            stdout,
            stderr,
        })
    }

    /// The child's pid, which is also its process group id.
    pub fn id(&self) -> u32 {
        self.entry.pid
    }

    /// The limits it runs under.
    pub fn limits(&self) -> &Limits {
        &self.limits
    }

    pub fn take_stdin(&mut self) -> Option<ChildStdin> {
        self.stdin.take()
    }

    pub fn take_stdout(&mut self) -> Option<ChildStdout> {
        self.stdout.take()
    }

    pub fn take_stderr(&mut self) -> Option<ChildStderr> {
        self.stderr.take()
    }

    /// Time since spawn.
    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }

    /// Records `reason` (unless one is recorded already) and sends `SIGKILL`
    /// to the process group. Nothing happens to a child already reaped.
    pub fn kill(&self, reason: Killed) {
        self.entry.kill(Some(reason));
    }

    /// Why the supervisor killed it, if it did.
    pub fn killed(&self) -> Option<Killed> {
        self.entry.lock().killed
    }

    /// Reaps the child if it has exited, without blocking.
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        let (status, newly) = self.entry.reap()?;
        if newly {
            self.table.remove(self.entry.pid);
        }
        Ok(status)
    }

    /// Waits for the child to exit and reaps it (by polling, so a shutdown
    /// can still kill it meanwhile).
    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        let mut delay = Duration::from_millis(1);
        loop {
            if let Some(s) = self.try_wait()? {
                return Ok(s);
            }
            thread::sleep(delay);
            delay = (delay * 2).min(Duration::from_millis(25));
        }
    }

    /// Samples the child's resident size (0 once it is reaped) and records
    /// the peak.
    pub fn sample(&mut self, sampler: &dyn Sampler) -> Sample {
        if self.entry.lock().status.is_some() {
            return Sample::default();
        }
        let s = sampler.sample(self.entry.pid);
        self.peak = self.peak.max(s);
        s
    }

    /// The largest values sampled so far.
    pub fn peak(&self) -> Sample {
        self.peak
    }

    /// One supervision step: samples, and kills the child if its resident
    /// size exceeds `limits.rss_cap` ([`Killed::Memory`]) or it has run past
    /// `limits.timeout` ([`Killed::Time`]). Returns the reason if this call
    /// killed it.
    pub fn check(&mut self, sampler: &dyn Sampler) -> Option<Killed> {
        let s = self.sample(sampler);
        let reason = if self.limits.rss_cap.is_some_and(|cap| s.rss > cap) {
            Killed::Memory
        } else if self.limits.timeout.is_some_and(|t| self.elapsed() >= t) {
            Killed::Time
        } else {
            return None;
        };
        if self.killed().is_some() || self.entry.lock().status.is_some() {
            return None;
        }
        self.kill(reason);
        Some(reason)
    }

    /// Runs [`Child::check`] every `interval` until the child exits, then
    /// reaps it.
    pub fn wait_supervised(
        &mut self,
        sampler: &dyn Sampler,
        interval: Duration,
    ) -> io::Result<Exit> {
        loop {
            if let Some(status) = self.try_wait()? {
                return Ok(self.exit(status));
            }
            self.check(sampler);
            // Short sleeps, so an exit is noticed quickly.
            let until = Instant::now() + interval;
            while Instant::now() < until {
                if let Some(status) = self.try_wait()? {
                    return Ok(self.exit(status));
                }
                thread::sleep(interval.min(Duration::from_millis(5)));
            }
        }
    }

    fn exit(&self, status: ExitStatus) -> Exit {
        Exit { status, killed: self.killed(), peak: self.peak, elapsed: self.elapsed() }
    }
}

impl Drop for Child {
    fn drop(&mut self) {
        if self.entry.lock().status.is_some() {
            return;
        }
        self.entry.kill(None);
        let _ = self.wait();
    }
}

/// `cmd`'s arguments, working directory and explicit environment copied
/// onto `to`.
fn copy_command(from: &Command, mut to: Command) -> Command {
    to.args(from.get_args());
    if let Some(d) = from.get_current_dir() {
        to.current_dir(d);
    }
    for (k, v) in from.get_envs() {
        match v {
            Some(v) => to.env(k, v),
            None => to.env_remove(k),
        };
    }
    to
}

/// `cmd` under an address-space cap of `kib` KiB, with core dumps off:
/// `sh -c 'ulimit -v <KiB> || exit 125; ulimit -c 0 2>/dev/null; exec "$@"'
/// slip-mem-limit <program> <args…>`. `exec` keeps the pid, so the process
/// group, the kill and the sampling all reach the program itself.
fn wrap(cmd: &Command, kib: u64) -> Command {
    let mut c = Command::new("sh");
    c.arg("-c")
        .arg(format!(
            "ulimit -v {kib} || exit {ULIMIT_FAILED}; ulimit -c 0 2>/dev/null; exec \"$@\""
        ))
        .arg(WRAPPER_NAME)
        .arg(cmd.get_program());
    copy_command(cmd, c)
}

/// Whether `sh` would be found for `cmd`: on its own `PATH` if it sets one
/// (which is what the spawn searches), else on this process's.
fn shell_available(cmd: &Command) -> bool {
    let explicit =
        cmd.get_envs().find(|(k, _)| *k == "PATH").map(|(_, v)| v.map(ToOwned::to_owned));
    let path = match explicit {
        Some(v) => v,
        None => std::env::var_os("PATH"),
    };
    path.is_some_and(|p| std::env::split_paths(&p).any(|d| d.join("sh").is_file()))
}

/// Its own process group (pgid = pid).
fn configure(cmd: &mut Command) -> &mut Command {
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(cmd, 0);
    cmd
}

/// `SIGKILL` to the process group led by `pid`. The caller guarantees the
/// leader is not reaped yet, so the id cannot have been reused. Without a
/// `kill` executable the shell's builtin does it.
#[cfg(unix)]
fn kill_group(pid: u32) {
    let quiet = |c: &mut Command| {
        c.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status()
    };
    if quiet(Command::new("kill").args(["-KILL", "--", &format!("-{pid}")])).is_err() {
        let _ = quiet(Command::new("sh").args(["-c", &format!("kill -KILL -{pid}")]));
    }
}

#[cfg(not(unix))]
fn kill_group(_pid: u32) {}

// ----- the table ------------------------------------------------------------

/// The live children of this process: entries between spawn and reap, the
/// spawns in flight, and whether a shutdown has run.
#[derive(Debug)]
struct Table {
    state: Mutex<TableState>,
    changed: Condvar,
}

#[derive(Debug, Default)]
struct TableState {
    live: Vec<Arc<Entry>>,
    spawning: usize,
    shutdown: bool,
}

static TABLE: Table = Table::new();

/// A spawn in flight, counted so that a shutdown waits for (and kills) a
/// child whose pid is not known yet. Ended by `spawned` or `failed`.
#[must_use]
#[derive(Debug)]
struct Spawning<'a>(&'a Table);

impl Table {
    const fn new() -> Self {
        Self {
            state: Mutex::new(TableState { live: Vec::new(), spawning: 0, shutdown: false }),
            changed: Condvar::new(),
        }
    }

    /// A list and two counters updated under the lock: a poisoned lock
    /// still holds a consistent value.
    fn lock(&self) -> MutexGuard<'_, TableState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn begin_spawn(&self) -> io::Result<Spawning<'_>> {
        let mut st = self.lock();
        if st.shutdown {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "slip is shutting down"));
        }
        st.spawning += 1;
        Ok(Spawning(self))
    }

    fn remove(&self, pid: u32) {
        let mut st = self.lock();
        st.live.retain(|e| e.pid != pid);
        drop(st);
        self.changed.notify_all();
    }

    fn pids(&self) -> Vec<u32> {
        self.lock().live.iter().map(|e| e.pid).collect()
    }

    /// Sets the shutdown flag, kills the group of every live child
    /// (recording [`Killed::Signal`]), and reaps them, waiting at most
    /// `grace` until none is live and no spawn is in flight; a child spawned
    /// meanwhile is killed as it registers. Returns how many were killed.
    fn shutdown(&self, grace: Duration) -> usize {
        let deadline = Instant::now() + grace;
        let entries: Vec<Arc<Entry>> = {
            let mut st = self.lock();
            st.shutdown = true;
            st.live.clone()
        };
        for e in &entries {
            e.kill(Some(Killed::Signal));
        }
        loop {
            // Reap here, so nothing depends on the owners still polling.
            let live: Vec<Arc<Entry>> = self.lock().live.clone();
            for e in &live {
                if let Ok((_, true)) = e.reap() {
                    self.remove(e.pid);
                }
            }
            let st = self.lock();
            if st.live.is_empty() && st.spawning == 0 {
                break;
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            let _ = self
                .changed
                .wait_timeout(st, left.min(Duration::from_millis(10)))
                .unwrap_or_else(PoisonError::into_inner);
        }
        entries.len()
    }

    /// `SIGKILL` to every live group without waiting, and every later spawn
    /// fails. Skipped if the table is locked (by a cleanup stuck holding
    /// it); a child whose entry is locked is skipped likewise.
    fn kill_now(&self) {
        let entries = match self.state.try_lock() {
            Ok(mut st) => {
                st.shutdown = true;
                st.live.clone()
            }
            Err(std::sync::TryLockError::Poisoned(p)) => {
                let mut st = p.into_inner();
                st.shutdown = true;
                st.live.clone()
            }
            Err(std::sync::TryLockError::WouldBlock) => return,
        };
        for e in &entries {
            let mut st = match e.state.try_lock() {
                Ok(st) => st,
                Err(std::sync::TryLockError::Poisoned(p)) => p.into_inner(),
                Err(std::sync::TryLockError::WouldBlock) => continue,
            };
            if st.status.is_none() {
                st.killed.get_or_insert(Killed::Signal);
                kill_group(e.pid);
                let _ = st.child.kill();
            }
        }
    }
}

impl Spawning<'_> {
    /// Registers the spawned child, killing it at once if a shutdown started
    /// meanwhile.
    fn spawned(self, entry: &Arc<Entry>) {
        let table = self.0;
        let mut st = table.lock();
        st.spawning -= 1;
        st.live.push(Arc::clone(entry));
        let late = st.shutdown;
        drop(st);
        if late {
            entry.kill(Some(Killed::Signal));
        }
        table.changed.notify_all();
    }

    fn failed(self) {
        let table = self.0;
        table.lock().spawning -= 1;
        table.changed.notify_all();
    }
}

/// The pids of the live children (spawned and not yet reaped).
pub fn live_children() -> Vec<u32> {
    TABLE.pids()
}

/// Whether [`kill_live_children`] or [`kill_live_children_now`] has run (new
/// spawns fail).
pub fn shutting_down() -> bool {
    TABLE.lock().shutdown
}

/// Kills (`SIGKILL`, by process group, reason [`Killed::Signal`]) every live
/// child, reaps them, waiting at most `grace`, and makes every later spawn
/// fail with `Interrupted`. Called by the signal path (§7) before the tokens
/// are returned. Returns how many children were killed.
pub fn kill_live_children(grace: Duration) -> usize {
    TABLE.shutdown(grace)
}

/// `SIGKILL` to every live child's group without waiting for anything, and
/// every later spawn fails: a second signal, or the time bound of §7.
pub fn kill_live_children_now() {
    TABLE.kill_now();
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt as _;

    fn sleeper() -> Command {
        let mut c = Command::new("sh");
        c.args(["-c", "sleep 30; :"]);
        c
    }

    /// §4 "Table", §7: the shutdown kills a registered child's whole group
    /// and reaps it although nobody else polls; a spawn in flight when it
    /// starts is killed as it registers; later spawns fail. (A private
    /// table: the global one is shared with the other tests.)
    #[test]
    fn shutdown_kills_and_reaps_live_children() {
        static T: Table = Table::new();
        let mut first = Child::spawn_in(&T, &sleeper(), Io::null(), &Limits::default()).unwrap();
        let late = T.begin_spawn().unwrap();
        let late = thread::spawn(move || {
            thread::sleep(Duration::from_millis(300));
            let c = sleeper().stdin(Stdio::null()).process_group_spawn();
            let entry = Arc::new(Entry {
                pid: c.id(),
                state: Mutex::new(EntryState { child: c, status: None, killed: None }),
            });
            late.spawned(&entry);
            entry
        });
        let t = Instant::now();
        assert_eq!(T.shutdown(Duration::from_secs(20)), 1);
        assert!(t.elapsed() < Duration::from_secs(10), "{:?}", t.elapsed());
        assert!(T.pids().is_empty());
        let late = late.join().unwrap();
        assert_eq!(late.lock().status.and_then(|s| s.signal()), Some(9));
        assert_eq!(late.lock().killed, Some(Killed::Signal));
        let s = first.try_wait().unwrap().unwrap();
        assert_eq!(s.signal(), Some(9));
        assert_eq!(first.killed(), Some(Killed::Signal));
        let refused = Child::spawn_in(&T, &sleeper(), Io::null(), &Limits::default())
            .map(|_| ())
            .unwrap_err();
        assert_eq!(refused.kind(), io::ErrorKind::Interrupted);
    }

    /// §7: the immediate kill reaches every group and refuses later spawns.
    #[test]
    fn kill_now_kills_without_waiting() {
        static T: Table = Table::new();
        let mut a = Child::spawn_in(&T, &sleeper(), Io::null(), &Limits::default()).unwrap();
        T.kill_now();
        assert_eq!(a.wait().unwrap().signal(), Some(9));
        assert_eq!(a.killed(), Some(Killed::Signal));
        assert!(Child::spawn_in(&T, &sleeper(), Io::null(), &Limits::default()).is_err());
    }

    trait SpawnGroup {
        fn process_group_spawn(&mut self) -> std::process::Child;
    }
    impl SpawnGroup for Command {
        fn process_group_spawn(&mut self) -> std::process::Child {
            configure(self).spawn().unwrap()
        }
    }
}
