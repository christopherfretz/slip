//! The `slip` command-line tool.

#![forbid(unsafe_code)]

use std::process::ExitCode;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if matches!(args.first().map(String::as_str), Some("--version" | "-V")) {
        println!("slip {VERSION} (kiln {})", slip_kiln::kiln_version());
        ExitCode::SUCCESS
    } else {
        eprintln!("slip {VERSION}: no commands are implemented yet (see docs/design/)");
        ExitCode::from(2)
    }
}
