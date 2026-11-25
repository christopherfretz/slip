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
  ci [--quick] [--step <name>]… [--keep-going] the gate (§2)
  corpus list|fetch|configure|status|path      the corpora (§8)
  corpus rename <corpus> --from <name> --to <identifier> [--kind <k>] [--owned <path>]…
         [--expect <state>] [--accept] [--build] [-j <n>] [--slip <path>]
                                               a rename over a corpus, checked (§6.5)
  jobserver status|refill [--fifo <p>] [--jobs <n>]
                                               the jobserver pool (§5); refill only
                                               while no client holds a token
  perf summarize <run record>…                 performance records (§7)
  perf record|check <corpus> <run record>      the corpus baseline (§7.3)
  transform [--only <s>]… [--bless] [-j <n>] [--no-clang] [--slip <path>]
                                               the transform tests (§6)
  invariants a|b <corpus> [--slip <path>] [--only <substring>]… [--limit <n>] [-j <n>]
                                               the corpus invariants (§9)
  demo 1 [--slip <path>] [-j <n>] [--case <name>]… [--keep]
                                               Demo 1 (§10)
  api-table [§N[.M]]…                          the rules-to-directories table of the
                                               script tests (docs/wp/WP-091.md)";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = xtask::repo_root();
    let rest = args.get(1..).unwrap_or_default();
    match args.first().map(String::as_str) {
        Some("lint") => xtask::lint::main(&root, rest),
        Some("wp") => xtask::wp::main(&root, rest),
        Some("ci") => xtask::ci::main(&root, rest),
        Some("corpus") => xtask::corpus::main(&root, rest),
        Some("jobserver") => xtask::jobserver::main(rest, &|name| std::env::var_os(name)),
        Some("perf") => xtask::perf::main(&root, rest),
        Some("transform") => xtask::transform::main(&root, rest),
        Some("invariants") => xtask::invariants::main(&root, rest),
        Some("demo") => xtask::demo::main(&root, rest),
        Some("api-table") => xtask::api_table::main(&root, rest),
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
