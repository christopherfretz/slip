//! `cargo xtask transform`: the transform test runner (`spec/harness.md`
//! §6.2 to §6.4).
//!
//! ```text
//! cargo xtask transform [--only <substring>]… [--bless] [-j <n>] [--no-clang] [--slip <path>]
//!         [--verbose]
//! ```
//!
//! The runner drives the `slip` executable from outside and judges it by
//! files: the exit status, the state, the standard output against
//! `expect.txt`, the tree against the expected post-image, then the
//! oracles. It knows nothing of C++ beyond handing files to a compiler.
//!
//! - [`test_file`]: a test's directory and `test.txt` ([`parse_test`]);
//! - [`tree`]: trees of files, compared byte for byte; the work directory;
//! - [`run`]: one test run and judged ([`run_one`]), and every `slip`
//!   command line ([`run::cmdline`]);
//! - [`oracles`]: the five oracles;
//! - [`mutate`]: the mutated edit sets ([`mutate::mutations`]).
//!
//! Each test works in `<target>/transform/<area>/<name>/`, removed and
//! made again by explicit path for each run, with `tree/`, `store/`,
//! `cdb.json`, `back/` and `determinism/`; it is removed when the test
//! passes and kept, its path printed, when it does not
//! (`spec/landed/harness/WP-077.md`).

pub mod mutate;
pub mod oracles;
pub mod run;
pub mod test_file;
pub mod tree;

pub use test_file::{Allow, Expected, Oracle, Test, parse_test};

use slip_exec::jobserver::Pool;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The outcome of one test (§6). `Fail`, `Error`, `XPass` and `Killed` fail
/// the gate; `XFail` does not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Pass,
    /// The message names every difference found (the first of each kind).
    Fail(String),
    /// An `xfail` test failed.
    XFail,
    /// An `xfail` test passed.
    XPass,
    /// The test could not be judged; never masked by `xfail`, never blessed.
    Error(String),
    /// A `slip` was killed by a signal or by its timeout; never masked by
    /// `xfail`.
    Killed(String),
}

impl Outcome {
    /// The tag printed for it.
    pub fn tag(&self) -> &'static str {
        match self {
            Outcome::Pass => "PASS",
            Outcome::Fail(_) => "FAIL",
            Outcome::XFail => "XFAIL",
            Outcome::XPass => "XPASS",
            Outcome::Error(_) => "ERROR",
            Outcome::Killed(_) => "KILLED",
        }
    }

    /// Whether it fails the gate.
    pub fn fails_gate(&self) -> bool {
        matches!(self, Outcome::Fail(_) | Outcome::Error(_) | Outcome::XPass | Outcome::Killed(_))
    }
}

/// An oracle that did not run for a test: turned off by `oracle:
/// no-<name>` (`off`), or skipped, with the reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub oracle: Oracle,
    pub off: bool,
    pub reason: String,
}

/// A test's outcome, whether its clang oracle was skipped for want of a
/// compiler (or `--no-clang`), and every oracle that did not run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub outcome: Outcome,
    pub clang_skipped: bool,
    pub notes: Vec<Note>,
}

/// Runs `test` with the executable `slip` in the work directory `work`
/// (`<target>/transform/<area>/<name>`, refused with `Error` otherwise)
/// and judges it; `clang` is the compiler of the clang oracle, `None` to
/// skip it; `bless` rewrites `expect.txt` and `post/` (§6.3).
pub fn run_one(
    test: &Test,
    slip: &Path,
    work: &Path,
    clang: Option<&Path>,
    bless: bool,
) -> Outcome {
    run::run_test(test, slip, work, clang, bless).outcome
}

/// [`run_one`] with the clang oracle's skip.
pub fn run_test(
    test: &Test,
    slip: &Path,
    work: &Path,
    clang: Option<&Path>,
    bless: bool,
) -> Report {
    run::run_test(test, slip, work, clang, bless)
}

/// The test directories under `<root>/tests/transform/`: every directory
/// `<area>/<name>`, in byte order of paths. None if the directory does
/// not exist. Every entry of `tests/transform/` and of an area must be a
/// directory (not a symbolic link): anything else is an error, not
/// ignored.
pub fn discover(root: &Path) -> Result<Vec<PathBuf>, String> {
    let base = root.join("tests").join("transform");
    match std::fs::symlink_metadata(&base) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("{}: {e}", base.display())),
        Ok(m) if !m.is_dir() => {
            return Err(format!("{}: not a directory", base.display()));
        }
        Ok(_) => {}
    }
    let dirs = |d: &Path| -> Result<Vec<PathBuf>, String> {
        let mut v = Vec::new();
        for e in std::fs::read_dir(d).map_err(|e| format!("{}: {e}", d.display()))? {
            let e = e.map_err(|e| format!("{}: {e}", d.display()))?;
            let kind = e.file_type().map_err(|err| format!("{}: {err}", e.path().display()))?;
            let rel = e.path();
            let rel = rel.strip_prefix(root).unwrap_or(&rel).display().to_string();
            if kind.is_symlink() {
                return Err(format!("{rel}: a symbolic link, not a test or an area directory"));
            }
            if !kind.is_dir() {
                return Err(format!(
                    "{rel}: not a directory (tests are tests/transform/<area>/<name>/)"
                ));
            }
            v.push(e.path());
        }
        v.sort();
        Ok(v)
    };
    let mut out = Vec::new();
    for area in dirs(&base)? {
        out.extend(dirs(&area)?);
    }
    Ok(out)
}

