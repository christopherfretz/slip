//! The first pass's window (`spec/edits.md` §5.6, landed WP-142 §4): with
//! several threads, a run is started only while it is fewer than
//! `WINDOW_PER_THREAD × threads` runs ahead of the earliest run not yet
//! folded, so at most that many runs' joined states wait; the earliest
//! unfolded run can always be started (no deadlock); every waiting worker
//! is woken when the pass moves, fails or a run panics; a run's panic
//! reaches the caller with its payload; the result is the one-thread
//! result. Built on the second review's probes of WP-142.
//!
//! Every call runs on a thread of its own and is given 30 s: a call that
//! does not return fails the test instead of holding the test binary (and
//! a host-wide build slot) forever. The stores block for short fixed times
//! (at most 200 ms a call; a few seconds for the whole binary).

mod common;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::ThreadId;
use std::time::Duration;

use common::*;
use slip_model::Digest;
use slip_reduce::{Options, ReduceError, Reduced, Shards, WINDOW_PER_THREAD, reduce};

const FAN_IN: usize = 2;
const LIMIT: Duration = Duration::from_secs(30);

fn window(threads: usize) -> usize {
    WINDOW_PER_THREAD * threads
}

fn runs_of(n: usize) -> usize {
    n.div_ceil(FAN_IN).max(1)
}

/// What a store does.
#[derive(Clone, Default)]
struct Plan {
    /// `(run, until, max, grace)`: the first open of `run`'s first index
    /// waits until a run `until` or later has been opened, or `max`; then
    /// `grace` more.
    gate: Option<(usize, usize, Duration, Duration)>,
    /// `(run, wait)`: the first open of `run`'s first index waits `wait`.
    delay: Option<(usize, Duration)>,
    /// A run whose own run file cannot be created (a directory is put at
    /// its name): an `Io` error of the run, not of a shard.
    io_fail_run: Option<usize>,
    /// An index whose first open panics.
    panic_at: Option<usize>,
    /// An index whose second open (the second pass) panics.
    panic_again_at: Option<usize>,
    /// An index whose open fails (an `Input` error of the shard).
    fail_at: Option<usize>,
}

/// One site claimed by every shard; [`Plan`] says what else.
struct Probe {
    n: usize,
    plan: Plan,
    scratch: PathBuf,
    opens: Vec<Mutex<u32>>,
    /// The greatest run opened so far.
    opened: Mutex<Option<usize>>,
    moved: Condvar,
    /// The greatest run opened when the gated run was let go.
    seen_at_release: Mutex<Option<usize>>,
    made_dir: Mutex<Option<PathBuf>>,
    /// The threads that opened a shard, by pass (the first open of a
    /// shard is the first pass's, the second the second pass's).
    threads: Mutex<[HashSet<ThreadId>; 2]>,
    pause: (Mutex<()>, Condvar),
}

impl Probe {
    fn new(n: usize, plan: Plan, scratch: PathBuf) -> Probe {
        Probe {
            n,
            plan,
            scratch,
            opens: (0..n).map(|_| Mutex::new(0)).collect(),
            opened: Mutex::new(None),
            moved: Condvar::new(),
            seen_at_release: Mutex::new(None),
            made_dir: Mutex::new(None),
            threads: Mutex::new([HashSet::new(), HashSet::new()]),
            pause: (Mutex::new(()), Condvar::new()),
        }
    }

    /// Waits `d` (a timed wait on a condition nobody signals).
    fn wait(&self, d: Duration) {
        if d > Duration::ZERO {
            let g = self.pause.0.lock().unwrap();
            drop(self.pause.1.wait_timeout(g, d).unwrap());
        }
    }
}

