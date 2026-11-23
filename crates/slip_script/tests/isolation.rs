//! Isolation and determinism of runs (`spec/api.md` §2.4 rule 3, §2.7;
//! `docs/wp/WP-091.md`, "Tests"), over the transforms of the script tests
//! under `tests/api/`.
//!
//! Every script test whose expectation is a completed run (not an
//! `xfail`) takes part. The directories that share a command line (the
//! groups of `spec/landed/api/WP-090.md`: one compile) run in one session,
//! [`N`] times each in two orders, the second interleaved with a run that
//! fails (the step budget of one step, §2.5): every completed run of a
//! directory renders the same output, its counters `api.steps`,
//! `api.host_calls` and `api.peak_bytes` included, and every failed run
//! renders the same failure. So no run sees what an earlier one did
//! (whether a template was instantiated, a host value it made, a failure),
//! through its output or its step count.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use slip_script::driver::Budgets;
use slip_script::script_test::{self, Ran, Script};

/// How many times each directory runs in each of the two orders.
const N: usize = 3;

/// The evaluator's counters (§7.6 rule 6), rendered for every run.
const COUNTERS: [&str; 3] = ["api.steps", "api.host_calls", "api.peak_bytes"];

/// The marker of a completed run's rendering (`spec/landed/api/WP-090.md`).
const COMPLETED: &[u8] = b"transform <id>\n";

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The scripts of the suite that complete, grouped by command line.
fn groups(root: &Path) -> Vec<Vec<Script>> {
    let mut keyed = Vec::new();
    for (i, dir) in script_test::discover(root).unwrap().into_iter().enumerate() {
        let script = script_test::load_script(&dir).unwrap();
        let expect = std::fs::read(dir.join(script_test::EXPECT)).unwrap();
        if script.xfail.is_some() || !expect.starts_with(COMPLETED) {
            continue;
        }
        let key = script_test::group_key(&script).unwrap();
        keyed.push((i, key, script));
    }
    script_test::groups(keyed)
        .into_iter()
        .map(|g| g.into_iter().map(|(_, s)| s).collect())
        .collect()
}

/// `script` with the evaluator's counters rendered as well as its own.
fn counted(script: &Script) -> Script {
    let mut s = script.clone();
    for c in COUNTERS {
        if !s.counters.iter().any(|x| x == c) {
            s.counters.push(c.to_owned());
        }
    }
    s
}

/// What one group's session gave that it should not have: an empty list
/// when every run of a directory rendered alike.
fn check_group(group: &[Script], resource: &Path, root: &Path) -> Vec<String> {
    let scripts: Vec<Script> = group.iter().map(counted).collect();
    let mut failing = scripts[0].clone();
    failing.budgets = Budgets { steps: 1, ..failing.budgets };
    // `None` is the failing run.
    let mut order: Vec<Option<usize>> = Vec::new();
    for _ in 0..N {
        order.extend((0..scripts.len()).map(Some));
    }
    for _ in 0..N {
        for i in (0..scripts.len()).rev() {
            order.push(None);
            order.push(Some(i));
        }
    }
    order.push(None);
    let refs: Vec<&Script> = order.iter().map(|o| o.map_or(&failing, |i| &scripts[i])).collect();
    let ran = script_test::run_group(&refs, resource);
    let mut problems = Vec::new();
    let mut first: BTreeMap<Option<usize>, String> = BTreeMap::new();
    for (k, (o, r)) in order.iter().zip(ran).enumerate() {
        let name = o.map_or_else(
            || format!("{} (failing)", script_test::case_name(root, &failing.dir)),
            |i| script_test::case_name(root, &scripts[i].dir),
        );
        let text = match r {
            Ran::Rendered(t) => t,
            other => {
                problems.push(format!("{name}, run {k}: no rendering: {other:?}"));
                continue;
            }
        };
        let completed = text.as_bytes().starts_with(COMPLETED);
        if completed != o.is_some() {
            problems.push(format!("{name}, run {k}: unexpected outcome:\n{text}"));
            continue;
        }
        match first.get(o) {
            None => {
                first.insert(*o, text);
            }
            Some(f) if *f != text => problems.push(format!(
                "{name}, run {k} differs from its first run:\n--- first\n{f}--- run {k}\n{text}"
            )),
            Some(_) => {}
        }
    }
    problems
}

/// §2.4 rule 3, §2.7: every directory of the suite that completes, run
/// `N` times in two orders in its group's session, with failing runs in
/// between: equal outputs and equal step counts.
#[test]
fn s2_4_rule3_s2_7_runs_are_isolated_and_deterministic() {
    let root = crate_dir().join("tests/api");
    let resource = std::fs::canonicalize(crate_dir().join("../../resource")).unwrap();
    let groups = groups(&root);
    assert!(groups.len() > 1, "the suite has groups");
    let threads = std::thread::available_parallelism().map_or(1, std::num::NonZero::get).min(4);
    let pool = slip_exec::jobserver::Pool::from_env();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let problems = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|s| {
        for _ in 0..threads.min(groups.len()) {
            s.spawn(|| {
                loop {
                    let n = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(group) = groups.get(n) else { break };
                    let held = pool.acquire();
                    let found = check_group(group, &resource, &root);
                    drop(held);
                    problems
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .extend(found);
                }
            });
        }
    });
    let problems = problems.into_inner().unwrap_or_else(std::sync::PoisonError::into_inner);
    assert!(problems.is_empty(), "{} problem(s):\n{}", problems.len(), problems.join("\n"));
}
