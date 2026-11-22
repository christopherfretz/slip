//! Developer tasks for Slip: the test, lint, benchmark and corpus harness
//! (`spec/harness.md`). Run as `cargo xtask <command>`. This file only
//! dispatches; each command is a module of the `xtask` library.

#![forbid(unsafe_code)]

use std::process::ExitCode;

const USAGE: &str = "\
usage: cargo xtask <command>

commands:
  lint                                         the lints of spec/harness.md §3
  wp [--ready]                                 the work packages (§4)
  ci [--quick] [--step <name>]… [--keep-going] the gate (§2)";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = xtask::repo_root();
    let rest = args.get(1..).unwrap_or_default();
    match args.first().map(String::as_str) {
        Some("lint") => xtask::lint::main(&root, rest),
        Some("wp") => xtask::wp::main(&root, rest),
        Some("ci") => xtask::ci::main(&root, rest),
        Some("help" | "--help" | "-h") => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("xtask: unknown command `{other}`\n{USAGE}");
            ExitCode::from(2)
        }
        None => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}
