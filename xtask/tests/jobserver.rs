//! `cargo xtask jobserver status|refill` (spec/harness.md §5,
//! spec/landed/harness/WP-013.md "Jobserver").
//!
//! Every fifo is made by the test (`mkfifo`) in its own temporary
//! directory, and the test holds it open read+write for its whole life so
//! the kernel keeps the bytes in it. The host's own pool
//! (`/run/slip/jobserver.fifo`) is never opened: the in-process tests pass
//! an explicit environment, and the executable is started with `MAKEFLAGS`,
//! `SLIP_JOBSERVER_FIFO` and `SLIP_JOBS` cleared and only what the test
//! sets.

#![cfg(target_os = "linux")]

mod common;

use common::Fixture;
use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use xtask::jobserver::{self, Failure, Request};

/// `O_NONBLOCK` on Linux.
const O_NONBLOCK: i32 = 0o4000;

/// A fifo made by the test, held open read+write.
struct Fifo {
    path: PathBuf,
    _hold: File,
}

impl Fifo {
    fn new(f: &Fixture, name: &str, tokens: usize) -> Fifo {
        let path = f.path(name);
        let st = Command::new("mkfifo").arg(&path).status().expect("mkfifo runs");
        assert!(st.success(), "mkfifo {}", path.display());
        let mut hold = OpenOptions::new().read(true).write(true).open(&path).unwrap();
        hold.write_all(&vec![b'+'; tokens]).unwrap();
        Fifo { path, _hold: hold }
    }

    /// The bytes in the fifo, counted without blocking and written back.
    fn count(&self) -> usize {
        let mut f = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(O_NONBLOCK)
            .open(&self.path)
            .unwrap();
        let mut got = Vec::new();
        let mut buf = [0u8; 64];
        loop {
            match f.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => got.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) => panic!("{e}"),
            }
        }
        f.write_all(&got).unwrap();
        got.len()
    }

    fn shown(&self) -> String {
        self.path.display().to_string()
    }
}

fn args(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| (*s).to_owned()).collect()
}

/// An environment of exactly `vars`.
fn env_of(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
    let map: HashMap<String, OsString> =
        vars.iter().map(|(k, v)| ((*k).to_owned(), OsString::from(v))).collect();
    move |k| map.get(k).cloned()
}

fn run(a: &[&str], vars: &[(&str, &str)]) -> Result<String, Failure> {
    jobserver::run(&args(a), &env_of(vars))
}

fn parse(a: &[&str], vars: &[(&str, &str)]) -> Result<Request, Failure> {
    jobserver::parse(&args(a), &env_of(vars))
}

fn error(a: &[&str], vars: &[(&str, &str)]) -> String {
    match run(a, vars) {
        Err(Failure::Error(e)) => e,
        other => panic!("expected an error (exit 1), got {other:?}"),
    }
}

fn usage(a: &[&str], vars: &[(&str, &str)]) -> String {
    match run(a, vars) {
        Err(Failure::Usage(e)) => e,
        other => panic!("expected a usage error (exit 2), got {other:?}"),
    }
}

