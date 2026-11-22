//! GNU make jobserver client, fifo style (`spec/exec.md` §3).
//!
//! Ported from Kiln's `xtask/src/jobs.rs` (`kiln/spec/harness.md`
//! "Jobserver"); both projects are MIT OR Apache-2.0 with the same owner.
//! Kept: the `MAKEFLAGS` parsing, the implicit token, one helper thread that
//! does every blocking `read`, release by `Drop`, [`Pool::release_all`] for
//! the signal path (every byte returned exactly once), and the non-blocking
//! `status`/`refill`. Changed: a waiter is a queue entry served by a
//! delivery callback (so the scheduler can receive its token over its own
//! channel and abandon the request, §3 last paragraph), [`Pool::try_acquire`]
//! reads without blocking, pools register themselves for [`release_all`],
//! and the names use the `SLIP_` prefix. Left out: Kiln's `cargo xtask
//! jobserver` command line (Slip's is a later work package) and the signal
//! thread, which is [`crate::signals`].
//!
//! A [`Pool`] is a connection to the fifo named by `MAKEFLAGS`, or the "no
//! jobserver" stand-in, in which every acquisition is free and `-j` alone
//! bounds the parallelism. Like make, the process owns one *implicit* token
//! that is not in the pool: one job may run without reading the fifo; every
//! further concurrent job holds one byte read from the fifo, which its
//! [`Token`] writes back when dropped.

use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::{self, ErrorKind, Read as _, Write as _};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, Weak};
use std::time::{Duration, Instant};

/// The fifo `cargo xtask jobserver` and [`default_fifo`] fall back to.
pub const DEFAULT_FIFO: &str = "/run/slip/jobserver.fifo";

/// The pool size of `scripts/jobserver.sh` when `SLIP_JOBS` is unset.
pub const DEFAULT_POOL_SIZE: usize = 8;

/// How long [`Pool::release_all`] waits for a helper `read` in flight to put
/// back the byte it may have taken.
const HELPER_SETTLE: Duration = Duration::from_millis(200);

/// How often an idle helper checks whether its pool is still in use.
const HELPER_IDLE_CHECK: Duration = Duration::from_secs(1);

/// The fifo of the last `--jobserver-auth=` word of `MAKEFLAGS` when it is
/// the `fifo:<path>` form (make passes the last one as authoritative). The
/// descriptor forms (`--jobserver-auth=R,W`, `--jobserver-fds=R,W`) need
/// inherited descriptors and give `None`.
pub fn fifo_from_makeflags(flags: &str) -> Option<PathBuf> {
    flags
        .split_ascii_whitespace()
        .filter_map(|w| w.strip_prefix("--jobserver-auth="))
        .next_back()?
        .strip_prefix("fifo:")
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
}

/// The fifo an inspection command addresses: `MAKEFLAGS`' `fifo:` path,
/// else `$SLIP_JOBSERVER_FIFO`, else [`DEFAULT_FIFO`].
pub fn default_fifo() -> PathBuf {
    resolve_fifo(
        std::env::var("MAKEFLAGS").ok().as_deref(),
        std::env::var_os("SLIP_JOBSERVER_FIFO").map(PathBuf::from),
    )
}

/// [`default_fifo`] from explicit values (`MAKEFLAGS`, `SLIP_JOBSERVER_FIFO`).
pub fn resolve_fifo(makeflags: Option<&str>, fifo_var: Option<PathBuf>) -> PathBuf {
    makeflags
        .and_then(fifo_from_makeflags)
        .or(fifo_var.filter(|p| !p.as_os_str().is_empty()))
        .unwrap_or_else(|| PathBuf::from(DEFAULT_FIFO))
}

/// The pool size an inspection command assumes: `$SLIP_JOBS`, else
/// [`DEFAULT_POOL_SIZE`]. A value that is not a number is an error.
pub fn default_pool_size() -> Result<usize, String> {
    resolve_pool_size(std::env::var("SLIP_JOBS").ok().as_deref())
}

