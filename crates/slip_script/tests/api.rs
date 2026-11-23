//! The script tests (`spec/api.md` §11, `docs/wp/WP-090.md`): every
//! directory under `tests/api/` that has a `transform.cpp` is one case,
//! named by its path there. No test framework: this binary's `main` runs
//! them (`harness = false`), one group of directories that share a
//! compiled transform per thread, the groups in parallel under the
//! jobserver of `MAKEFLAGS`.
//!
//! ```text
//! cargo test -p slip_script --test api [-- [<filter>…] [--exact] [--list] [--test-threads <n>]]
//! SLIP_BLESS=1 cargo test -p slip_script --test api    # writes expect.txt; reviewed like code
//! ```
//!
//! Each case prints `<OUTCOME> <name>` (with what was wrong, for a failure
//! or an error), then the count line. The exit status is 1 if any case is
//! `FAIL`, `XPASS` or `ERROR`, or the suite cannot run; 2 for a usage
//! error. `cargo xtask ci` never blesses: it fails when `SLIP_BLESS` is
//! set.

#![forbid(unsafe_code)]

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use slip_script::script_test::{self, Case, SuiteOptions, Verdict};

const USAGE: &str = "usage: api [<filter>…] [--exact] [--list] [--test-threads <n>] \
                     (libtest's --nocapture, --show-output, --quiet, --color, --format and \
                     --include-ignored are accepted and change nothing; --ignored runs nothing)";

/// The command line, as far as this runner reads it.
#[derive(Debug, Default)]
struct Args {
    filters: Vec<String>,
    exact: bool,
    list: bool,
    ignored: bool,
    threads: Option<usize>,
}

fn parse(args: &[String]) -> Result<Args, String> {
    let mut a = Args::default();
    let mut it = args.iter();
    let count = |v: &str| {
        v.parse::<usize>()
            .ok()
            .filter(|n| *n > 0)
            .ok_or_else(|| format!("--test-threads {v}: not a positive number"))
    };
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--exact" => a.exact = true,
            "--list" => a.list = true,
            "--ignored" => a.ignored = true,
            "--nocapture" | "--show-output" | "--quiet" | "-q" | "--include-ignored" => {}
            "--test-threads" => {
                let v = it.next().ok_or("--test-threads needs a number")?;
                a.threads = Some(count(v)?);
            }
            "--color" | "--format" => {
                it.next().ok_or_else(|| format!("{arg} needs a value"))?;
            }
            other => {
                if let Some(v) = other.strip_prefix("--test-threads=") {
                    a.threads = Some(count(v)?);
                } else if other.starts_with("--color=") || other.starts_with("--format=") {
                } else if other.starts_with('-') {
                    return Err(format!("unknown option `{other}`"));
                } else {
                    a.filters.push(other.to_owned());
                }
            }
        }
    }
    Ok(a)
}

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// `SLIP_TEST_THREADS` if set (a positive number), else the smaller of 4
/// and the available parallelism: each group holds a compiled transform
/// with the standard headers, so memory, not only cores, bounds it.
fn default_threads() -> Result<usize, String> {
    match std::env::var_os("SLIP_TEST_THREADS") {
        Some(v) => {
            v.to_str().and_then(|s| s.parse::<usize>().ok()).filter(|n| *n > 0).ok_or_else(|| {
                format!("SLIP_TEST_THREADS={}: not a positive number", v.to_string_lossy())
            })
        }
        None => Ok(std::thread::available_parallelism().map_or(1, std::num::NonZero::get).min(4)),
    }
}

fn say(text: &str) {
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(text.as_bytes());
    let _ = out.flush();
}

fn line(case: &Case) -> String {
    match &case.verdict {
        Verdict::Pass | Verdict::Xpass | Verdict::Blessed => {
            format!("{} {}\n", case.verdict.name(), case.name)
        }
        Verdict::Xfail(why) => format!("XFAIL {}: {why}\n", case.name),
        Verdict::Fail(why) | Verdict::Error(why) => {
            format!("{} {}: {why}\n", case.verdict.name(), case.name)
        }
    }
}

fn run(args: &Args, root: &Path) -> Result<bool, String> {
    let bless = script_test::bless_from(std::env::var_os("SLIP_BLESS").as_deref())?;
    if args.list {
        for dir in script_test::discover(root)? {
            let name = script_test::case_name(root, &dir);
            if args.filters.is_empty()
                || args
                    .filters
                    .iter()
                    .any(|f| if args.exact { name == *f } else { name.contains(f) })
            {
                say(&format!("{name}: test\n"));
            }
        }
        return Ok(true);
    }
    if args.ignored {
        script_test::discover(root)?;
        say("api: no script test is ignored\n");
        return Ok(true);
    }
    let resource = std::fs::canonicalize(crate_dir().join("../../resource"))
        .map_err(|e| format!("the resource directory: {e}"))?;
    let threads = match args.threads {
        Some(n) => n,
        None => default_threads()?,
    };
    let opts = SuiteOptions { filters: args.filters.clone(), exact: args.exact, threads, bless };
    let pool = slip_exec::jobserver::Pool::from_env();
    let token = || -> Box<dyn std::any::Any> { Box::new(pool.acquire()) };
    let report = |c: &Case| say(&line(c));
    let cases = script_test::run_suite(root, &resource, &opts, &token, &report)?;
    let (count, ok) = script_test::summary(&cases);
    let all = script_test::discover(root)?.len();
    let filtered = if cases.len() < all {
        format!(" ({} of {all} directories selected)", cases.len())
    } else {
        String::new()
    };
    say(&format!("{count}{filtered}\n"));
    Ok(ok)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args = match parse(&args) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("api: {e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match run(&args, &crate_dir().join("tests/api")) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            eprintln!("api: {e}");
            ExitCode::from(1)
        }
    }
}
