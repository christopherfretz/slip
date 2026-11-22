//! Determinism (`spec/digest.md` §1 rule 3, §14): every source of every
//! pair-test directory, digested twice in this process and once in
//! another, gives the same `Output` each time.
//!
//! The other process is this test binary run again with
//! `SLIP_DIGEST_CHILD=<directory>` and `--exact child --nocapture`: the
//! test `child` then digests both sides of that one directory and prints
//! the `Debug` form of each `Output` between marker lines, which the
//! parent compares with its own.

mod common;

use std::path::{Path, PathBuf};

use common::Side;

const BEGIN: &str = "=== slip-digest-output-begin";
const END: &str = "=== slip-digest-output-end";

fn all_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    for root in ["differ", "equal", "demand", "indeterminate"] {
        v.extend(common::dirs(root).into_iter().filter(|d| !common::is_pending(d)));
    }
    v
}

/// Both sides of `dir`, each as its `Output`'s `Debug` text; a side that
/// does not parse is its error.
fn render(dir: &Path) -> Vec<String> {
    [Side::A, Side::B]
        .into_iter()
        .map(|s| match common::digest_side(dir, s) {
            Ok(d) => format!("{:?}", d.output),
            Err(e) => format!("error: {}", e.trim_end()),
        })
        .collect()
}

#[test]
fn child() {
    let Ok(dir) = std::env::var("SLIP_DIGEST_CHILD") else { return };
    for out in render(Path::new(&dir)) {
        println!("{BEGIN}\n{out}\n{END}");
    }
}

#[test]
fn determinism() {
    let dirs = all_dirs();
    let mut failures = Vec::new();
    let exe = std::env::current_exe().expect("the test binary's path");
    for (i, dir) in dirs.iter().enumerate() {
        let first = render(dir);
        let second = render(dir);
        if first != second {
            failures.push(format!("{}: two walks in one process differ", common::rel(dir)));
            continue;
        }
        // One directory in three goes through another process: enough to
        // see a dependence on the process (hash seeds, addresses) while
        // keeping the test under its time budget.
        if i % 3 != 0 {
            continue;
        }
        let out = std::process::Command::new(&exe)
            .args(["--exact", "child", "--nocapture", "--test-threads=1"])
            .env("SLIP_DIGEST_CHILD", dir)
            .output();
        let out = match out {
            Ok(o) => String::from_utf8_lossy(&o.stdout).into_owned(),
            Err(e) => {
                failures.push(format!("{}: cannot run the child: {e}", common::rel(dir)));
                continue;
            }
        };
        let mut theirs = Vec::new();
        let mut rest = out.as_str();
        while let Some(b) = rest.find(BEGIN) {
            let after = &rest[b + BEGIN.len() + 1..];
            let Some(e) = after.find(END) else { break };
            theirs.push(after[..e].trim_end_matches('\n').to_owned());
            rest = &after[e..];
        }
        if theirs != first {
            failures.push(format!("{}: the walk in another process differs", common::rel(dir)));
        }
    }
    eprintln!("determinism: {} directories", dirs.len());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
