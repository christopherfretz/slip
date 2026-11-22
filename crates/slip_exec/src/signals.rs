//! Signal handling: kill children, return tokens, always exit
//! (`spec/exec.md` §7).
//!
//! Ported from the signal path of Kiln's `xtask/src/jobs.rs`
//! (`release_on_signal`, `exit_interrupted`, `finish`, the panic hook;
//! `kiln/spec/harness.md` "Jobserver", Signals, WP-1436); both projects are
//! MIT OR Apache-2.0 with the same owner. Kept: a `signal-hook` thread (the
//! handler's `unsafe` stays in that dependency) that runs the cleanup on a
//! thread of its own and only watches it; the 15 s bound; a second signal
//! 0.5 s or more after the first ending the process at once; messages that
//! ignore write errors and never block the exit; other threads that find
//! the process interrupted waiting for it to end instead of racing it to
//! `exit`. Changed: the store's part of the cleanup (tmp files, the lock,
//! the run record) is the caller's callback; one line is printed; the time
//! limits are a parameter ([`Timing`]) rather than a test-only variable.
//! Left out: the cargo-step `SIGTERM` grace.
//!
//! On the first `SIGINT` or `SIGTERM` the cleanup thread: kills every live
//! child's group and waits for them to be reaped (at most [`Timing::reap`],
//! [`crate::process::kill_live_children`]); returns every pool token
//! ([`crate::jobserver::release_all`]); runs the caller's callback (a panic
//! in it is contained); prints `slip: signal N: killed K child process(es),
//! returned T jobserver token(s)`. The signal thread then ends the process
//! by the signal's default action.

use std::io::Write as _;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use crate::jobserver::say;

/// The time limits of the signal path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    /// How long the cleanup waits for the killed children to be reaped
    /// before returning the tokens anyway (a child in uninterruptible sleep).
    pub reap: Duration,
    /// How long the whole cleanup may take before the process ends anyway.
    pub bound: Duration,
    /// A signal this soon after the first is the same interruption
    /// delivered twice (`timeout(1)` signals its child and then its group).
    pub duplicate: Duration,
}

impl Default for Timing {
    /// §7: 10 s, 15 s, 0.5 s.
    fn default() -> Self {
        Self {
            reap: Duration::from_secs(10),
            bound: Duration::from_secs(15),
            duplicate: Duration::from_millis(500),
        }
    }
}

/// The names of the threads of the signal path.
pub const SIGNAL_THREAD: &str = "slip-signals";
pub const CLEANUP_THREAD: &str = "slip-cleanup";

/// The signal that interrupted the process (0: none yet).
static INTERRUPTED: AtomicI32 = AtomicI32::new(0);

/// [`install`] has run (successfully or not).
static INSTALLED: AtomicBool = AtomicBool::new(false);

/// The limits in force, for [`exit_interrupted`].
static TIMING: Mutex<Option<Timing>> = Mutex::new(None);

/// The interrupting signal, once `SIGINT` or `SIGTERM` has arrived.
pub fn interrupted() -> Option<i32> {
    match INTERRUPTED.load(Ordering::SeqCst) {
        0 => None,
        s => Some(s),
    }
}

/// [`install_with`] the default [`Timing`] of §7.
pub fn install(on_cleanup: Box<dyn FnOnce() + Send>) -> bool {
    install_with(on_cleanup, Timing::default())
}

/// Starts catching `SIGINT` and `SIGTERM` (§7). `on_cleanup` is the
/// caller's part of the cleanup (removing `tmp/` files and the store lock,
/// writing the run record); it runs after the children are reaped and the
/// tokens returned. Idempotent: only the first call installs anything, and
/// later calls drop their callback and return `false`. Also `false` if the
/// signals cannot be caught (a warning is printed).
pub fn install_with(on_cleanup: Box<dyn FnOnce() + Send>, timing: Timing) -> bool {
    if INSTALLED.swap(true, Ordering::SeqCst) {
        return false;
    }
    *TIMING.lock().unwrap_or_else(PoisonError::into_inner) = Some(timing);
    hold_panics_while_interrupted();
    start(on_cleanup, timing)
}

fn timing() -> Timing {
    TIMING.lock().unwrap_or_else(PoisonError::into_inner).unwrap_or_default()
}

