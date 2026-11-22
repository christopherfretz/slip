//! `cargo xtask ci [--quick] [--step <name>]… [--keep-going]`: the gate
//! (`spec/harness.md` §2).
//!
//! Steps run in gate order ([`STEPS`]). `--quick` keeps the quick steps;
//! `--step` keeps the named ones (a name may be given more than once; the
//! order on the command line does not matter). Both together keep the named
//! steps and require each to be quick: `--quick --step kiln` is a usage
//! error rather than a gate that silently runs nothing. A failing step
//! stops the gate unless `--keep-going`.
//!
//! Output, on the writer given to [`run`] (the commands' own output goes
//! to the inherited stdout and stderr):
//!
//! ```text
//! ==> <name>
//! <name>: ok (<seconds>s)        seconds with one decimal
//! <name>: FAILED
//! ci: ok | ci: FAILED (<step>, <step>)
//! ```
//!
//! Exit status: 0 if every step that ran passed, 1 if one failed, 2 for a
//! usage error. Cargo is the `CARGO` environment variable (cargo sets it
//! for `cargo run`), else `cargo` on `PATH`; each cargo command runs in
//! the repository root.

use crate::lint;
use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::path::Path;
use std::process::{Command, ExitCode};
use std::time::Instant;

/// The crates that link Kiln, which the `kiln` step checks (the same list
/// the `kiln-boundary` lint enforces).
pub const KILN_CRATES: &[&str] = lint::KILN_LINKING;

/// One step of the gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step {
    pub name: &'static str,
    /// Part of `--quick`.
    pub quick: bool,
}

/// The steps, in gate order.
pub const STEPS: &[Step] = &[
    Step { name: "lint", quick: true },
    Step { name: "fmt", quick: true },
    Step { name: "clippy", quick: true },
    Step { name: "test", quick: true },
    Step { name: "kiln", quick: false },
    Step { name: "transform", quick: false },
];

const USAGE: &str = "usage: cargo xtask ci [--quick] [--step <name>]… [--keep-going]\n\
                     steps: lint, fmt, clippy, test, kiln, transform";

/// A parsed command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// The steps to run, in gate order.
    pub steps: Vec<&'static str>,
    pub keep_going: bool,
}

/// Parses the arguments after `ci`; `Err` is the usage error's message.
pub fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut quick = false;
    let mut keep_going = false;
    let mut named: Vec<&str> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--quick" => quick = true,
            "--keep-going" => keep_going = true,
            "--step" => {
                let Some(name) = it.next() else { return Err("`--step` needs a name".into()) };
                named.push(name);
            }
            other => return Err(format!("unexpected argument `{other}`")),
        }
    }
    for name in &named {
        let Some(step) = STEPS.iter().find(|s| s.name == *name) else {
            return Err(format!("unknown step `{name}`"));
        };
        if quick && !step.quick {
            return Err(format!("step `{name}` is not part of `--quick`"));
        }
    }
    let steps = STEPS
        .iter()
        .filter(|s| if named.is_empty() { !quick || s.quick } else { named.contains(&s.name) })
        .map(|s| s.name)
        .collect();
    Ok(Options { steps, keep_going })
}

/// The cargo to run: `$CARGO`, else `cargo`.
pub fn cargo() -> OsString {
    std::env::var_os("CARGO").filter(|c| !c.is_empty()).unwrap_or_else(|| "cargo".into())
}

/// Runs the gate over the repository at `root` with `cargo`, writing the
/// gate's lines to `out`; returns whether every step passed.
pub fn run(opts: &Options, root: &Path, cargo: &OsStr, out: &mut dyn Write) -> bool {
    let mut failed: Vec<&str> = Vec::new();
    for &name in &opts.steps {
        let _ = writeln!(out, "==> {name}");
        let _ = out.flush();
        let start = Instant::now();
        let ok = run_step(name, root, cargo, out);
        if ok {
            let _ = writeln!(out, "{name}: ok ({:.1}s)", start.elapsed().as_secs_f64());
        } else {
            let _ = writeln!(out, "{name}: FAILED");
            failed.push(name);
        }
        let _ = out.flush();
        if !ok && !opts.keep_going {
            break;
        }
    }
    if failed.is_empty() {
        let _ = writeln!(out, "ci: ok");
    } else {
        let _ = writeln!(out, "ci: FAILED ({})", failed.join(", "));
    }
    let _ = out.flush();
    failed.is_empty()
}

fn run_step(name: &str, root: &Path, cargo: &OsStr, out: &mut dyn Write) -> bool {
    let clippy_tail = ["--all-targets", "--", "-D", "warnings"];
    let kiln_packages: Vec<&str> = KILN_CRATES.iter().flat_map(|c| ["-p", c]).collect();
    match name {
        "lint" => match lint::run(root) {
            Ok(v) => {
                for v in &v {
                    let _ = writeln!(out, "{v}");
                }
                v.is_empty()
            }
            Err(e) => {
                let _ = writeln!(out, "lint: {e}");
                false
            }
        },
        "fmt" => cargo_ok(cargo, root, &["fmt", "--all", "--check"], out),
        // Without `-p`, cargo uses the workspace's default members: the
        // crates that do not link Kiln.
        "clippy" => cargo_ok(cargo, root, &[&["clippy"][..], &clippy_tail].concat(), out),
        "test" => cargo_ok(cargo, root, &["test"], out),
        "kiln" => {
            let clippy = [&["clippy"][..], &kiln_packages, &clippy_tail].concat();
            let test = [&["test"][..], &kiln_packages].concat();
            cargo_ok(cargo, root, &clippy, out) && cargo_ok(cargo, root, &test, out)
        }
        "transform" => {
            let _ = writeln!(out, "transform: no transform tests yet (spec/harness.md §6)");
            true
        }
        _ => unreachable!("parse_args admits only the steps of STEPS, not `{name}`"),
    }
}

fn cargo_ok(cargo: &OsStr, root: &Path, args: &[&str], out: &mut dyn Write) -> bool {
    let _ = out.flush();
    match Command::new(cargo).args(args).current_dir(root).status() {
        Ok(status) => status.success(),
        Err(e) => {
            let _ =
                writeln!(out, "cannot run `{} {}`: {e}", cargo.to_string_lossy(), args.join(" "));
            false
        }
    }
}

/// `cargo xtask ci …`.
pub fn main(root: &Path, args: &[String]) -> ExitCode {
    let opts = match parse_args(args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("xtask ci: {e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let mut stdout = std::io::stdout();
    if run(&opts, root, &cargo(), &mut stdout) { ExitCode::SUCCESS } else { ExitCode::from(1) }
}