/// The clang oracle's compiler (§6.4): `$SLIP_CLANG` if set and not empty,
/// else `clang++-19`, else `clang++` on `PATH`.
pub fn find_clang(env: &dyn Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    if let Some(c) = env("SLIP_CLANG").filter(|c| !c.is_empty()) {
        return Some(PathBuf::from(c));
    }
    let path = env("PATH")?;
    for name in ["clang++-19", "clang++"] {
        for dir in std::env::split_paths(&path) {
            let p = dir.join(name);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

/// The cargo target directory: `$CARGO_TARGET_DIR` (relative to `root`)
/// if set and not empty, else `<root>/target`.
pub fn target_dir(root: &Path) -> PathBuf {
    match std::env::var_os("CARGO_TARGET_DIR").filter(|d| !d.is_empty()) {
        Some(d) => root.join(d),
        None => root.join("target"),
    }
}

const USAGE: &str = "usage: cargo xtask transform [--only <substring>]… [--bless] [-j <n>] \
                     [--no-clang] [--slip <path>] [--verbose]";

/// A parsed command line.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Options {
    pub only: Vec<String>,
    pub bless: bool,
    pub jobs: Option<usize>,
    pub no_clang: bool,
    pub slip: Option<PathBuf>,
    /// Print, per test, each oracle that did not run and why.
    pub verbose: bool,
}

/// Parses the arguments after `transform`; `Err` is the usage error.
pub fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut o = Options::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = |name: &str| it.next().cloned().ok_or(format!("`{name}` needs a value"));
        match a.as_str() {
            "--only" => o.only.push(value("--only")?),
            "--bless" => o.bless = true,
            "--no-clang" => o.no_clang = true,
            "--verbose" => o.verbose = true,
            "--slip" => {
                let v = value("--slip")?;
                if v.is_empty() || o.slip.is_some() {
                    return Err("`--slip` given twice or empty".into());
                }
                o.slip = Some(PathBuf::from(v));
            }
            "-j" => {
                let v = value("-j")?;
                let n = v
                    .parse::<usize>()
                    .ok()
                    .filter(|n| *n > 0 && v.bytes().all(|b| b.is_ascii_digit()));
                let Some(n) = n else { return Err(format!("-j: `{v}` is not a positive integer")) };
                if o.jobs.replace(n).is_some() {
                    return Err("`-j` given twice".into());
                }
            }
            other => return Err(format!("unexpected argument `{other}`")),
        }
    }
    Ok(o)
}

/// `cargo xtask transform …`.
pub fn main(root: &Path, args: &[String]) -> ExitCode {
    let opts = match parse_args(args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("xtask transform: {e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    if run_all(root, &opts) { ExitCode::SUCCESS } else { ExitCode::from(1) }
}

/// Runs every selected test and prints the outcomes; whether the gate
/// passes.
pub fn run_all(root: &Path, opts: &Options) -> bool {
    let tests = match discover(root) {
        Ok(t) => t,
        Err(e) => {
            println!("transform: {e}");
            return false;
        }
    };
    let selected: Vec<PathBuf> = tests
        .into_iter()
        .filter(|t| {
            let rel = t.strip_prefix(root.join("tests").join("transform")).unwrap_or(t);
            let rel = rel.to_string_lossy();
            opts.only.is_empty() || opts.only.iter().any(|o| rel.contains(o.as_str()))
        })
        .collect();
    if !opts.only.is_empty() && selected.is_empty() {
        println!("transform: --only {} matches no test", opts.only.join(" --only "));
        return false;
    }
    let slip = match (&opts.slip, selected.is_empty()) {
        (Some(s), _) => s.clone(),
        (None, true) => PathBuf::new(),
        (None, false) => match build_slip(root) {
            Ok(s) => s,
            Err(e) => {
                println!("transform: {e}");
                return false;
            }
        },
    };
    let clang = if opts.no_clang { None } else { find_clang(&|n| std::env::var_os(n)) };
    let target = target_dir(root);

    let pool = Pool::from_env();
    let jobs = opts.jobs.unwrap_or_else(|| pool.jobs()).min(selected.len()).max(1);
    let next = AtomicUsize::new(0);
    let reports: Mutex<Vec<Option<Ran>>> = Mutex::new(vec![None; selected.len()]);
    std::thread::scope(|s| {
        for _ in 0..jobs {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(dir) = selected.get(i) else { break };
                    let token = pool.acquire();
                    let r = one(dir, &slip, &target, clang.as_deref(), opts.bless);
                    drop(token);
                    reports.lock().unwrap_or_else(std::sync::PoisonError::into_inner)[i] = Some(r);
                }
            });
        }
    });
    let reports = reports.into_inner().unwrap_or_else(std::sync::PoisonError::into_inner);
    print_outcomes(root, opts, &selected, reports)
}