/// [`default_pool_size`] from an explicit `SLIP_JOBS` value.
pub fn resolve_pool_size(jobs_var: Option<&str>) -> Result<usize, String> {
    match jobs_var {
        None => Ok(DEFAULT_POOL_SIZE),
        Some(v) => v.trim().parse().map_err(|_| format!("bad SLIP_JOBS `{v}`")),
    }
}

/// The number of cores available to this process (at least 1): the default
/// `-j`.
pub fn available_cores() -> usize {
    std::thread::available_parallelism().map_or(1, NonZeroUsize::get)
}

/// One line on stderr that never panics: stderr may be a pipe whose reader
/// is gone (EPIPE), where `eprintln!` panics.
pub(crate) fn say(args: std::fmt::Arguments<'_>) {
    let _ = writeln!(io::stderr(), "{args}");
}

/// A jobserver connection (or the "no jobserver" stand-in) and the `-j`
/// bound. Cloning shares the connection.
#[derive(Debug, Clone)]
pub struct Pool {
    shared: Option<Arc<Shared>>,
    jobs: usize,
}

struct Shared {
    path: PathBuf,
    /// Opened read+write, so neither end ever sees EOF or EPIPE.
    fifo: File,
    /// A second descriptor with `O_NONBLOCK`, for [`Pool::try_acquire`]
    /// (absent where the flag's value is not known).
    nonblocking: Option<File>,
    state: Mutex<State>,
    /// The helper waiting for a request.
    helper: Condvar,
    /// `reading` changed ([`Pool::release_all`] waits on it).
    settled: Condvar,
}

impl std::fmt::Debug for Shared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Shared").field("path", &self.path).finish_non_exhaustive()
    }
}

type Deliver = Box<dyn FnOnce(Token) + Send>;

// Independent flags of one mutex-guarded state machine.
#[allow(clippy::struct_excessive_bools)]
struct State {
    implicit_free: bool,
    /// Requests not yet served, oldest first.
    pending: VecDeque<(u64, Deliver)>,
    next_id: u64,
    helper_started: bool,
    /// The fifo failed: acquisition is free from now on.
    broken: bool,
    /// The fallback warning was printed.
    warned: bool,
    /// Pool bytes held by live tokens, including tokens delivered but not
    /// yet received (returned by [`Pool::release_all`]).
    held: Vec<u8>,
    /// [`Pool::release_all`] ran: tokens dropped later return nothing and
    /// new ones are free (the process is exiting).
    released: bool,
    /// The helper is in (or just back from) a `read`, so it may hold a byte
    /// nobody has accounted for yet.
    reading: bool,
}

/// A held parallelism slot; dropping it releases it, on every path.
#[derive(Debug)]
pub struct Token {
    kind: Kind,
    shared: Option<Arc<Shared>>,
}

/// What a [`Token`] stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// No jobserver, a broken one, or one already released: nothing to give
    /// back.
    Free,
    /// The process's own token, not in the pool.
    Implicit,
    /// One byte read from the fifo, written back on release.
    Pool(u8),
}

impl Token {
    /// What this token stands for.
    pub fn kind(&self) -> Kind {
        self.kind
    }
}

/// An outstanding [`Pool::request`]. Dropping it abandons the request: if
/// its token has not been delivered yet, it never will be, and a byte the
/// helper reads for it afterwards goes straight back to the fifo.
#[derive(Debug)]
pub struct Request {
    shared: Option<Arc<Shared>>,
    id: u64,
}

impl Request {
    /// Whether the token has not been delivered yet.
    pub fn is_pending(&self) -> bool {
        let Some(sh) = &self.shared else { return false };
        sh.lock().pending.iter().any(|(id, _)| *id == self.id)
    }
}

impl Drop for Request {
    fn drop(&mut self) {
        let Some(sh) = &self.shared else { return };
        let removed = {
            let mut st = sh.lock();
            st.pending.iter().position(|(id, _)| *id == self.id).and_then(|i| st.pending.remove(i))
        };
        // The callback (and whatever it owns) is dropped outside the lock.
        drop(removed);
    }
}

/// The pools of this process, for [`release_all`] (the signal path).
static POOLS: Mutex<Vec<Weak<Shared>>> = Mutex::new(Vec::new());

impl Pool {
    /// No jobserver: every acquisition is free; `-j` is the number of cores.
    pub fn none() -> Self {
        Self { shared: None, jobs: available_cores() }
    }

