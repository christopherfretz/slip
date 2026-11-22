//! Developer tasks for Slip: the test, lint, benchmark and corpus harness
//! (`spec/harness.md`). Run as `cargo xtask <command>`.

#![forbid(unsafe_code)]

use std::process::ExitCode;

const USAGE: &str = "\
usage: cargo xtask <command>

No commands are implemented yet; spec/harness.md defines them.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if matches!(args.first().map(String::as_str), Some("help" | "--help" | "-h")) {
        println!("{USAGE}");
        ExitCode::SUCCESS
    } else {
        eprintln!("{USAGE}");
        ExitCode::from(2)
    }
}