/// A test's report and the work directory it made, if any.
type Ran = (Report, Option<PathBuf>);

/// Prints one line per outcome that is not `PASS` (and the kept work
/// directories), the summary and the clang skips; whether the gate passes.
fn print_outcomes(
    root: &Path,
    opts: &Options,
    selected: &[PathBuf],
    reports: Vec<Option<Ran>>,
) -> bool {
    let require_all = std::env::var_os("SLIP_REQUIRE_ALL").is_some_and(|v| v == "1");
    let mut counts = [0usize; 6];
    let mut clang_skipped = 0;
    let (mut off, mut skipped) = (0usize, 0usize);
    let mut gate = true;
    for (dir, r) in selected.iter().zip(reports) {
        let Some((mut report, work)) = r else { continue };
        // spec/harness.md §1 principle 3: a skip for want of a capability
        // (here the clang oracle's compiler, not `--no-clang`) is an ERROR
        // under SLIP_REQUIRE_ALL=1.
        if require_all && report.clang_skipped && !opts.no_clang && report.outcome == Outcome::Pass
        {
            report.outcome = Outcome::Error(
                "the clang oracle was skipped: no compiler (SLIP_REQUIRE_ALL=1)".into(),
            );
        }
        let id = dir
            .strip_prefix(root.join("tests").join("transform"))
            .unwrap_or(dir)
            .to_string_lossy()
            .into_owned();
        let o = &report.outcome;
        counts[match o {
            Outcome::Pass => 0,
            Outcome::Fail(_) => 1,
            Outcome::XFail => 2,
            Outcome::XPass => 3,
            Outcome::Error(_) => 4,
            Outcome::Killed(_) => 5,
        }] += 1;
        clang_skipped += usize::from(report.clang_skipped);
        gate &= !o.fails_gate();
        let message = match o {
            Outcome::Pass => None,
            Outcome::Fail(m) | Outcome::Error(m) | Outcome::Killed(m) => Some(m.clone()),
            Outcome::XFail | Outcome::XPass => Some(format!(
                "xfail: {}",
                parse_test(dir).ok().and_then(|t| t.xfail).unwrap_or_default()
            )),
        };
        if let Some(m) = message {
            println!("{} {id}: {m}", o.tag());
        }
        for n in &report.notes {
            if n.off {
                off += 1;
            } else {
                skipped += 1;
            }
            if opts.verbose {
                let what = if n.off { "off" } else { "skipped" };
                println!("  {id}: oracle {} {what}: {}", n.oracle.name(), n.reason);
            }
        }
        if let Some(w) = work {
            let keep = !matches!(o, Outcome::Pass | Outcome::XFail);
            if keep {
                println!("  kept {}", w.display());
            } else if let Err(e) = tree::remove(&w) {
                println!("  cannot remove {}: {e}", w.display());
            }
        }
    }
    let names = ["passed", "failed", "xfail", "xpass", "error", "killed"];
    let mut parts = Vec::new();
    for (k, (n, name)) in counts.iter().zip(names).enumerate() {
        if k < 2 || *n > 0 {
            parts.push(format!("{n} {name}"));
        }
    }
    println!("transform: {}", parts.join(", "));
    if off + skipped > 0 {
        println!("oracles: {off} off, {skipped} skipped (--verbose names them)");
    }
    if clang_skipped > 0 {
        let why = if opts.no_clang {
            "--no-clang"
        } else {
            "no compiler: $SLIP_CLANG, clang++-19 and clang++ not found"
        };
        println!("clang oracle skipped: {clang_skipped} ({why})");
        gate &= opts.no_clang;
    }
    gate
}

/// One test: parse it, work out its work directory, run it.
fn one(
    dir: &Path,
    slip: &Path,
    target: &Path,
    clang: Option<&Path>,
    bless: bool,
) -> (Report, Option<PathBuf>) {
    let test = match parse_test(dir) {
        Ok(t) => t,
        Err(e) => {
            let report =
                Report { outcome: Outcome::Error(e), clang_skipped: false, notes: Vec::new() };
            return (report, None);
        }
    };
    let work = target.join("transform").join(&test.area).join(&test.name);
    let report = run::run_test(&test, slip, &work, clang, bless);
    // A work directory refused by the runner was never made: keep nothing.
    let made = tree::check_work_dir(&work, &test.area, &test.name, &test.dir).is_ok();
    (report, made.then_some(work))
}

/// `cargo build -p slip_cli` under the jobserver; the path of `slip`.
fn build_slip(root: &Path) -> Result<PathBuf, String> {
    let cargo = crate::ci::cargo();
    let status = std::process::Command::new(&cargo)
        .args(["build", "-p", "slip_cli"])
        .current_dir(root)
        .status()
        .map_err(|e| format!("cannot run `{} build -p slip_cli`: {e}", cargo.to_string_lossy()))?;
    if !status.success() {
        return Err("`cargo build -p slip_cli` failed".into());
    }
    Ok(target_dir(root).join("debug").join("slip"))
}
