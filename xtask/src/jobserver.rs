//! `cargo xtask jobserver status|refill` (`spec/harness.md` §5,
//! `spec/landed/harness/WP-013.md`): look at, or top up, the host's make
//! jobserver pool without blocking, through `slip_exec::jobserver`'s
//! inspection functions (`spec/exec.md` §3 "Inspection").
//!
//! ```text
//! cargo xtask jobserver status [--fifo <path>] [--jobs <n>]
//! cargo xtask jobserver refill [--fifo <path>] [--jobs <n>]
//! ```
//!
//! The fifo is `--fifo`, else the `fifo:` path of `MAKEFLAGS`, else
//! `$SLIP_JOBSERVER_FIFO`, else `/run/slip/jobserver.fifo`; `<n>` is
//! `--jobs`, else `$SLIP_JOBS`, else 8. The environment is a parameter so
//! the tests never read the host's own.

use slip_exec::jobserver;
use std::ffi::OsString;
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "\
usage: cargo xtask jobserver status [--fifo <path>] [--jobs <n>]
       cargo xtask jobserver refill [--fifo <path>] [--jobs <n>]

status prints how many of the <n> tokens are idle in the pool; it changes
nothing. refill tops the pool up to <n> idle tokens. refill is only correct
while no client holds a token: a token in use comes back on top of the
refill, and this command cannot know whether one is in use.

The fifo is --fifo, else MAKEFLAGS' --jobserver-auth=fifo:<path>, else
$SLIP_JOBSERVER_FIFO, else /run/slip/jobserver.fifo. <n> is --jobs, else
$SLIP_JOBS, else 8. Neither command blocks.";

/// Why a command failed: a usage error (exit status 2) or any other
/// error (exit status 1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    Usage(String),
    Error(String),
}

/// What to inspect and how: the parsed command line with the environment
/// applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// `true` for `refill`, `false` for `status`.
    pub refill: bool,
    pub fifo: PathBuf,
    pub jobs: usize,
}

/// `cargo xtask jobserver …`: [`run`], its line on stdout, an error as
/// `xtask jobserver: <message>` on stderr.
pub fn main(args: &[String], env: &dyn Fn(&str) -> Option<OsString>) -> ExitCode {
    match run(args, env) {
        Ok(line) => {
            println!("{line}");
            ExitCode::SUCCESS
        }
        Err(Failure::Usage(e)) => {
            eprintln!("xtask jobserver: {e}\n{USAGE}");
            ExitCode::from(2)
        }
        Err(Failure::Error(e)) => {
            eprintln!("xtask jobserver: {e}");
            ExitCode::from(1)
        }
    }
}

/// Parses `args` (after `jobserver`), resolves the fifo and `<n>` from
/// `env`, and runs the command: the line it prints, without the newline.
pub fn run(args: &[String], env: &dyn Fn(&str) -> Option<OsString>) -> Result<String, Failure> {
    let request = parse(args, env)?;
    inspect(&request).map_err(Failure::Error)
}

/// The command line `args` (after `jobserver`) with `env` applied.
pub fn parse(args: &[String], env: &dyn Fn(&str) -> Option<OsString>) -> Result<Request, Failure> {
    let usage = |s: String| Failure::Usage(s);
    let refill = match args.first().map(String::as_str) {
        Some("status") => false,
        Some("refill") => true,
        Some(other) => return Err(usage(format!("unknown command `{other}`"))),
        None => return Err(usage("missing command".to_owned())),
    };
    let mut fifo: Option<PathBuf> = None;
    let mut jobs: Option<usize> = None;
    let mut rest = args[1..].iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--fifo" | "--jobs" => {
                let Some(value) = rest.next() else {
                    return Err(usage(format!("`{arg}` needs a value")));
                };
                if arg == "--fifo" {
                    if fifo.is_some() {
                        return Err(usage("`--fifo` given twice".to_owned()));
                    }
                    if value.is_empty() {
                        return Err(usage("`--fifo` needs a non-empty path".to_owned()));
                    }
                    fifo = Some(PathBuf::from(value));
                } else {
                    if jobs.is_some() {
                        return Err(usage("`--jobs` given twice".to_owned()));
                    }
                    jobs = Some(positive(value).map_err(|e| usage(format!("--jobs: {e}")))?);
                }
            }
            other => return Err(usage(format!("unknown option `{other}`"))),
        }
    }
    // A MAKEFLAGS that is not UTF-8 is ignored, as
    // `slip_exec::jobserver::default_fifo` ignores it.
    let fifo = fifo.unwrap_or_else(|| {
        let makeflags = env("MAKEFLAGS").and_then(|m| m.into_string().ok());
        jobserver::resolve_fifo(makeflags.as_deref(), env("SLIP_JOBSERVER_FIFO").map(PathBuf::from))
    });
    let jobs = match jobs {
        Some(n) => n,
        None => match env("SLIP_JOBS") {
            None => jobserver::DEFAULT_POOL_SIZE,
            Some(v) => {
                let v = v.into_string().map_err(|v| {
                    Failure::Error(format!("SLIP_JOBS `{}`: not UTF-8", v.display()))
                })?;
                positive(&v).map_err(|e| Failure::Error(format!("SLIP_JOBS: {e}")))?
            }
        },
    };
    Ok(Request { refill, fifo, jobs })
}

/// A positive decimal integer: ASCII digits only (no sign, no spaces), not
/// zero, fitting `usize`.
fn positive(text: &str) -> Result<usize, String> {
    let bad = || format!("`{text}` is not a positive integer");
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(bad());
    }
    match text.parse::<usize>() {
        Ok(0) | Err(_) => Err(bad()),
        Ok(n) => Ok(n),
    }
}

/// Runs a parsed request against its fifo.
pub fn inspect(request: &Request) -> Result<String, String> {
    let fifo = &request.fifo;
    check_fifo(fifo)?;
    let shown = fifo.display();
    if request.refill {
        let r = jobserver::refill(fifo, request.jobs)
            .map_err(|e| format!("{shown}: cannot refill: {e}"))?;
        Ok(format!("jobserver: {shown}: {} idle, {} added", r.idle, r.added))
    } else {
        let idle =
            jobserver::status(fifo).map_err(|e| format!("{shown}: cannot read the pool: {e}"))?;
        Ok(format!("jobserver: {shown}: {idle} of {} tokens idle", request.jobs))
    }
}

/// The fifo exists and is a fifo (symbolic links followed), with a message
/// for each way it is not.
fn check_fifo(fifo: &Path) -> Result<(), String> {
    let shown = fifo.display();
    match std::fs::metadata(fifo) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Err(format!("{shown}: does not exist"))
        }
        Err(e) => Err(format!("{shown}: cannot stat: {e}")),
        Ok(m) if !m.file_type().is_fifo() => Err(format!("{shown}: is not a fifo")),
        Ok(_) => Ok(()),
    }
}
