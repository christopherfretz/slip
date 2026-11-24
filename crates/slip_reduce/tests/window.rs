//! The first pass's window (`spec/edits.md` §5.6, landed WP-142 §4): with
//! several threads, a run is started only while it is fewer than
//! `WINDOW_PER_THREAD × threads` runs ahead of the earliest run not yet
//! folded, so at most that many runs' joined states wait; the earliest
//! unfolded run can always be started (no deadlock); the result is the
//! one-thread result.
//!
//! Every call runs on a thread of its own and is given 60 s: a call that
//! does not return fails the test instead of holding the test binary (and
//! a host-wide build slot) forever. The stores block for a short fixed time
//! at most (150 ms or 100 ms, under a second for the whole binary).

mod common;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use common::*;
use slip_model::Digest;
use slip_reduce::{Options, ReduceError, Reduced, Shards, WINDOW_PER_THREAD, reduce};

const FAN_IN: usize = 2;
const THREADS: usize = 4;
const WINDOW: usize = WINDOW_PER_THREAD * THREADS;
/// Runs of the first pass: many more than the window.
const RUNS: usize = 100;
const N: usize = RUNS * FAN_IN;

/// What a store does at one index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Act {
    Give,
    Fail,
    Panic,
}

/// One site claimed by every shard (as in `memory.rs`). The first `open`
/// of the first index of run `slow` waits until a shard of run `until` or
/// later has been opened, or for `wait`, whichever comes first; the
/// index `at.0` does `at.1`.
struct Gated {
    slow: Option<usize>,
    until: usize,
    wait: Duration,
    at: (usize, Act),
    /// The greatest run opened so far, and whether any.
    opened: Mutex<Option<usize>>,
    moved: Condvar,
    gated: AtomicBool,
    /// The greatest run opened when the slow run was let go.
    seen_at_release: Mutex<Option<usize>>,
}

impl Gated {
    fn new(slow: Option<usize>, until: usize, wait: Duration, at: (usize, Act)) -> Gated {
        Gated {
            slow,
            until,
            wait,
            at,
            opened: Mutex::new(None),
            moved: Condvar::new(),
            gated: AtomicBool::new(false),
            seen_at_release: Mutex::new(None),
        }
    }

    fn plain() -> Gated {
        Gated::new(None, 0, Duration::ZERO, (usize::MAX, Act::Give))
    }
}

impl Shards for Gated {
    fn open(&self, index: usize) -> Result<Option<Vec<u8>>, String> {
        let run = index / FAN_IN;
        {
            let mut o = self.opened.lock().unwrap();
            *o = Some(o.map_or(run, |m| m.max(run)));
            self.moved.notify_all();
        }
        if Some(run) == self.slow
            && index == run * FAN_IN
            && !self.gated.swap(true, Ordering::SeqCst)
        {
            let o = self.opened.lock().unwrap();
            let (o, _) = self
                .moved
                .wait_timeout_while(o, self.wait, |o| o.is_none_or(|m| m < self.until))
                .unwrap();
            *self.seen_at_release.lock().unwrap() = *o;
        }
        if index == self.at.0 {
            match self.at.1 {
                Act::Give => {}
                Act::Fail => return Err("the store cannot read it".into()),
                Act::Panic => panic!("the store panics at index {index}"),
            }
        }
        let s = ShardB::new(index)
            .claim(replace("/s/a.cc", 10, 11, vec![lit("g")]).notes(&["the rename"]))
            .build();
        Ok(Some(s.to_bytes()))
    }
    fn len(&self) -> usize {
        N
    }
    fn observer(&self, index: usize) -> Digest {
        observer(index)
    }
}

/// How a call ended within the time given.
enum Ended {
    Returned(Result<Reduced, ReduceError>),
    Panicked,
}

/// `reduce` over `store` on `threads` threads, on a thread of its own,
/// within 60 s: a call that does not return is a deadlock and fails.
fn within(store: &Arc<Gated>, threads: usize) -> Ended {
    let store = Arc::clone(store);
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let o = Options { fan_in: FAN_IN, threads, scratch: scratch("window") };
        let r = reduce(&script(), REQUEST, PLAN, 0, &*store, &o);
        assert_clean(&o.scratch);
        let _ = tx.send(r);
    });
    match rx.recv_timeout(Duration::from_secs(60)) {
        Ok(r) => Ended::Returned(r),
        Err(RecvTimeoutError::Disconnected) => Ended::Panicked,
        Err(RecvTimeoutError::Timeout) => {
            panic!("reduce did not return within 60 s on {threads} threads: a deadlock")
        }
    }
}

fn returned(e: Ended) -> Result<Reduced, ReduceError> {
    match e {
        Ended::Returned(r) => r,
        Ended::Panicked => panic!("reduce panicked"),
    }
}