    /// The jobserver this process inherited through `MAKEFLAGS`, with the
    /// fallback of §3: without a usable `fifo:` word (none, the descriptor
    /// forms, a path that is not a fifo or cannot be opened) it warns once
    /// when a path was named and returns [`Pool::none`].
    pub fn from_env() -> Self {
        Self::from_makeflags(&std::env::var("MAKEFLAGS").unwrap_or_default())
    }

    /// [`Pool::from_env`] for an explicit `MAKEFLAGS` value.
    pub fn from_makeflags(flags: &str) -> Self {
        let Some(path) = fifo_from_makeflags(flags) else { return Self::none() };
        Self::from_fifo(&path).unwrap_or_else(|e| {
            say(format_args!(
                "slip: warning: cannot use jobserver fifo {} ({e}); falling back to -j",
                path.display()
            ));
            Self::none()
        })
    }

    /// Connects to the fifo at `path`, opened read+write. An error if it is
    /// not a fifo or cannot be opened.
    pub fn from_fifo(path: &Path) -> io::Result<Self> {
        if !is_fifo(path)? {
            return Err(io::Error::other("not a fifo"));
        }
        let fifo = OpenOptions::new().read(true).write(true).open(path)?;
        let nonblocking = open_nonblocking(path).ok();
        let shared = Arc::new(Shared {
            path: path.to_owned(),
            fifo,
            nonblocking,
            state: Mutex::new(State {
                implicit_free: true,
                pending: VecDeque::new(),
                next_id: 0,
                helper_started: false,
                broken: false,
                warned: false,
                held: Vec::new(),
                released: false,
                reading: false,
            }),
            helper: Condvar::new(),
            settled: Condvar::new(),
        });
        let mut pools = POOLS.lock().unwrap_or_else(PoisonError::into_inner);
        pools.retain(|w| w.strong_count() > 0);
        pools.push(Arc::downgrade(&shared));
        drop(pools);
        Ok(Self { shared: Some(shared), jobs: available_cores() })
    }

    /// The same pool with `-j` set to `jobs` (at least 1).
    #[must_use]
    pub fn with_jobs(mut self, jobs: usize) -> Self {
        self.jobs = jobs.max(1);
        self
    }

    /// The `-j` bound: an upper bound on concurrent jobs, which the pool can
    /// only lower. The scheduler enforces it; the pool only carries it.
    pub fn jobs(&self) -> usize {
        self.jobs
    }

    /// `fifo:<path>`, or `None` without a jobserver.
    pub fn describe(&self) -> Option<String> {
        self.shared.as_ref().map(|s| format!("fifo:{}", s.path.display()))
    }

    /// Whether acquisition is free: no jobserver, a fifo that failed, or a
    /// pool already released by [`Pool::release_all`].
    pub fn is_free(&self) -> bool {
        self.shared.as_ref().is_none_or(|s| {
            let st = s.lock();
            st.broken || st.released
        })
    }

