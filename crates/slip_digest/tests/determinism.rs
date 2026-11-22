//! Determinism (`spec/digest.md` §1 rule 3, §14): every source of every
//! pair-test directory, digested twice in this process and once in
//! another, gives the same `Output` each time.
//!
//! The other process is this test binary run again with
//! `SLIP_DIGEST_CHILD=1` and `--exact child --nocapture`: the test `child`
//! then digests both sides of every directory and prints, for each, the
//! directory and the `Debug` form of each side's `Output` between marker
//! lines; the parent compares them with its own. One child serves every
//! directory, so that every source is covered at the cost of one process.

mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use common::Side;

const BEGIN: &str = "=== slip-digest-output-begin";
const END: &str = "=== slip-digest-output-end";

fn all_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    for root in ["differ", "equal", "demand", "indeterminate", "resolved"] {
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
    if std::env::var_os("SLIP_DIGEST_CHILD").is_none() {
        return;
    }
    for dir in all_dirs() {
        let rel = common::rel(&dir);
        for out in render(&dir) {
            println!("{BEGIN} {rel}\n{out}\n{END}");
        }
    }
}

/// The child's outputs by directory, in the order printed.
fn parse_child(out: &str) -> BTreeMap<String, Vec<String>> {
    let mut theirs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut rest = out;
    while let Some(b) = rest.find(BEGIN) {
        let after = &rest[b + BEGIN.len()..];
        let Some(nl) = after.find('\n') else { break };
        let rel = after[..nl].trim().to_owned();
        let body = &after[nl + 1..];
        let Some(e) = body.find(END) else { break };
        theirs.entry(rel).or_default().push(body[..e].trim_end_matches('\n').to_owned());
        rest = &body[e..];
    }
    theirs
}

#[test]
fn determinism() {
    let dirs = all_dirs();
    let mut failures = Vec::new();
    let exe = std::env::current_exe().expect("the test binary's path");
    let child = std::process::Command::new(&exe)
        .args(["--exact", "child", "--nocapture", "--test-threads=1"])
        .env("SLIP_DIGEST_CHILD", "1")
        .output()
        .expect("the child runs");
    let theirs = parse_child(&String::from_utf8_lossy(&child.stdout));
    for dir in &dirs {
        let rel = common::rel(dir);
        let first = render(dir);
        let second = render(dir);
        if first != second {
            failures.push(format!("{rel}: two walks in one process differ"));
            continue;
        }
        match theirs.get(&rel) {
            Some(t) if *t == first => {}
            Some(_) => failures.push(format!("{rel}: the walk in another process differs")),
            None => failures.push(format!("{rel}: the child printed nothing for it")),
        }
    }
    if theirs.len() != dirs.len() {
        failures.push(format!("the child covered {} of {} directories", theirs.len(), dirs.len()));
    }
    eprintln!("determinism: {} directories, all through another process", dirs.len());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
