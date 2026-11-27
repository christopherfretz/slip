//! `spec/exec.md` §3 against fifos made by the tests. Ported from the tests
//! of Kiln's `xtask/src/jobs.rs`. No test reads `MAKEFLAGS` or touches the
//! host's pool: every fifo is created in a fresh temporary directory, and
//! `Pool::from_env` is only exercised through `Pool::from_makeflags` with
//! explicit values.
#![cfg(unix)]

mod common;

use slip_exec::jobserver::{
    self, DEFAULT_FIFO, DEFAULT_POOL_SIZE, Kind, Pool, RefillReport, Token, fifo_from_makeflags,
};
use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// A fifo in a directory of its own, with a holder end primed with the pool
/// (like `scripts/jobserver.sh`); the directory is removed on drop.
struct TestFifo {
    path: PathBuf,
    dir: PathBuf,
    _holder: File,
}

impl Drop for TestFifo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn scratch(tag: &str) -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let n = N.fetch_add(1, Ordering::SeqCst);
    let dir =
        common::work_root().join(format!("slip-exec-jobserver-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn fifo(tag: &str, tokens: usize) -> TestFifo {
    let dir = scratch(tag);
    let path = dir.join("fifo");
    let ok = std::process::Command::new("mkfifo").arg(&path).status().unwrap().success();
    assert!(ok, "mkfifo failed");
    let mut holder = OpenOptions::new().read(true).write(true).open(&path).unwrap();
    holder.write_all(&vec![b'+'; tokens]).unwrap();
    TestFifo { path, dir, _holder: holder }
}

fn idle(f: &TestFifo) -> usize {
    jobserver::status(&f.path).unwrap()
}

/// Polls `cond` for up to 10 s.
fn eventually(what: &str, mut cond: impl FnMut() -> bool) {
    let t = Instant::now();
    while !cond() {
        assert!(t.elapsed() < Duration::from_secs(10), "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// §3 "Honoured": the last `--jobserver-auth=` word wins, only its fifo form.
#[test]
fn s3_parses_makeflags() {
    let f = |s: &str| fifo_from_makeflags(s);
    assert_eq!(f(""), None);
    assert_eq!(f("-j8"), None);
    assert_eq!(
        f(" -j8 --jobserver-auth=fifo:/run/x/jobserver.fifo"),
        Some(PathBuf::from("/run/x/jobserver.fifo"))
    );
    assert_eq!(f("--jobserver-auth=fifo:/a --jobserver-auth=fifo:/b"), Some("/b".into()));
    // The last word is a descriptor form: no fifo, even after a fifo word.
    assert_eq!(f("--jobserver-auth=fifo:/a --jobserver-auth=3,4"), None);
    assert_eq!(f("-j --jobserver-auth=3,4"), None);
    assert_eq!(f("--jobserver-fds=3,4 -j"), None);
    assert_eq!(f("--jobserver-auth=fifo:"), None);
}

/// §3 defaults of the inspection commands, with the `SLIP_` names.
#[test]
fn s3_default_fifo_and_pool_size() {
    use jobserver::{resolve_fifo, resolve_pool_size};
    assert_eq!(resolve_fifo(None, None), PathBuf::from(DEFAULT_FIFO));
    assert_eq!(DEFAULT_FIFO, "/run/slip/jobserver.fifo");
    assert_eq!(resolve_fifo(None, Some("/x".into())), PathBuf::from("/x"));
    assert_eq!(resolve_fifo(Some("-j2"), Some("/x".into())), PathBuf::from("/x"));
    assert_eq!(
        resolve_fifo(Some("--jobserver-auth=fifo:/m"), Some("/x".into())),
        PathBuf::from("/m")
    );
    assert_eq!(resolve_pool_size(None), Ok(DEFAULT_POOL_SIZE));
    assert_eq!(DEFAULT_POOL_SIZE, 8);
    assert_eq!(resolve_pool_size(Some("3")), Ok(3));
    assert!(resolve_pool_size(Some("x")).is_err());
}

/// §3 "Fallback": no jobserver, the descriptor forms, a missing path and a
/// path that is not a fifo all make acquisition free.
#[test]
fn s3_fallback_is_free() {
    let dir = scratch("fallback");
    let plain = dir.join("plain");
    std::fs::write(&plain, "++").unwrap();
    assert!(Pool::from_fifo(&plain).is_err(), "not a fifo");
    assert!(Pool::from_fifo(&dir.join("missing")).is_err());
    for flags in [
        String::new(),
        "-j4 --jobserver-auth=3,4".to_owned(),
        format!("--jobserver-auth=fifo:{}", plain.display()),
        format!("--jobserver-auth=fifo:{}", dir.join("missing").display()),
    ] {
        let p = Pool::from_makeflags(&flags);
        assert_eq!(p.describe(), None, "{flags}");
        assert!(p.is_free());
        let toks: Vec<Token> = (0..16).map(|_| p.acquire()).collect();
        assert!(toks.iter().all(|t| t.kind() == Kind::Free), "{flags}");
        assert_eq!(p.try_acquire().map(|t| t.kind()), Some(Kind::Free));
    }
    // The plain file was never consumed.
    assert_eq!(std::fs::read(&plain).unwrap(), b"++");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// §3 "-j": the bound is carried, defaults to the cores, and is at least 1.
#[test]
fn s3_jobs_limit() {
    assert_eq!(Pool::none().jobs(), jobserver::available_cores());
    assert!(jobserver::available_cores() >= 1);
    assert_eq!(Pool::none().with_jobs(3).jobs(), 3);
    assert_eq!(Pool::none().with_jobs(0).jobs(), 1);
    let f = fifo("jobs", 0);
    assert_eq!(Pool::from_fifo(&f.path).unwrap().with_jobs(5).jobs(), 5);
}

/// §3 "The implicit token": concurrency stays within pool + 1, and every
/// pool byte comes back.
#[test]
fn s3_bounds_concurrency_by_pool_plus_implicit() {
    let f = fifo("bound", 2);
    let c = Pool::from_fifo(&f.path).unwrap();
    assert!(c.describe().unwrap().starts_with("fifo:"));
    let live = AtomicUsize::new(0);
    let peak = AtomicUsize::new(0);
    std::thread::scope(|s| {
        for _ in 0..8 {
            s.spawn(|| {
                for _ in 0..5 {
                    let _t = c.acquire();
                    let n = live.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(n, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(5));
                    live.fetch_sub(1, Ordering::SeqCst);
                }
            });
        }
    });
    let peak = peak.load(Ordering::SeqCst);
    assert!(peak <= 3, "peak {peak} exceeds 2 pool tokens + the implicit one");
    assert!(peak >= 2, "peak {peak}: the pool was never used");
    eventually("every byte back", || idle(&f) == 2);
}

/// §3 "The implicit token": the first acquisition reads nothing, and a
/// freed implicit token wakes a blocked waiter.
#[test]
fn s3_implicit_token_frees_a_waiter() {
    let f = fifo("implicit", 0);
    let c = Pool::from_fifo(&f.path).unwrap();
    let first = c.acquire();
    assert_eq!(first.kind(), Kind::Implicit);
    std::thread::scope(|s| {
        let h = s.spawn(|| c.acquire().kind());
        std::thread::sleep(Duration::from_millis(50));
        assert!(!h.is_finished(), "a second token out of an empty pool");
        drop(first);
        assert_eq!(h.join().unwrap(), Kind::Implicit);
    });
}

/// §3 "Release": a dropped token, an early error return and a panic all
/// give the byte back.
#[test]
fn s3_release_on_every_path() {
    let f = fifo("release-paths", 1);
    let c = Pool::from_fifo(&f.path).unwrap();
    let _implicit = c.acquire();
    // Drop.
    let t = c.acquire();
    assert_eq!(t.kind(), Kind::Pool(b'+'));
    assert_eq!(idle(&f), 0);
    drop(t);
    assert_eq!(idle(&f), 1);
    // Error return.
    let job = |c: &Pool| -> Result<(), String> {
        let _t = c.acquire();
        Err("failed".into())
    };
    assert!(job(&c).is_err());
    assert_eq!(idle(&f), 1);
    // Panic.
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let t = c.acquire();
        assert!(matches!(t.kind(), Kind::Pool(b'+')));
        panic!("worker died");
    }));
    assert!(r.is_err());
    assert_eq!(idle(&f), 1);
}

/// §3: `try_acquire` takes the implicit token, then a byte only if one is
/// there now, and never blocks.
#[test]
fn s3_try_acquire_never_blocks() {
    let fifo = fifo("try", 1);
    let pool = Pool::from_fifo(&fifo.path).unwrap();
    let a = pool.try_acquire().unwrap();
    assert_eq!(a.kind(), Kind::Implicit);
    let b = pool.try_acquire().unwrap();
    assert_eq!(b.kind(), Kind::Pool(b'+'));
    let t = Instant::now();
    assert!(pool.try_acquire().is_none(), "empty pool");
    assert!(t.elapsed() < Duration::from_secs(1));
    drop(b);
    assert_eq!(idle(&fifo), 1);
    drop(a);
    assert_eq!(pool.try_acquire().unwrap().kind(), Kind::Implicit);
}

/// §3 last paragraph: a request is answered over the caller's channel, by
/// the implicit token or a pool byte, in order.
#[test]
fn s3_request_delivers_over_a_channel() {
    let f = fifo("request", 1);
    let c = Pool::from_fifo(&f.path).unwrap();
    let (tx, rx) = mpsc::channel::<(u32, Token)>();
    let tx1 = tx.clone();
    let r1 = c.request(move |t| {
        let _ = tx1.send((1, t));
    });
    // The implicit token was free: delivered at once.
    let (n, t1) = rx.try_recv().unwrap();
    assert_eq!((n, t1.kind()), (1, Kind::Implicit));
    assert!(!r1.is_pending());
    let tx2 = tx.clone();
    let _r2 = c.request(move |t| {
        let _ = tx2.send((2, t));
    });
    let (n, t2) = rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!((n, t2.kind()), (2, Kind::Pool(b'+')));
    let tx3 = tx.clone();
    let r3 = c.request(move |t| {
        let _ = tx3.send((3, t));
    });
    std::thread::sleep(Duration::from_millis(50));
    assert!(rx.try_recv().is_err(), "nothing left in the pool");
    assert!(r3.is_pending());
    // A finished job's token goes to the waiting request.
    drop(t1);
    let (n, t3) = rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!((n, t3.kind()), (3, Kind::Implicit));
    drop((t2, t3));
    eventually("the byte back", || idle(&f) == 1);
}

/// §3 last paragraph: an abandoned request gets nothing, and a byte read
/// for it afterwards is written straight back.
#[test]
fn s3_abandoned_request_puts_a_late_byte_back() {
    let f = fifo("abandon", 0);
    let c = Pool::from_fifo(&f.path).unwrap();
    let _implicit = c.acquire();
    let (tx, rx) = mpsc::channel::<Token>();
    let r = c.request(move |t| {
        let _ = tx.send(t);
    });
    assert!(r.is_pending());
    // Let the helper block in its read, then abandon the request.
    std::thread::sleep(Duration::from_millis(50));
    drop(r);
    // A byte arrives (another client released one): the helper reads it
    // for nobody and puts it back.
    let mut w = OpenOptions::new().read(true).write(true).open(&f.path).unwrap();
    w.write_all(b"x").unwrap();
    std::thread::sleep(Duration::from_millis(100));
    assert!(rx.try_recv().is_err(), "an abandoned request got a token");
    eventually("the late byte back", || idle(&f) == 1);
}

/// §3 last paragraph: a request whose receiver is gone returns the token it
/// is handed.
#[test]
fn s3_request_with_a_dead_receiver_returns_its_token() {
    let f = fifo("dead-rx", 1);
    let c = Pool::from_fifo(&f.path).unwrap();
    let _implicit = c.acquire();
    let (tx, rx) = mpsc::channel::<Token>();
    drop(rx);
    let _r = c.request(move |t| {
        let _ = tx.send(t);
    });
    eventually("the byte back", || idle(&f) == 1);
}

/// §3 and §7: `release_all` writes back every held byte (delivered or not)
/// exactly once; later drops return nothing; pending requests and new
/// acquisitions are free.
#[test]
fn s3_release_all_returns_each_byte_once() {
    let f = fifo("release", 2);
    let c = Pool::from_fifo(&f.path).unwrap();
    let _implicit = c.acquire();
    let a = c.acquire();
    // Delivered into a channel but never received.
    let (tx, rx) = mpsc::channel::<Token>();
    let tx2 = tx.clone();
    let _ra = c.request(move |t| {
        let _ = tx.send(t);
    });
    eventually("the second byte delivered", || idle(&f) == 0);
    let (txp, rxp) = mpsc::channel::<Token>();
    let rp = c.request(move |t| {
        let _ = txp.send(t);
    });
    assert!(rp.is_pending());
    assert_eq!(c.release_all(), 2);
    assert_eq!(c.release_all(), 0, "idempotent");
    assert_eq!(rxp.recv_timeout(Duration::from_secs(10)).unwrap().kind(), Kind::Free);
    assert!(c.is_free());
    drop(a);
    drop(rx);
    drop(tx2);
    assert_eq!(c.acquire().kind(), Kind::Free);
    assert_eq!(idle(&f), 2);
    // The global form finds nothing more to return for this pool.
    assert_eq!(c.release_all(), 0);
}

/// §3 "Inspection": `status` counts without blocking; `refill` tops up to
/// `n` and leaves a fuller pool alone; neither accepts a non-fifo.
#[test]
fn s3_status_and_refill() {
    let f = fifo("refill", 3);
    assert_eq!(jobserver::status(&f.path).unwrap(), 3);
    assert_eq!(jobserver::refill(&f.path, 5).unwrap(), RefillReport { idle: 3, added: 2 });
    assert_eq!(jobserver::refill(&f.path, 5).unwrap(), RefillReport { idle: 5, added: 0 });
    assert_eq!(jobserver::refill(&f.path, 4).unwrap(), RefillReport { idle: 5, added: 0 });
    assert_eq!(jobserver::status(&f.path).unwrap(), 5);
    let empty = fifo("refill-empty", 0);
    let t = Instant::now();
    assert_eq!(jobserver::status(&empty.path).unwrap(), 0);
    assert_eq!(jobserver::refill(&empty.path, 2).unwrap(), RefillReport { idle: 0, added: 2 });
    assert!(t.elapsed() < Duration::from_secs(5));
    let p = empty.dir.join("plain");
    std::fs::write(&p, "").unwrap();
    assert!(jobserver::refill(&p, 1).is_err(), "not a fifo");
    assert!(jobserver::status(&p).is_err());
    assert!(jobserver::status(Path::new("/nonexistent/slip-fifo")).is_err());
}
