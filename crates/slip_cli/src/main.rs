//! The `slip` command-line tool: [`slip_cli::run`] over the process's
//! arguments and standard streams, through `signals::finish`.

#![forbid(unsafe_code)]

use std::ffi::OsString;
use std::process::ExitCode;

use slip_exec::signals;

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    // Standard error is not locked for the run: the signal path, the
    // jobserver and the pool write warnings to it from other threads, and
    // a lock held by this thread would block them.
    let code = slip_cli::run(&args, &mut std::io::stdout().lock(), &mut std::io::stderr());
    signals::finish(code)
}