#[cfg(unix)]
fn start(on_cleanup: Box<dyn FnOnce() + Send>, timing: Timing) -> bool {
    use signal_hook::consts::{SIGINT, SIGTERM};
    let mut signals = match signal_hook::iterator::Signals::new([SIGINT, SIGTERM]) {
        Ok(s) => s,
        Err(e) => {
            say(format_args!(
                "slip: warning: cannot catch SIGINT/SIGTERM ({e}); an interrupted run will \
                 neither kill its children nor return its jobserver tokens"
            ));
            return false;
        }
    };
    let spawned = std::thread::Builder::new().name(SIGNAL_THREAD.into()).spawn(move || {
        // `forever` only ends when the handle is closed, which nothing does.
        let Some(sig) = signals.forever().next() else { return };
        INTERRUPTED.store(sig, Ordering::SeqCst);
        let start = Instant::now();
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let cleanup = move || {
            let killed = crate::process::kill_live_children(timing.reap);
            let returned = crate::jobserver::release_all();
            let callback = std::panic::catch_unwind(std::panic::AssertUnwindSafe(on_cleanup));
            say(format_args!(
                "slip: signal {sig}: killed {killed} child process(es), returned {returned} \
                 jobserver token(s){}",
                if callback.is_err() { "; the cleanup callback panicked" } else { "" }
            ));
            let _ = tx.send(());
        };
        if let Err(e) = std::thread::Builder::new().name(CLEANUP_THREAD.into()).spawn(cleanup) {
            // No thread for it: the essential part runs here, panic-free.
            let _ = crate::process::kill_live_children(timing.reap);
            crate::jobserver::release_all();
            say_bounded(format!("slip: warning: cannot start the cleanup thread ({e})"));
            die(sig);
        }
        loop {
            match rx.recv_timeout(Duration::from_millis(50)) {
                // Done, or the cleanup panicked (its sender is gone).
                Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            }
            if start.elapsed() >= timing.bound {
                crate::process::kill_live_children_now();
                crate::jobserver::release_all();
                say_bounded(format!(
                    "slip: signal {sig}: cleanup still running after {:.1}s; exiting anyway",
                    timing.bound.as_secs_f64()
                ));
                break;
            }
            if let Some(again) = signals.pending().next()
                && start.elapsed() >= timing.duplicate
            {
                crate::process::kill_live_children_now();
                crate::jobserver::release_all();
                say_bounded(format!("slip: signal {again}: exiting without waiting for cleanup"));
                die(again);
            }
        }
        die(sig);
    });
    match spawned {
        Ok(_) => true,
        Err(e) => {
            say(format_args!("slip: warning: cannot start the signal thread ({e})"));
            false
        }
    }
}

#[cfg(not(unix))]
fn start(_: Box<dyn FnOnce() + Send>, _: Timing) -> bool {
    false
}

/// [`say`] on a thread of its own, waiting for it at most 100 ms: the signal
/// thread must not block on a full stderr pipe.
fn say_bounded(line: String) {
    let (tx, rx) = std::sync::mpsc::channel();
    let spawned = std::thread::Builder::new().spawn(move || {
        let _ = writeln!(std::io::stderr(), "{line}");
        let _ = tx.send(());
    });
    if spawned.is_ok() {
        let _ = rx.recv_timeout(Duration::from_millis(100));
    }
}

/// Ends the process as `sig` would by default (so the parent sees it killed
/// by that signal); if that does not end it, `_exit(128 + sig)` without
/// exit handlers, which `std::process::exit` would run and which can block
/// behind another thread.
#[cfg(unix)]
fn die(sig: i32) -> ! {
    let _ = signal_hook::low_level::emulate_default_handler(sig);
    signal_hook::low_level::exit(128 + sig)
}

#[cfg(not(unix))]
fn die(sig: i32) -> ! {
    std::process::exit(128 + sig)
}

/// For a thread that finds the process interrupted ([`interrupted`]): waits
/// for the signal thread to end the process, and ends it itself (`SIGTERM`'s
/// default action if no signal is recorded) should that not have happened
/// 5 s after [`Timing::bound`], so no thread waits forever.
pub fn exit_interrupted() -> ! {
    let deadline = Instant::now() + timing().bound + Duration::from_secs(5);
    loop {
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        std::thread::park_timeout(deadline - now);
    }
    die(interrupted().unwrap_or(15))
}

/// `main`'s exit: `code`, unless the process is being interrupted, in which
/// case the signal thread ends it (with the signal's status, after the
/// cleanup) instead of `main` racing it to `exit`.
pub fn finish(code: ExitCode) -> ExitCode {
    if interrupted().is_some() {
        exit_interrupted();
    }
    code
}

/// Makes a panic of any other thread during the cleanup wait for the
/// process to end ([`exit_interrupted`]) instead of unwinding into `main`,
/// which would exit with 101 before the children are reaped and the tokens
/// returned. Outside an interruption the previous hook runs as before.
fn hold_panics_while_interrupted() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let own = std::thread::current()
            .name()
            .is_some_and(|n| n == SIGNAL_THREAD || n == CLEANUP_THREAD);
        if interrupted().is_some() && !own {
            exit_interrupted();
        }
        previous(info);
    }));
}