/// The built executable with the jobserver variables cleared and `vars` set.
fn xtask(a: &[&str], vars: &[(&str, &Path)]) -> (Option<i32>, String, String) {
    let mut c = Command::new(env!("CARGO_BIN_EXE_xtask"));
    c.arg("jobserver").args(a);
    for v in ["MAKEFLAGS", "SLIP_JOBSERVER_FIFO", "SLIP_JOBS", "CARGO_MAKEFLAGS"] {
        c.env_remove(v);
    }
    for (k, v) in vars {
        c.env(k, v);
    }
    let out = c.output().unwrap();
    (
        out.status.code(),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

// ------------------------------------------------------------------ status

/// §5 `status`: counts the idle tokens, prints the line, changes nothing.
#[test]
fn status_counts_and_leaves_the_tokens() {
    let f = Fixture::empty("js-status");
    let fifo = Fifo::new(&f, "pool", 3);
    let line = run(&["status", "--fifo", &fifo.shown()], &[]).unwrap();
    assert_eq!(line, format!("jobserver: {}: 3 of 8 tokens idle", fifo.shown()));
    assert_eq!(fifo.count(), 3, "status puts every token back");
    let line = run(&["status", "--fifo", &fifo.shown(), "--jobs", "4"], &[]).unwrap();
    assert_eq!(line, format!("jobserver: {}: 3 of 4 tokens idle", fifo.shown()));
    assert_eq!(fifo.count(), 3);
}

/// §5 `status` of an empty pool: 0 idle, and it does not block.
#[test]
fn status_of_an_empty_pool() {
    let f = Fixture::empty("js-empty");
    let fifo = Fifo::new(&f, "pool", 0);
    let line = run(&["status", "--fifo", &fifo.shown()], &[]).unwrap();
    assert_eq!(line, format!("jobserver: {}: 0 of 8 tokens idle", fifo.shown()));
    assert_eq!(fifo.count(), 0);
}

// ------------------------------------------------------------------ refill

/// §5 `refill` of a short pool tops it up to `<n>`.
#[test]
fn refill_tops_up_a_short_pool() {
    let f = Fixture::empty("js-short");
    let fifo = Fifo::new(&f, "pool", 3);
    let line = run(&["refill", "--fifo", &fifo.shown(), "--jobs", "5"], &[]).unwrap();
    assert_eq!(line, format!("jobserver: {}: 3 idle, 2 added", fifo.shown()));
    assert_eq!(fifo.count(), 5);
}

/// §5 `refill` of a full pool adds nothing.
#[test]
fn refill_of_a_full_pool_adds_nothing() {
    let f = Fixture::empty("js-full");
    let fifo = Fifo::new(&f, "pool", 5);
    let line = run(&["refill", "--fifo", &fifo.shown(), "--jobs", "5"], &[]).unwrap();
    assert_eq!(line, format!("jobserver: {}: 5 idle, 0 added", fifo.shown()));
    assert_eq!(fifo.count(), 5);
}

/// §5 `refill` of a pool above `<n>` leaves it alone (nothing is taken
/// out).
#[test]
fn refill_of_a_pool_above_n_leaves_it() {
    let f = Fixture::empty("js-above");
    let fifo = Fifo::new(&f, "pool", 7);
    let line = run(&["refill", "--fifo", &fifo.shown(), "--jobs", "5"], &[]).unwrap();
    assert_eq!(line, format!("jobserver: {}: 7 idle, 0 added", fifo.shown()));
    assert_eq!(fifo.count(), 7);
}

/// §5: `<n>` of `refill` defaults to `$SLIP_JOBS`, then 8.
#[test]
fn refill_takes_n_from_slip_jobs_then_8() {
    let f = Fixture::empty("js-refill-n");
    let fifo = Fifo::new(&f, "pool", 1);
    let line = run(&["refill", "--fifo", &fifo.shown()], &[("SLIP_JOBS", "3")]).unwrap();
    assert_eq!(line, format!("jobserver: {}: 1 idle, 2 added", fifo.shown()));
    let line = run(&["refill", "--fifo", &fifo.shown()], &[]).unwrap();
    assert_eq!(line, format!("jobserver: {}: 3 idle, 5 added", fifo.shown()));
    assert_eq!(fifo.count(), 8);
}

// --------------------------------------------------------------- the fifo

/// §5: the fifo is `--fifo`, else `MAKEFLAGS`, else `$SLIP_JOBSERVER_FIFO`
/// (each fifo holds a different number of tokens, so the line shows which
/// was read).
#[test]
fn fifo_precedence() {
    let f = Fixture::empty("js-prec");
    let (a, b, c) = (Fifo::new(&f, "a", 1), Fifo::new(&f, "b", 2), Fifo::new(&f, "c", 3));
    let makeflags = format!("-j --jobserver-auth=fifo:{}", b.shown());
    let all = [("MAKEFLAGS", makeflags.as_str()), ("SLIP_JOBSERVER_FIFO", &c.shown())];
    let line = run(&["status", "--fifo", &a.shown()], &all).unwrap();
    assert_eq!(line, format!("jobserver: {}: 1 of 8 tokens idle", a.shown()));
    let line = run(&["status"], &all).unwrap();
    assert_eq!(line, format!("jobserver: {}: 2 of 8 tokens idle", b.shown()));
    let line = run(&["status"], &all[1..]).unwrap();
    assert_eq!(line, format!("jobserver: {}: 3 of 8 tokens idle", c.shown()));
    // A MAKEFLAGS without a fifo (the descriptor form) names none.
    let line = run(&["status"], &[("MAKEFLAGS", "--jobserver-auth=3,4"), all[1]]).unwrap();
    assert_eq!(line, format!("jobserver: {}: 3 of 8 tokens idle", c.shown()));
    assert_eq!((a.count(), b.count(), c.count()), (1, 2, 3));
}

/// §5: with nothing set the fifo is `/run/slip/jobserver.fifo`. Only the
/// parse is checked: the host's pool is not opened.
#[test]
fn fifo_default_is_resolved_not_opened() {
    let r = parse(&["status"], &[]).unwrap();
    assert_eq!(
        r,
        Request { refill: false, fifo: PathBuf::from("/run/slip/jobserver.fifo"), jobs: 8 }
    );
    // An empty `$SLIP_JOBSERVER_FIFO` counts as unset.
    let r = parse(&["refill"], &[("SLIP_JOBSERVER_FIFO", "")]).unwrap();
    assert_eq!(r.fifo, PathBuf::from("/run/slip/jobserver.fifo"));
    assert!(r.refill);
}

// -------------------------------------------------------------------- <n>

/// §5: `<n>` is `--jobs`, else `$SLIP_JOBS`, else 8.
#[test]
fn n_precedence() {
    let p = |a: &[&str], v: &[(&str, &str)]| parse(a, v).unwrap().jobs;
    assert_eq!(p(&["status", "--jobs", "3"], &[("SLIP_JOBS", "5")]), 3);
    assert_eq!(p(&["status"], &[("SLIP_JOBS", "5")]), 5);
    assert_eq!(p(&["status"], &[]), 8);
    assert_eq!(p(&["status", "--jobs", "64"], &[]), 64);
}

/// §5: a `<n>` that is not a positive integer: from `--jobs` a usage error
/// (exit 2), from `$SLIP_JOBS` an error (exit 1).
#[test]
fn bad_n() {
    for bad in ["0", "x", "-1", "+3", " 3", "3 ", "", "1.5", "99999999999999999999999"] {
        let e = usage(&["status", "--fifo", "/nonexistent", "--jobs", bad], &[]);
        assert_eq!(e, format!("--jobs: `{bad}` is not a positive integer"));
        let e = error(&["status", "--fifo", "/nonexistent"], &[("SLIP_JOBS", bad)]);
        assert_eq!(e, format!("SLIP_JOBS: `{bad}` is not a positive integer"));
    }
}

// ----------------------------------------------------------------- errors

/// §5: a fifo that does not exist, and one that is not a fifo.
#[test]
fn missing_path_and_regular_file() {
    let f = Fixture::empty("js-err");
    let missing = f.path("nope");
    let e = error(&["status", "--fifo", &missing.display().to_string()], &[]);
    assert_eq!(e, format!("{}: does not exist", missing.display()));
    let e = error(&["refill", "--fifo", &missing.display().to_string()], &[]);
    assert_eq!(e, format!("{}: does not exist", missing.display()));
    f.write("plain", "++++");
    let plain = f.path("plain");
    let e = error(&["status", "--fifo", &plain.display().to_string()], &[]);
    assert_eq!(e, format!("{}: is not a fifo", plain.display()));
    let e = error(&["refill", "--fifo", &plain.display().to_string()], &[]);
    assert_eq!(e, format!("{}: is not a fifo", plain.display()));
    assert_eq!(std::fs::read_to_string(&plain).unwrap(), "++++", "a regular file is not written");
    // A directory is not a fifo either.
    let e = error(&["status", "--fifo", &f.root().display().to_string()], &[]);
    assert_eq!(e, format!("{}: is not a fifo", f.root().display()));
}

/// §5: usage errors (exit 2): an unknown option, a missing or unknown
/// command, an option without its value, an option given twice.
#[test]
fn usage_errors() {
    assert_eq!(usage(&["status", "--pool", "x"], &[]), "unknown option `--pool`");
    assert_eq!(usage(&["status", "extra"], &[]), "unknown option `extra`");
    assert_eq!(usage(&[], &[]), "missing command");
    assert_eq!(usage(&["drain"], &[]), "unknown command `drain`");
    assert_eq!(usage(&["status", "--fifo"], &[]), "`--fifo` needs a value");
    assert_eq!(usage(&["refill", "--jobs"], &[]), "`--jobs` needs a value");
    assert_eq!(usage(&["status", "--fifo", ""], &[]), "`--fifo` needs a non-empty path");
    assert_eq!(usage(&["status", "--fifo", "a", "--fifo", "b"], &[]), "`--fifo` given twice");
    assert_eq!(usage(&["status", "--jobs", "1", "--jobs", "2"], &[]), "`--jobs` given twice");
}

// ------------------------------------------------------- the executable

/// §5 through the built executable, with the host's jobserver variables
/// cleared: the line on stdout, exit 0; errors as `xtask jobserver: …` on
/// stderr with exit 1, usage errors with exit 2 and the usage text.
#[test]
fn through_the_executable() {
    let f = Fixture::empty("js-bin");
    let fifo = Fifo::new(&f, "pool", 2);
    let (code, out, err) = xtask(&["status"], &[("SLIP_JOBSERVER_FIFO", &fifo.path)]);
    assert_eq!(code, Some(0), "{err}");
    assert_eq!(out, format!("jobserver: {}: 2 of 8 tokens idle\n", fifo.shown()));
    assert_eq!(fifo.count(), 2);
    let (code, out, err) =
        xtask(&["refill", "--jobs", "4"], &[("SLIP_JOBSERVER_FIFO", &fifo.path)]);
    assert_eq!(code, Some(0), "{err}");
    assert_eq!(out, format!("jobserver: {}: 2 idle, 2 added\n", fifo.shown()));
    assert_eq!(fifo.count(), 4);
    let (code, out, err) = xtask(&["status", "--fifo", &f.path("nope").display().to_string()], &[]);
    assert_eq!(code, Some(1));
    assert_eq!(out, "");
    assert_eq!(err, format!("xtask jobserver: {}: does not exist\n", f.path("nope").display()));
    let (code, _, err) = xtask(&["status", "--bogus"], &[("SLIP_JOBSERVER_FIFO", &fifo.path)]);
    assert_eq!(code, Some(2));
    assert!(err.starts_with("xtask jobserver: unknown option `--bogus`\nusage:"), "{err}");
    assert!(
        err.contains("only correct\nwhile no client holds a token"),
        "the usage says so: {err}"
    );
    let (code, _, err) =
        xtask(&["status"], &[("SLIP_JOBSERVER_FIFO", &fifo.path), ("SLIP_JOBS", Path::new("0"))]);
    assert_eq!(code, Some(1));
    assert_eq!(err, "xtask jobserver: SLIP_JOBS: `0` is not a positive integer\n");
    assert_eq!(fifo.count(), 4);
}
