//! The `slip` command-line tool (`spec/exec.md` §8): `plan`, `parse`,
//! `dump`, `resource`, `verify`, `declare`, `units`, `map`, `reduce`, `check`,
//! `apply`, `rename`,
//! `--version`, and the hidden worker. Work package WP-012 (`resource`: WP-082; the verify task:
//! WP-052; `declare`'s resolution: WP-055; the three M1 commands: WP-053; `reduce` and `apply`: WP-075; `map`, `check` and `rename`: WP-076);
//! the interface is §8.6.
//!
//! The binary is a thin `main` over [`run`]; [`args`] is pure, so the
//! command line is tested without starting a process.

#![forbid(unsafe_code)]

pub mod args;
pub mod commands;
pub mod declare;
pub mod image;
pub mod locate;
pub mod negatives;
pub mod transform;
pub mod units;
pub mod verify_cmd;
pub mod verify_job;
pub mod worker;

use std::ffi::OsString;
use std::io::Write;
use std::panic::{self, AssertUnwindSafe};
use std::process::ExitCode;
use std::sync::{Mutex, PoisonError};

use args::Invocation;
use slip_exec::{jobserver, process};

/// The exit statuses of §8.5.
pub mod status {
    pub const OK: u8 = 0;
    pub const FINDINGS: u8 = 1;
    pub const USAGE: u8 = 2;
    pub const INPUT: u8 = 3;
    pub const INCOMPLETE: u8 = 4;
    pub const INTERNAL: u8 = 70;
}

/// `slip <version> (kiln <version>)`: `--version`'s line and the tool the
/// worker announces in `Ready`.
pub fn version_line() -> String {
    format!("slip {} (kiln {})", env!("CARGO_PKG_VERSION"), slip_kiln::kiln_version())
}

/// Runs the command line `args` (after `argv[0]`) and returns the exit
/// status (§8.5). `main` passes it through `signals::finish`.
pub fn run(args: &[OsString], out: &mut dyn Write, err: &mut dyn Write) -> ExitCode {
    let invocation = match args::parse(args) {
        Ok(invocation) => invocation,
        Err(e) => {
            let _ = writeln!(err, "slip: {e}");
            let _ = write!(err, "{}", args::USAGE);
            return ExitCode::from(status::USAGE);
        }
    };
    // The worker keeps Rust's own panic report: it goes to the attempt's
    // log, and the coordinator records the observer as failed.
    if let Invocation::Worker(w) = &invocation {
        return worker::main(w);
    }
    install_panic_hook();
    match panic::catch_unwind(AssertUnwindSafe(|| commands::dispatch(invocation, out, err))) {
        Ok(code) => code,
        Err(payload) => {
            process::kill_live_children_now();
            jobserver::release_all();
            let line = PANIC_LINE
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take()
                .unwrap_or_else(|| format!("{} (unknown location)", payload_text(&*payload)));
            let _ = writeln!(err, "slip: internal error: {line}");
            ExitCode::from(status::INTERNAL)
        }
    }
}

/// The first panic of the process, as `<message> (<file>:<line>)`.
static PANIC_LINE: Mutex<Option<String>> = Mutex::new(None);

/// Replaces Rust's panic report with nothing: the first panic's message and
/// location are kept for [`run`], which prints the one line of §8.5 once
/// the children are killed and the tokens returned. Installed once; a later
/// `signals::install` wraps it.
fn install_panic_hook() {
    static INSTALLED: std::sync::Once = std::sync::Once::new();
    INSTALLED.call_once(|| {
        panic::set_hook(Box::new(|info| {
            let location = info.location().map_or_else(
                || "unknown location".to_owned(),
                |l| format!("{}:{}", l.file(), l.line()),
            );
            let line = format!("{} ({location})", payload_text(info.payload()));
            let mut slot = PANIC_LINE.lock().unwrap_or_else(PoisonError::into_inner);
            if slot.is_none() {
                *slot = Some(line);
            }
        }));
    });
}

fn payload_text(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic with a non-text payload".to_owned())
}