impl Shards for Probe {
    fn open(&self, index: usize) -> Result<Option<Vec<u8>>, String> {
        let run = index / FAN_IN;
        {
            let mut o = self.opened.lock().unwrap();
            *o = Some(o.map_or(run, |m| m.max(run)));
            self.moved.notify_all();
        }
        let nth = {
            let mut c = self.opens[index].lock().unwrap();
            *c += 1;
            *c
        };
        if let Some(by_pass) = self.threads.lock().unwrap().get_mut(nth as usize - 1) {
            by_pass.insert(std::thread::current().id());
        }
        if nth == 1 && index == run * FAN_IN {
            if let Some((g, until, max, grace)) = self.plan.gate
                && g == run
            {
                let o = self.opened.lock().unwrap();
                let (o, _) =
                    self.moved.wait_timeout_while(o, max, |o| o.is_none_or(|m| m < until)).unwrap();
                *self.seen_at_release.lock().unwrap() = *o;
                drop(o);
                self.wait(grace);
            }
            if let Some((d, wait)) = self.plan.delay
                && d == run
            {
                self.wait(wait);
            }
            if self.plan.io_fail_run == Some(run) {
                let basis = std::fs::read_dir(&self.scratch)
                    .unwrap()
                    .map(|e| e.unwrap().file_name().into_string().unwrap())
                    .find(|n| n.ends_with("-basis"))
                    .expect("the basis file exists during the first pass");
                let stem = basis.strip_suffix("-basis").unwrap();
                let p = self.scratch.join(format!("{stem}-0-{run}"));
                std::fs::create_dir(&p).unwrap();
                *self.made_dir.lock().unwrap() = Some(p);
            }
        }
        assert!(nth != 1 || self.plan.panic_at != Some(index), "the store panics at index {index}");
        assert!(
            nth != 2 || self.plan.panic_again_at != Some(index),
            "the store panics again at index {index}"
        );
        if self.plan.fail_at == Some(index) {
            return Err("the store cannot read it".into());
        }
        let s = ShardB::new(index)
            .claim(replace("/s/a.cc", 10, 11, vec![lit("g")]).notes(&["the rename"]))
            .build();
        Ok(Some(s.to_bytes()))
    }
    fn len(&self) -> usize {
        self.n
    }
    fn observer(&self, index: usize) -> Digest {
        observer(index)
    }
}

/// How a call ended within the time given.
enum Ended {
    Returned(Box<Result<Reduced, ReduceError>>, Arc<Probe>),
    /// The panic's message.
    Panicked(String),
}

/// `reduce` over `n` shards of `plan` on `threads` threads, on a thread of
/// its own, within [`LIMIT`]: a call that does not return is a deadlock
/// and fails the test.
fn within(n: usize, plan: Plan, threads: usize, what: &str) -> Ended {
    let dir = scratch("window");
    let store = Arc::new(Probe::new(n, plan, dir.clone()));
    let mine = Arc::clone(&store);
    let (tx, rx) = channel();
    let h = std::thread::spawn(move || {
        let o = Options { fan_in: FAN_IN, threads, scratch: dir };
        let r = reduce(&script(), REQUEST, PLAN, 0, &*mine, &o);
        if let Some(p) = mine.made_dir.lock().unwrap().take() {
            std::fs::remove_dir(&p).unwrap();
        }
        assert_clean(&o.scratch);
        let _ = tx.send(r);
    });
    match rx.recv_timeout(LIMIT) {
        Ok(r) => Ended::Returned(Box::new(r), store),
        Err(RecvTimeoutError::Disconnected) => {
            let e = h.join().unwrap_err();
            let m = e
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| e.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                .unwrap_or_default();
            Ended::Panicked(m)
        }
        Err(RecvTimeoutError::Timeout) => {
            panic!("reduce did not return within {LIMIT:?} ({what}, {threads} threads): a deadlock")
        }
    }
}

fn ok(e: Ended, what: &str) -> (Reduced, Arc<Probe>) {
    match e {
        Ended::Returned(r, p) => (r.unwrap_or_else(|e| panic!("{what}: {e:?}")), p),
        Ended::Panicked(m) => panic!("{what}: panicked: {m}"),
    }
}

