//! Determinism (`spec/digest.md` §1 rule 3, §14): every source of every
//! pair-test directory, digested twice in this process and once in
//! another, gives the same `Output` each time.
//!
//! The other process is this test binary run again with
//! `SLIP_DIGEST_CHILD=1` and `--exact child --nocapture`: the test `child`
//! then digests both sides of every directory and prints, for each, the
//! directory and the `Debug` form of each side's `Output` between marker
//! lines; the parent compares them with its own. One child serves every
//! directory, so that every source is covered at the cost of one process;
//! it walks while this process does. Both processes walk the directories
//! on the runners' pool (`common::pool_map`); the order of the walks does
//! not matter to what is asserted, and the failures are reported in
//! directory order.
//!
//! The first walk of each source also serves the uniqueness test of keys
//! and display names (`keys_and_names_are_unique`, WP-039), which covers
//! the pending directories as well.

mod common;
#[path = "determinism/keys.rs"]
mod keys;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

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

/// A side as the determinism test compares it: its `Output`'s `Debug`
/// text, or its error.
fn text(r: &Result<common::Digested, String>) -> String {
    match r {
        Ok(d) => format!("{:?}", d.output),
        Err(e) => format!("error: {}", e.trim_end()),
    }
}

/// Both sides of `dir`, each as [`text`].
fn render(dir: &Path) -> Vec<String> {
    [Side::A, Side::B].into_iter().map(|s| text(&common::digest_side(dir, s))).collect()
}

/// The first walk of every directory of the four roots, pending ones
/// included: each side's [`text`] and what [`keys::check`] finds in it.
/// Both tests of this file read it, so a source is parsed once for both.
struct First {
    rel: String,
    pending: bool,
    texts: Vec<String>,
    findings: Vec<keys::Finding>,
}

fn first() -> &'static [First] {
    static FIRST: OnceLock<Vec<First>> = OnceLock::new();
    FIRST.get_or_init(|| {
        let mut dirs = Vec::new();
        for root in ["differ", "equal", "demand", "indeterminate", "resolved"] {
            dirs.extend(common::dirs(root));
        }
        let threads = common::pool_size().unwrap_or_else(|e| panic!("{e}"));
        let walks = common::pool_map(&dirs, threads, |dir| {
            let rel = common::rel(dir);
            let mut f = First {
                pending: common::is_pending(dir),
                texts: Vec::new(),
                findings: Vec::new(),
                rel: rel.clone(),
            };
            for side in [Side::A, Side::B] {
                let r = common::digest_side(dir, side);
                match &r {
                    Ok(d) => f.findings.extend(keys::check(&rel, side, d)),
                    Err(e) => f.findings.push(keys::unparsed(&rel, side, e)),
                }
                f.texts.push(text(&r));
            }
            f
        });
        dirs.iter()
            .zip(walks)
            .map(|(dir, w)| {
                w.unwrap_or_else(|p| First {
                    rel: common::rel(dir),
                    pending: common::is_pending(dir),
                    texts: vec![format!("panicked: {p}")],
                    findings: vec![keys::Finding::Problem(format!(
                        "{}: panicked: {p}",
                        common::rel(dir)
                    ))],
                })
            })
            .collect()
    })
}

#[test]
fn child() {
    if std::env::var_os("SLIP_DIGEST_CHILD").is_none() {
        return;
    }
    let dirs = all_dirs();
    let threads = common::pool_size().unwrap_or_else(|e| panic!("{e}"));
    let renders = common::pool_map(&dirs, threads, |d| render(d));
    for (dir, r) in dirs.iter().zip(renders) {
        let rel = common::rel(dir);
        for out in r.unwrap_or_else(|p| vec![format!("panicked: {p}")]) {
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
    // The child walks while this process does.
    let child = std::process::Command::new(&exe)
        .args(["--exact", "child", "--nocapture", "--test-threads=1"])
        .env("SLIP_DIGEST_CHILD", "1")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("the child runs");
    // The first walk of a directory is the shared one ([`first`]); the
    // second runs on the pool, the directories in any order, the failures
    // in theirs.
    let firsts: BTreeMap<&str, &First> = first().iter().map(|f| (f.rel.as_str(), f)).collect();
    let threads = common::pool_size().unwrap_or_else(|e| panic!("{e}"));
    let seconds = common::pool_map(&dirs, threads, |d| render(d));
    let mut ours = Vec::new();
    for (dir, r) in dirs.iter().zip(seconds) {
        let rel = common::rel(dir);
        let Some(f) = firsts.get(rel.as_str()) else {
            failures.push(format!("{rel}: not walked"));
            continue;
        };
        match r {
            Ok(second) => ours.push((rel, f.texts.clone(), second)),
            Err(p) => failures.push(format!("{rel}: panicked: {p}")),
        }
    }
    let child = child.wait_with_output().expect("the child's output");
    let theirs = parse_child(&String::from_utf8_lossy(&child.stdout));
    for (rel, first, second) in ours {
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

/// WP-039 item 1 and 2: no two units of one output share a key, nor two
/// keys a display name ([`keys`]), over every source of the pair tests,
/// both sides, pending ones included.
#[test]
fn keys_and_names_are_unique() {
    let all = first();
    let problems = keys::problems(all.iter().flat_map(|f| &f.findings));
    eprintln!(
        "keys: {} directories ({} pending), {} sides listed as not digesting",
        all.len(),
        all.iter().filter(|f| f.pending).count(),
        keys::UNPARSED.len()
    );
    assert!(
        all.len() >= keys::MIN_DIRS,
        "the sweep walked {} directories, fewer than {}",
        all.len(),
        keys::MIN_DIRS
    );
    assert!(problems.is_empty(), "{} problems:\n{}", problems.len(), problems.join("\n"));
}