    /// Blocks until a slot is available: the implicit token if it is free,
    /// else one byte from the pool. Without a jobserver it returns at once.
    pub fn acquire(&self) -> Token {
        if let Some(t) = self.fast_path() {
            return t;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let request = self.request(move |t| {
            let _ = tx.send(t);
        });
        let token = rx.recv();
        drop(request);
        token.unwrap_or_else(|_| {
            // The delivery callback is only dropped undelivered by the
            // request's own `Drop`, which has not run yet.
            unreachable!("jobserver: a request was dropped without delivery")
        })
    }

    /// A slot if one is available now, without blocking: the implicit token,
    /// a free token when acquisition is free, or a byte the fifo holds at
    /// this moment (read without blocking). `None` otherwise.
    pub fn try_acquire(&self) -> Option<Token> {
        if let Some(t) = self.fast_path() {
            return Some(t);
        }
        let sh = self.shared.as_ref()?;
        let nb = sh.nonblocking.as_ref()?;
        // Under the lock, so `release_all` cannot run between the read and
        // the byte being accounted for in `held`.
        let mut st = sh.lock();
        if st.broken || st.released {
            return Some(Token { kind: Kind::Free, shared: Some(Arc::clone(sh)) });
        }
        let mut byte = [0u8; 1];
        loop {
            return match (&*nb).read(&mut byte) {
                Ok(1) => {
                    st.held.push(byte[0]);
                    Some(Token { kind: Kind::Pool(byte[0]), shared: Some(Arc::clone(sh)) })
                }
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                // Opened read+write, so 0 is not EOF: nothing there.
                Ok(_) => None,
                Err(e) if e.kind() == ErrorKind::WouldBlock => None,
                Err(e) => {
                    let pending = sh.fail(&mut st, &e);
                    drop(st);
                    deliver_free(sh, pending);
                    Some(Token { kind: Kind::Free, shared: Some(Arc::clone(sh)) })
                }
            };
        }
    }

    /// Asks for a token and returns at once. The token is handed to
    /// `deliver` exactly once, on whichever thread makes it available (the
    /// caller's if one is available now, the helper thread after a read, or
    /// the thread dropping another token), so `deliver` must not block: the
    /// intended use is a send on the scheduler's own channel. Dropping the
    /// returned [`Request`] before delivery abandons it. Requests are served
    /// in order.
    pub fn request(&self, deliver: impl FnOnce(Token) + Send + 'static) -> Request {
        let Some(sh) = &self.shared else {
            deliver(Token { kind: Kind::Free, shared: None });
            return Request { shared: None, id: 0 };
        };
        let mut st = sh.lock();
        if st.implicit_free || st.broken || st.released {
            let kind = if st.implicit_free {
                st.implicit_free = false;
                Kind::Implicit
            } else {
                Kind::Free
            };
            drop(st);
            deliver(Token { kind, shared: Some(Arc::clone(sh)) });
            return Request { shared: None, id: 0 };
        }
        let id = st.next_id;
        st.next_id += 1;
        st.pending.push_back((id, Box::new(deliver)));
        if !st.helper_started {
            st.helper_started = true;
            let weak = Arc::downgrade(sh);
            let spawned =
                std::thread::Builder::new().name("jobserver".into()).spawn(move || helper(&weak));
            if let Err(e) = spawned {
                let pending = sh.fail(&mut st, &e);
                drop(st);
                deliver_free(sh, pending);
                return Request { shared: Some(Arc::clone(sh)), id };
            }
        }
        sh.helper.notify_one();
        Request { shared: Some(Arc::clone(sh)), id }
    }

    /// Writes every pool byte this process holds (by live tokens, delivered
    /// or not) back to the fifo and returns how many. Afterwards dropping a
    /// token returns nothing and acquiring is free (pending requests get a
    /// free token): it is the last act of an interrupted process (§7).
    /// Idempotent, so each byte goes back exactly once.
    pub fn release_all(&self) -> usize {
        self.shared.as_ref().map_or(0, Shared::release_all)
    }

    fn fast_path(&self) -> Option<Token> {
        let Some(sh) = &self.shared else { return Some(Token { kind: Kind::Free, shared: None }) };
        let mut st = sh.lock();
        let kind = if st.implicit_free {
            st.implicit_free = false;
            Kind::Implicit
        } else if st.broken || st.released {
            Kind::Free
        } else {
            return None;
        };
        Some(Token { kind, shared: Some(Arc::clone(sh)) })
    }
}

/// [`Pool::release_all`] for every pool of this process; returns the bytes
/// written back. The signal path (§7) calls it after the children are dead.
pub fn release_all() -> usize {
    let pools: Vec<Arc<Shared>> = POOLS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .filter_map(Weak::upgrade)
        .collect();
    pools.iter().map(Shared::release_all).sum()
}

impl Shared {
    /// The state is a handful of counters and queues updated atomically
    /// under the lock, so a poisoned lock still holds a consistent value.
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Returns `byte` to the fifo.
    fn give_back(&self, byte: u8) {
        if let Err(e) = (&self.fifo).write_all(&[byte]) {
            say(format_args!(
                "slip: warning: lost a jobserver token: write to {} failed: {e}",
                self.path.display()
            ));
        }
    }