/// The one-thread result over `n` plain shards, to compare with.
fn one_thread(n: usize) -> Reduced {
    ok(within(n, Plan::default(), 1, "one thread"), "one thread").0
}

fn same(a: &Reduced, b: &Reduced, what: &str) {
    assert_eq!(a.edits.to_bytes(), b.edits.to_bytes(), "{what}: the edit set differs");
    assert_eq!(
        a.declared.as_ref().map(slip_model::declared::Declared::to_bytes),
        b.declared.as_ref().map(slip_model::declared::Declared::to_bytes),
        "{what}: the declared changes differ"
    );
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// §5.6: the first run is slow (it waits until a run twice the window
/// ahead is opened, which the window forbids, or 150 ms). The other
/// threads go on to the end of the window and wait: no run at or past
/// the window is opened before the first run is let go, at most `W` runs'
/// states wait, and the result is the one-thread result.
#[test]
fn a_slow_first_run_holds_at_most_a_window() {
    let t = 4;
    let n = 50 * FAN_IN;
    let plan = Plan { gate: Some((0, 2 * window(t), ms(150), ms(0))), ..Plan::default() };
    let (r, p) = ok(within(n, plan, t, "slow first"), "slow first");
    let seen = p.seen_at_release.lock().unwrap().expect("the slow run was opened");
    eprintln!("slow first run: run {seen} opened at its release, {:?}", r.peak);
    assert!(r.peak.waiting_runs <= window(t) as u64, "{:?}", r.peak);
    assert!(seen < window(t), "run {seen} was opened while run 0 was not folded");
    assert!(r.peak.sections <= t as u64, "{:?}", r.peak);
    let one = one_thread(n);
    same(&r, &one, "slow first");
    assert_eq!(one.peak.waiting_runs, 1, "one thread folds each run at once");
}

/// §5.6: the window is reached exactly, for every thread count 2 to 8: the
/// first run waits until run `W - 1` is opened and 200 ms more, so that
/// runs 1 to `W - 1` have finished and wait when it is folded: `W` states.
#[test]
fn the_window_is_reached_exactly() {
    for t in 2..=8 {
        let w = window(t);
        let n = 2 * w * FAN_IN;
        let plan = Plan { gate: Some((0, w - 1, ms(2000), ms(200))), ..Plan::default() };
        let what = format!("exact, {t} threads");
        let (r, _) = ok(within(n, plan, t, &what), &what);
        assert_eq!(r.peak.waiting_runs, w as u64, "{what}: {:?}", r.peak);
    }
}

/// §5.6: the last run is slow (100 ms): every run before it is folded as
/// it finishes, nothing waits on it, and the result is the one-thread
/// result.
#[test]
fn a_slow_last_run() {
    let t = 4;
    let n = 50 * FAN_IN;
    let plan = Plan { delay: Some((runs_of(n) - 1, ms(100))), ..Plan::default() };
    let (r, _) = ok(within(n, plan, t, "slow last"), "slow last");
    assert!(r.peak.waiting_runs <= window(t) as u64, "{:?}", r.peak);
    same(&r, &one_thread(n), "slow last");
}

/// No deadlock: with the earliest run slow, every other worker reaches
/// the end of the window and waits, and the earliest run still finishes
/// and wakes them. The call returns within the time given.
#[test]
fn the_earliest_unfolded_run_always_proceeds() {
    let t = 2;
    let n = 50 * FAN_IN;
    let plan = Plan { gate: Some((0, usize::MAX, ms(100), ms(0))), ..Plan::default() };
    let (r, _) = ok(within(n, plan, t, "earliest"), "earliest");
    assert!(r.peak.waiting_runs <= window(t) as u64, "{:?}", r.peak);
    same(&r, &one_thread(n), "earliest");
}

/// The end of the pass: the earliest run slow and one or two runs past
/// the window, so that every other worker sleeps on the window and one
/// fold must wake them all (one woken worker would leave the others
/// asleep when the runs run out).
#[test]
fn every_waiting_worker_is_woken_at_the_end() {
    for t in 3..=8 {
        let w = window(t);
        for extra in [1, 2] {
            let n = (w + extra) * FAN_IN;
            let plan = Plan { gate: Some((0, usize::MAX, ms(80), ms(0))), ..Plan::default() };
            let what = format!("{t} threads, {} runs", w + extra);
            let (r, _) = ok(within(n, plan, t, &what), &what);
            assert!(r.peak.waiting_runs <= w as u64, "{what}: {:?}", r.peak);
        }
    }
}

/// Two slow early runs (run 0 30 ms, run 1 80 ms): when run 0 is folded
/// the waiting workers are woken while run 1 still holds the window, and
/// each must check the window again before it starts a run.
#[test]
fn two_slow_early_runs() {
    for t in 3..=8 {
        let w = window(t);
        let n = 4 * w * FAN_IN;
        let plan = Plan {
            gate: Some((0, usize::MAX, ms(30), ms(0))),
            delay: Some((1, ms(80))),
            ..Plan::default()
        };
        let what = format!("two slow, {t} threads");
        let (r, _) = ok(within(n, plan, t, &what), &what);
        assert!(r.peak.waiting_runs <= w as u64, "{what}: {:?}", r.peak);
    }
}

/// A run's own error (its run file cannot be created) is the result.
fn io_error(e: Ended, run: usize, what: &str) {
    match e {
        Ended::Returned(r, _) => match *r {
            Err(ReduceError::Io(m)) => {
                assert!(m.contains(&format!("-0-{run}")), "{what}: another Io error: {m}");
            }
            other => panic!("{what}: not an Io error: {:?}", other.map(|r| r.peak)),
        },
        Ended::Panicked(m) => panic!("{what}: panicked: {m}"),
    }
}

/// An error of the earliest run while every other worker waits on the
/// window (the earliest run slow, then failing): only the error's wake-up
/// lets them go. The run's `Io` error is the result, on 1 to 8 threads.
#[test]
fn an_error_of_the_earliest_run_wakes_the_window() {
    for t in 1..=8 {
        let n = 3 * window(t) * FAN_IN;
        let plan = Plan {
            gate: Some((0, usize::MAX, ms(40), ms(0))),
            io_fail_run: Some(0),
            ..Plan::default()
        };
        let what = format!("earliest failing, {t} threads");
        io_error(within(n, plan, t, &what), 0, &what);
    }
}

/// The positions of the probes: in, at the end of, past and well past
/// the window, and the last run.
fn positions(t: usize) -> Vec<usize> {
    let w = window(t);
    let last = runs_of(3 * w * FAN_IN) - 1;
    let mut p = vec![0, 1, 5.min(w - 1), w - 1, w, w + 2, 2 * w, last];
    p.dedup();
    p
}

/// A run's own error at each position, the first run slow, on 1, 2, 4
/// and 8 threads: that error is the result, and nothing hangs.
#[test]
fn a_run_error_at_each_position() {
    for t in [1, 2, 4, 8] {
        let n = 3 * window(t) * FAN_IN;
        for fail in positions(t) {
            let what = format!("{t} threads, run {fail} fails");
            let plan = Plan {
                gate: Some((0, usize::MAX, ms(40), ms(0))),
                io_fail_run: Some(fail),
                ..Plan::default()
            };
            io_error(within(n, plan, t, &what), fail, &what);
        }
    }
    // Fewer runs than threads.
    for (n, t) in [(1, 8), (2, 8), (5, 8), (3, 1)] {
        let what = format!("{n} shards, {t} threads");
        let plan = Plan { io_fail_run: Some(0), ..Plan::default() };
        io_error(within(n, plan, t, &what), 0, &what);
    }
}

/// A run's panic at each position, the first run slow, on 1, 2, 4 and 8
/// threads: the panic reaches the caller with its own message (the first
/// pass keeps its payload and resumes it), and nothing hangs.
#[test]
fn a_run_panic_at_each_position_reaches_the_caller() {
    for t in [1, 2, 4, 8] {
        let n = 3 * window(t) * FAN_IN;
        for run in positions(t) {
            let what = format!("{t} threads, run {run} panics");
            let index = run * FAN_IN + 1;
            let plan = Plan {
                gate: Some((0, usize::MAX, ms(40), ms(0))),
                panic_at: Some(index),
                ..Plan::default()
            };
            match within(n, plan, t, &what) {
                Ended::Panicked(m) => {
                    assert_eq!(m, format!("the store panics at index {index}"), "{what}");
                }
                Ended::Returned(..) => panic!("{what}: returned"),
            }
        }
    }
}

/// A panic in the second pass (the pool of the second pass and of the
/// merges) reaches the caller with its own message too, on 1 and 4
/// threads.
#[test]
fn a_panic_of_the_second_pass_reaches_the_caller() {
    for t in [1, 4] {
        let index = 7;
        let plan = Plan { panic_again_at: Some(index), ..Plan::default() };
        let what = format!("second pass, {t} threads");
        match within(20 * FAN_IN, plan, t, &what) {
            Ended::Panicked(m) => {
                assert_eq!(m, format!("the store panics again at index {index}"), "{what}");
            }
            Ended::Returned(..) => panic!("{what}: returned"),
        }
    }
}

/// §5.7: a shard that cannot be read inside a full window (the first run
/// slow, the failing index in run 5) is the `Input` error of the least
/// index that shows one, as with one thread; nothing hangs.
#[test]
fn a_failing_shard_inside_a_full_window() {
    let n = 50 * FAN_IN;
    let at = 5 * FAN_IN + 1;
    let plan = Plan {
        gate: Some((0, 2 * window(4), ms(150), ms(0))),
        fail_at: Some(at),
        ..Plan::default()
    };
    let error = |e: Ended| match e {
        Ended::Returned(r, _) => r.err().expect("an Input error"),
        Ended::Panicked(m) => panic!("panicked: {m}"),
    };
    let e = error(within(n, plan, 4, "failing shard"));
    let one = error(within(n, Plan { fail_at: Some(at), ..Plan::default() }, 1, "one thread"));
    assert_eq!(e, one);
    let ReduceError::Input { observer: o, .. } = e else { panic!("not an Input error: {e:?}") };
    assert_eq!(o, observer(at));
}

/// `threads` far beyond the runs: the window saturates instead of
/// wrapping to 0 (which would leave every worker waiting), at most one
/// thread per run is spawned (3 runs: at most 3 threads open shards in
/// each pass), and
/// the result is the one-thread result.
#[test]
fn a_huge_thread_count() {
    let n = 3 * FAN_IN;
    let one = one_thread(n);
    for t in [1usize << 62, usize::MAX / 4 + 1, usize::MAX, 8] {
        let what = format!("{t} threads");
        let (r, p) = ok(within(n, Plan::default(), t, &what), &what);
        same(&r, &one, &what);
        let spawned = p.threads.lock().unwrap().clone().map(|s| s.len());
        assert!(spawned[0] <= 3 && spawned[1] <= 3, "{what}: {spawned:?} threads by pass");
        assert!(r.peak.sections <= 3, "{what}: {:?}", r.peak);
    }
}

/// Without a slow run, the window changes nothing either.
#[test]
fn several_threads_without_a_slow_run() {
    let n = 50 * FAN_IN;
    let (r, _) = ok(within(n, Plan::default(), 4, "plain"), "plain");
    assert!(r.peak.waiting_runs <= window(4) as u64, "{:?}", r.peak);
    same(&r, &one_thread(n), "plain");
}