/// The one-thread result over the same shards, to compare with.
fn one_thread(at: (usize, Act)) -> Result<Reduced, ReduceError> {
    returned(within(&Arc::new(Gated::new(None, 0, Duration::ZERO, at)), 1))
}

fn same(a: &Reduced, b: &Reduced) {
    assert_eq!(a.edits.to_bytes(), b.edits.to_bytes());
    assert_eq!(
        a.declared.as_ref().map(slip_model::declared::Declared::to_bytes),
        b.declared.as_ref().map(slip_model::declared::Declared::to_bytes)
    );
}

/// §5.6: the first run is slow (it waits until a run twice the window
/// ahead is opened, which the window forbids, or 150 ms). The other
/// threads go on to the end of the window and wait: no run at or past
/// the window is opened before the first run is let go, at most `W` runs'
/// states wait, and the result is the one-thread result.
#[test]
fn a_slow_first_run_holds_at_most_a_window() {
    let store = Arc::new(Gated::new(
        Some(0),
        2 * WINDOW,
        Duration::from_millis(150),
        (usize::MAX, Act::Give),
    ));
    let r = returned(within(&store, THREADS)).unwrap();
    let seen = store.seen_at_release.lock().unwrap().expect("the slow run was opened");
    eprintln!("slow first run: run {seen} opened at its release, {:?}", r.peak);
    assert!(r.peak.waiting_runs <= WINDOW as u64, "{:?}", r.peak);
    assert!(seen < WINDOW, "run {seen} was opened while run 0 was not folded");
    assert!(r.peak.sections <= THREADS as u64, "{:?}", r.peak);
    let one = one_thread((usize::MAX, Act::Give)).unwrap();
    same(&r, &one);
    assert_eq!(one.peak.waiting_runs, 1, "one thread folds each run at once");
}

/// §5.6: the last run is slow (100 ms): every run before it is folded as
/// it finishes, nothing waits on it, and the result is the one-thread
/// result.
#[test]
fn a_slow_last_run() {
    let store = Arc::new(Gated::new(
        Some(RUNS - 1),
        usize::MAX,
        Duration::from_millis(100),
        (usize::MAX, Act::Give),
    ));
    let r = returned(within(&store, THREADS)).unwrap();
    assert!(r.peak.waiting_runs <= WINDOW as u64, "{:?}", r.peak);
    same(&r, &one_thread((usize::MAX, Act::Give)).unwrap());
}

/// No deadlock: with more threads than fit while the earliest run is
/// slow, every worker but one reaches the end of the window and waits,
/// and the earliest run still starts, finishes and wakes them. The call
/// returns within the time given (`within`).
#[test]
fn the_earliest_unfolded_run_always_proceeds() {
    let store = Arc::new(Gated::new(
        Some(0),
        usize::MAX,
        Duration::from_millis(100),
        (usize::MAX, Act::Give),
    ));
    let r = returned(within(&store, 2)).unwrap();
    assert!(r.peak.waiting_runs <= (WINDOW_PER_THREAD * 2) as u64, "{:?}", r.peak);
    same(&r, &one_thread((usize::MAX, Act::Give)).unwrap());
}

/// §5.7: a shard that cannot be read inside a full window (the first run
/// slow, the failing index in run 5) is the `Input` error of the least
/// index that shows one, as with one thread; nothing hangs.
#[test]
fn a_failing_run_inside_a_full_window() {
    let at = (5 * FAN_IN + 1, Act::Fail);
    let store = Arc::new(Gated::new(Some(0), 2 * WINDOW, Duration::from_millis(150), at));
    let e = returned(within(&store, THREADS)).unwrap_err();
    assert_eq!(e, one_thread(at).unwrap_err());
    let ReduceError::Input { observer: o, .. } = e else { panic!("not an Input error: {e:?}") };
    assert_eq!(o, observer(at.0));
}

/// A worker that panics inside a full window stops the pass and wakes the
/// workers waiting on the window: the panic reaches the caller, and the
/// call does not hang.
#[test]
fn a_panicking_worker_inside_a_full_window() {
    let at = (5 * FAN_IN, Act::Panic);
    let store = Arc::new(Gated::new(Some(0), usize::MAX, Duration::from_millis(100), at));
    assert!(matches!(within(&store, THREADS), Ended::Panicked));
}

/// Without a slow run, the window changes nothing either.
#[test]
fn several_threads_without_a_slow_run() {
    let r = returned(within(&Arc::new(Gated::plain()), THREADS)).unwrap();
    assert!(r.peak.waiting_runs <= WINDOW as u64, "{:?}", r.peak);
    same(&r, &one_thread((usize::MAX, Act::Give)).unwrap());
}