    /// The fifo failed: acquisition becomes free (warned once). Returns the
    /// pending requests, to be served free tokens outside the lock.
    fn fail(&self, st: &mut State, e: &io::Error) -> Vec<Deliver> {
        st.broken = true;
        if !st.warned {
            st.warned = true;
            say(format_args!(
                "slip: warning: jobserver fifo {} failed ({e}); falling back to -j",
                self.path.display()
            ));
        }
        st.pending.drain(..).map(|(_, d)| d).collect()
    }

    fn release_all(self: &Arc<Self>) -> usize {
        let (bytes, pending) = {
            let mut st = self.lock();
            st.released = true;
            let bytes = std::mem::take(&mut st.held);
            let pending: Vec<Deliver> = st.pending.drain(..).map(|(_, d)| d).collect();
            (bytes, pending)
        };
        self.helper.notify_all();
        for &b in &bytes {
            self.give_back(b);
        }
        // The helper may be blocked in `read`, and the bytes just written
        // back wake it: the one it takes goes straight back too, but only if
        // the process lives that long. Wait (briefly: with an empty pool it
        // stays blocked) until it has.
        let deadline = Instant::now() + HELPER_SETTLE;
        let mut st = self.lock();
        while st.reading {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            st = self.settled.wait_timeout(st, left).unwrap_or_else(PoisonError::into_inner).0;
        }
        drop(st);
        deliver_free(self, pending);
        bytes.len()
    }
}

fn deliver_free(sh: &Arc<Shared>, pending: Vec<Deliver>) {
    for d in pending {
        d(Token { kind: Kind::Free, shared: Some(Arc::clone(sh)) });
    }
}

/// Reads one byte per pending request and delivers it to the oldest; a byte
/// nobody waits for any more goes straight back. Ends when the pool is no
/// longer in use, after [`Pool::release_all`], or on a read error.
fn helper(weak: &Weak<Shared>) {
    loop {
        let Some(sh) = weak.upgrade() else { return };
        {
            let mut st = sh.lock();
            loop {
                if st.released || st.broken {
                    return;
                }
                if !st.pending.is_empty() {
                    break;
                }
                // Only this thread's own (temporary) reference left.
                if Arc::strong_count(&sh) == 1 {
                    return;
                }
                st = sh
                    .helper
                    .wait_timeout(st, HELPER_IDLE_CHECK)
                    .unwrap_or_else(PoisonError::into_inner)
                    .0;
            }
            st.reading = true;
        }
        let mut byte = [0u8; 1];
        let r = loop {
            match (&sh.fifo).read(&mut byte) {
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Ok(0) => break Err(io::Error::new(ErrorKind::UnexpectedEof, "end of fifo")),
                Ok(_) => break Ok(byte[0]),
                Err(e) => break Err(e),
            }
        };
        let mut st = sh.lock();
        let served = match r {
            Ok(b) => {
                let next = if st.released { None } else { st.pending.pop_front() };
                if let Some((_, deliver)) = next {
                    st.held.push(b);
                    Some((deliver, b))
                } else {
                    // Under the lock, so `release_all` sees `reading` clear
                    // only once the byte is back.
                    sh.give_back(b);
                    None
                }
            }
            Err(e) => {
                st.reading = false;
                sh.settled.notify_all();
                let pending = sh.fail(&mut st, &e);
                drop(st);
                deliver_free(&sh, pending);
                return;
            }
        };
        st.reading = false;
        sh.settled.notify_all();
        drop(st);
        if let Some((deliver, b)) = served {
            deliver(Token { kind: Kind::Pool(b), shared: Some(Arc::clone(&sh)) });
        }
    }
}

impl Drop for Token {
    fn drop(&mut self) {
        let Some(sh) = &self.shared else { return };
        let mut st = sh.lock();
        match self.kind {
            Kind::Free => {}
            Kind::Implicit => {
                if let Some((_, deliver)) = st.pending.pop_front() {
                    // Hand it to the oldest waiter; a read the helper may
                    // have in flight for it then goes straight back.
                    drop(st);
                    deliver(Token { kind: Kind::Implicit, shared: Some(Arc::clone(sh)) });
                } else {
                    st.implicit_free = true;
                }
            }
            Kind::Pool(b) => {
                if st.released {
                    // `release_all` already wrote this byte back.
                    return;
                }
                if let Some((_, deliver)) = st.pending.pop_front() {
                    // Still held, now by the waiter.
                    drop(st);
                    deliver(Token { kind: Kind::Pool(b), shared: Some(Arc::clone(sh)) });
                } else {
                    if let Some(i) = st.held.iter().position(|&h| h == b) {
                        st.held.swap_remove(i);
                    }
                    drop(st);
                    sh.give_back(b);
                }
            }
        }
    }
}

// ----- inspection (`cargo xtask jobserver status|refill`) ------------------

/// What [`refill`] found and did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefillReport {
    /// Idle tokens found.
    pub idle: usize,
    /// Tokens written on top of the ones put back.
    pub added: usize,
}

/// Counts the idle tokens of the fifo without blocking: reads until the fifo
/// would block and writes the same bytes back.
pub fn status(fifo: &Path) -> io::Result<usize> {
    drain_and_refill(fifo, None).map(|r| r.idle)
}

/// Tops the pool up to `jobs` idle tokens without blocking (a pool above
/// `jobs` is left alone). Only correct while no client holds tokens: tokens
/// in use come back on top of the refill.
pub fn refill(fifo: &Path, jobs: usize) -> io::Result<RefillReport> {
    drain_and_refill(fifo, Some(jobs))
}

fn drain_and_refill(path: &Path, target: Option<usize>) -> io::Result<RefillReport> {
    if !is_fifo(path)? {
        return Err(io::Error::other("not a fifo"));
    }
    let mut f = open_nonblocking(path)?;
    let mut got = Vec::new();
    let mut buf = [0u8; 64];
    loop {
        match f.read(&mut buf) {
            // Opened read+write, so there is always a writer: 0 cannot be
            // EOF, but stop rather than spin.
            Ok(0) => break,
            Ok(n) => got.extend_from_slice(&buf[..n]),
            Err(e) if e.kind() == ErrorKind::WouldBlock => break,
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) => {
                // Put back what was taken before failing.
                let _ = f.write_all(&got);
                return Err(e);
            }
        }
    }
    let idle = got.len();
    let added = target.map_or(0, |t| t.saturating_sub(idle));
    got.resize(idle + added, b'+');
    f.write_all(&got)?;
    Ok(RefillReport { idle, added })
}

#[cfg(unix)]
fn is_fifo(path: &Path) -> io::Result<bool> {
    use std::os::unix::fs::FileTypeExt as _;
    Ok(std::fs::metadata(path)?.file_type().is_fifo())
}

#[cfg(not(unix))]
fn is_fifo(_: &Path) -> io::Result<bool> {
    Ok(false)
}

/// `O_NONBLOCK` for `OpenOptionsExt::custom_flags` (a plain constant; no
/// `libc` dependency for one number).
#[cfg(all(
    any(target_os = "linux", target_os = "android"),
    not(any(
        target_arch = "mips",
        target_arch = "mips64",
        target_arch = "sparc",
        target_arch = "sparc64",
        target_arch = "powerpc",
        target_arch = "powerpc64"
    ))
))]
const O_NONBLOCK: Option<i32> = Some(0o4000);
#[cfg(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "dragonfly"
))]
const O_NONBLOCK: Option<i32> = Some(0x0004);
#[cfg(not(any(
    all(
        any(target_os = "linux", target_os = "android"),
        not(any(
            target_arch = "mips",
            target_arch = "mips64",
            target_arch = "sparc",
            target_arch = "sparc64",
            target_arch = "powerpc",
            target_arch = "powerpc64"
        ))
    ),
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "dragonfly"
)))]
const O_NONBLOCK: Option<i32> = None;

/// Opens the fifo read+write with `O_NONBLOCK`.
fn open_nonblocking(path: &Path) -> io::Result<File> {
    let Some(flag) = O_NONBLOCK else {
        return Err(io::Error::other("non-blocking open is not supported on this platform"));
    };
    let mut opts = OpenOptions::new();
    opts.read(true).write(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::custom_flags(&mut opts, flag);
    #[cfg(not(unix))]
    let _ = flag;
    opts.open(path)
}
