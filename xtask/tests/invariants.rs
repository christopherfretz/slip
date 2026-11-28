//! `cargo xtask invariants` and `xtask::slip_run` (spec/harness.md §9).
//!
//! The pure parts (`compare_runs`, `check_identity`) are given `Loaded`
//! values built in memory with `slip_model` and reports built as JSON
//! values. The command runs against a fake `slip`: a shell script the test
//! writes, which copies a store the test made with `slip_exec::store` into
//! the `--store` it is given, prints fixed lines, and copies a fixed report
//! to `--report`. The corpus is a fixture repository with one row and
//! stamps that make it configured.

#![cfg(target_os = "linux")]
#![allow(clippy::many_single_char_names)]

mod common;

use common::Fixture;
use slip_exec::store::{Access, Store};
use slip_model::digests::{self, Digests, Group, GroupClass};
use slip_model::json::{self, Value};
use slip_model::{
    Command as Cmd, Configuration, Counter, Digest, End, FileRef, Job, Language, Observer, Origin,
    Outcome, Phase, Plan, Run, RunRecord, Shard, Status,
};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use xtask::corpus::{self, Layout, rows};
use xtask::invariants::{self, A, Env, Failure, Side, check_identity, compare_runs};
use xtask::perf::{Loaded, RunPath};
use xtask::slip_run::{self, Ran};

// ---------------------------------------------------------------- helpers

fn d(n: u8) -> Digest {
    Digest([n; 32])
}

fn observer(main: &str, cost: u64, entry: u64) -> Observer {
    let command = Cmd {
        directory: b"/build".to_vec(),
        argv: vec![b"clang++".to_vec(), b"-c".to_vec(), main.as_bytes().to_vec()],
        env: vec![],
    };
    Observer {
        id: command.observer_id(),
        command,
        language: Language::Cxx,
        main: FileRef { path: main.as_bytes().to_vec(), size: cost, content: d(entry as u8) },
        cost_hint: cost,
        origins: vec![Origin { configuration: 0, entry }],
    }
}

const A: &str = "/src/A.cpp";
const B: &str = "/src/B.cpp";
const C: &str = "/src/C.cpp";
const D: &str = "/src/D.cpp";
const STARTED: u64 = 1_700_000_000_000_000_000;

/// A plan of `A`..`D`, in that plan order (cost descending).
fn plan() -> Plan {
    let observers = [A, B, C, D]
        .iter()
        .enumerate()
        .map(|(i, m)| observer(m, (4 - i as u64) * 100, i as u64))
        .collect();
    Plan {
        tool_slip: "0.0.0".into(),
        tool_kiln: "1.3.0".into(),
        configurations: vec![Configuration {
            name: "default".into(),
            database: FileRef {
                path: b"/build/compile_commands.json".to_vec(),
                size: 1,
                content: d(9),
            },
        }],
        observers,
        excluded: vec![],
    }
}

fn obs(p: &Plan, main: &str) -> Digest {
    p.observers.iter().find(|o| o.main.path == main.as_bytes()).unwrap().id
}

/// A digests section of one `tu` group whose digest is `d(n)`.
fn digests_of(n: u8) -> Digests {
    let groups = vec![Group {
        class: GroupClass::Tu,
        file: vec![],
        count: 1,
        indeterminate: 0,
        digest: d(n),
        units: None,
    }];
    Digests { scheme: digests::SCHEME, tu: digests::tu_digest(&groups), groups }
}

/// A map shard of round 0; `ok` shards carry `digests_of(digest)`.
fn shard(observer: Digest, status: Status, digest: u8) -> Shard {
    Shard {
        observer,
        phase: Phase::Map,
        round: 0,
        outcome: Outcome {
            status,
            errors: u64::from(status == Status::Diagnosed),
            warnings: 0,
            message: if status == Status::Ok { String::new() } else { format!("{status:?}\nmore") },
        },
        inputs: (status != Status::Failed).then(Vec::new),
        counters: if status == Status::Failed {
            vec![]
        } else {
            vec![Counter { name: "inputs.files".into(), value: 1 }]
        },
        digests: (status == Status::Ok).then(|| digests_of(digest)),
        verdict: None,
        edits: None,
        binding: None,
        rule_use: None,
        spellings: None,
        names: (status == Status::Ok).then(slip_model::names::Names::default),
        negatives: None,
        found: None,
        rebuilt_use: None,
        existing_use: None,
    }
}

fn id_of(s: &Shard) -> Digest {
    slip_model::formats::content_id(&s.to_bytes()).unwrap()
}

fn job(observer: Digest, attempt: u64, end: End, shard: Option<Digest>) -> Job {
    Job {
        observer,
        attempt,
        end,
        shard,
        queued_ns: 0,
        wall_ns: 1_000,
        cpu_ns: 1_000,
        peak_rss: 1 << 20,
        estimate: 1 << 20,
    }
}

/// A completed job that wrote `s`.
fn wrote(s: &Shard) -> Job {
    job(s.observer, 1, End::Completed, Some(id_of(s)))
}

fn record(plan: Digest, started: u64, jobs: Vec<Job>) -> RunRecord {
    RunRecord {
        run: Run {
            tool_slip: "0.0.0".into(),
            tool_kiln: "1.3.0".into(),
            plan,
            phase: Phase::Map,
            round: 0,
            started_unix_ns: started,
            wall_ns: 1_000_000,
            cores: 4,
            workers_max: 2,
            memory_budget: 0,
            peak_rss_sum: 0,
        },
        jobs,
    }
}

fn plan_id(p: &Plan) -> Digest {
    slip_model::formats::content_id(&p.to_bytes()).unwrap()
}

/// A `Loaded` built in memory: the record's jobs, and the store's shards
/// (`stored`, each kept for the observers of the record only).
fn loaded(p: &Plan, jobs: Vec<Job>, stored: &[Shard]) -> Loaded {
    let id = plan_id(p);
    let record = record(id, STARTED, jobs);
    let mut shards = BTreeMap::new();
    for j in &record.jobs {
        shards.insert(j.observer, stored.iter().find(|s| s.observer == j.observer).cloned());
    }
    Loaded {
        path: RunPath {
            store: PathBuf::from("/nowhere"),
            plan: id,
            phase: Phase::Map,
            round: 0,
            started_unix_ns: STARTED,
        },
        record,
        plan: p.clone(),
        shards,
    }
}

/// A run in which every observer of `shards` completed once.
fn run_of(p: &Plan, shards: &[Shard]) -> Loaded {
    loaded(p, shards.iter().map(wrote).collect(), shards)
}

fn all(p: &Plan) -> Vec<Digest> {
    p.observers.iter().map(|o| o.id).collect()
}

// ------------------------------------------------------------ §9.4 slip_run

fn executable(f: &Fixture, rel: &str, text: &str, mode: u32) -> PathBuf {
    f.write(rel, text);
    let p = f.path(rel);
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(mode)).unwrap();
    p
}

// §9: `--slip`, else `$CARGO_TARGET_DIR` or `target` in the repository,
// then `release/slip`; missing says to build it; the result is absolute.
#[test]
fn s9_find_slip() {
    let f = Fixture::empty("inv-find");
    let repo = f.root().to_path_buf();
    let given = executable(&f, "bin/slip", "#!/bin/sh\n", 0o755);
    assert_eq!(slip_run::find_slip(&repo, Some(&given), None), Ok(given.clone()));
    // `--slip` wins over the target directory.
    let other = OsString::from(f.path("nowhere"));
    assert_eq!(slip_run::find_slip(&repo, Some(&given), Some(&other)), Ok(given.clone()));
    let e = slip_run::find_slip(&repo, Some(&f.path("bin/none")), None).unwrap_err();
    assert!(e.ends_with("bin/none does not exist"), "{e}");
    let plain = executable(&f, "bin/plain", "x", 0o644);
    let e = slip_run::find_slip(&repo, Some(&plain), None).unwrap_err();
    assert!(e.ends_with("bin/plain is not executable"), "{e}");
    let e = slip_run::find_slip(&repo, Some(&f.path("bin")), None).unwrap_err();
    assert!(e.ends_with("bin is not a file"), "{e}");

    // The default: `target/release/slip` in the repository.
    let e = slip_run::find_slip(&repo, None, None).unwrap_err();
    assert_eq!(
        e,
        format!(
            "{} does not exist: run `cargo build --release -p slip_cli`",
            repo.join("target/release/slip").display()
        )
    );
    let in_repo = executable(&f, "target/release/slip", "#!/bin/sh\n", 0o755);
    assert_eq!(slip_run::find_slip(&repo, None, None), Ok(in_repo.clone()));
    assert_eq!(slip_run::find_slip(&repo, None, Some(&OsString::new())), Ok(in_repo));
    // `$CARGO_TARGET_DIR` replaces `target`.
    let td = OsString::from(f.path("t"));
    let e = slip_run::find_slip(&repo, None, Some(&td)).unwrap_err();
    assert!(e.starts_with(&f.path("t/release/slip").display().to_string()), "{e}");
    assert!(e.ends_with(": run `cargo build --release -p slip_cli`"), "{e}");
    let in_td = executable(&f, "t/release/slip", "#!/bin/sh\n", 0o755);
    assert_eq!(slip_run::find_slip(&repo, None, Some(&td)), Ok(in_td));
}

// §9.4: `run` reads standard output and returns any status; `line` is the
// rest of the first line `<key> …`.
#[test]
fn s9_run_and_line() {
    let f = Fixture::empty("inv-run");
    let p = executable(
        &f,
        "s",
        "#!/bin/sh\n[ \"$1\" = --probe ] && exit 0\necho \"plan $1\"\necho run x y\nexit 3\n",
        0o755,
    );
    common::wait_executable(&p);
    let ran = slip_run::run(&p, &["abc".into()]).unwrap();
    assert_eq!(ran, Ran { status: 3, stdout: "plan abc\nrun x y\n".into() });
    assert_eq!(slip_run::line(&ran.stdout, "plan"), Some("abc"));
    assert_eq!(slip_run::line(&ran.stdout, "run"), Some("x y"));
    assert_eq!(slip_run::line(&ran.stdout, "ru"), None);
    assert_eq!(slip_run::line("plan\nplanned x\nplan  y\n", "plan"), Some(" y"));
    let k = executable(&f, "k", "#!/bin/sh\n[ \"$1\" = --probe ] && exit 0\nkill -9 $$\n", 0o755);
    common::wait_executable(&k);
    let e = slip_run::run(&k, &[]).unwrap_err();
    assert!(e.ends_with("was killed by signal 9"), "{e}");
    let e = slip_run::run(&f.path("none"), &[]).unwrap_err();
    assert!(e.starts_with("cannot run "), "{e}");
}

// §9.1: `remove_tree` removes a tree under the corpus directory, without
// following links, and refuses anything not strictly under it.
#[test]
fn s9_remove_tree() {
    let f = Fixture::empty("inv-rm");
    let corpus = f.path("corpus");
    f.write("corpus/stores/a1/x/y", "1");
    f.write("corpus/stores/a1/z", "2");
    f.write("outside/keep", "3");
    std::os::unix::fs::symlink(f.path("outside"), f.path("corpus/stores/a1/link")).unwrap();
    slip_run::remove_tree(&corpus, &corpus.join("stores/a1")).unwrap();
    assert!(!corpus.join("stores/a1").exists());
    assert!(corpus.join("stores").is_dir());
    assert!(f.path("outside/keep").is_file(), "a link is removed, not followed");
    // Absent: nothing to do; also when its parent is absent.
    slip_run::remove_tree(&corpus, &corpus.join("stores/a1")).unwrap();
    slip_run::remove_tree(&corpus, &corpus.join("none/a1")).unwrap();
    // A file is removed.
    f.write("corpus/stores/b.json", "{}");
    slip_run::remove_tree(&corpus, &corpus.join("stores/b.json")).unwrap();
    assert!(!corpus.join("stores/b.json").exists());

    let refused = |dir: &Path| {
        let e = slip_run::remove_tree(&corpus, dir).unwrap_err();
        assert!(e.starts_with("refusing to remove"), "{e}");
    };
    refused(&f.path("outside"));
    refused(&corpus);
    refused(&corpus.join("../outside"));
    refused(Path::new("corpus/stores"));
    // A link inside the corpus directory leading out of it.
    std::os::unix::fs::symlink(f.path("outside"), f.path("corpus/out")).unwrap();
    refused(&corpus.join("out/keep"));
    assert!(f.path("outside/keep").is_file());
    let e = slip_run::remove_tree(Path::new("rel"), Path::new("rel/x")).unwrap_err();
    assert!(e.starts_with("refusing to remove"), "{e}");
}

// ----------------------------------------------------- §9.1 compare_runs

// §9.1: equal shards in both runs.
#[test]
fn s9_1_equal() {
    let p = plan();
    let shards: Vec<Shard> = [A, B, C]
        .iter()
        .zip([Status::Ok, Status::Diagnosed, Status::Rejected])
        .map(|(m, st)| shard(obs(&p, m), st, 1))
        .collect();
    let (r1, r2) = (run_of(&p, &shards), run_of(&p, &shards));
    let a = compare_runs(&p, &r1, &r2, &all(&p)[..3]);
    assert_eq!(a, A { observers: 3, equal: 3, failed_both: 0, different: vec![] });
    assert!(invariants::a_holds(&a));
}

// §9.1: shards that differ, here only in their digests section, are
// different; the list is in plan order whatever the record's order.
#[test]
fn s9_1_different() {
    let p = plan();
    let (a, b) = (obs(&p, A), obs(&p, B));
    let first = [shard(a, Status::Ok, 1), shard(b, Status::Ok, 1)];
    let second = [shard(a, Status::Ok, 2), shard(b, Status::Ok, 3)];
    let r1 = run_of(&p, &first);
    let r2 = loaded(&p, vec![wrote(&second[1]), wrote(&second[0])], &second);
    let x = compare_runs(&p, &r1, &r2, &[b, a]);
    assert_eq!((x.observers, x.equal, x.failed_both), (2, 0, 0));
    let mains: Vec<&[u8]> = x.different.iter().map(|d| &d.main[..]).collect();
    assert_eq!(mains, [A.as_bytes(), B.as_bytes()]);
    assert_eq!(x.different[0].first, Side::Shard(Status::Ok, id_of(&first[0])));
    assert_eq!(x.different[0].second, Side::Shard(Status::Ok, id_of(&second[0])));
    assert!(!invariants::a_holds(&x));
    // A different status with the same digests: different.
    let diag = [shard(a, Status::Diagnosed, 1)];
    let x = compare_runs(&p, &run_of(&p, &first[..1]), &run_of(&p, &diag), &[a]);
    assert_eq!(x.different.len(), 1);
}

// §9.1: failed in one run only is different; failed in both is counted
// and not compared, whatever the two failed shards hold.
#[test]
fn s9_1_failed() {
    let p = plan();
    let (a, b) = (obs(&p, A), obs(&p, B));
    let mut f2 = shard(b, Status::Failed, 0);
    f2.outcome.message = "another panic".into();
    let r1 = run_of(&p, &[shard(a, Status::Failed, 0), shard(b, Status::Failed, 0)]);
    let r2 = run_of(&p, &[shard(a, Status::Ok, 1), f2.clone()]);
    let x = compare_runs(&p, &r1, &r2, &[a, b]);
    assert_eq!((x.observers, x.equal, x.failed_both), (2, 0, 1));
    assert_eq!(x.different.len(), 1);
    assert_eq!(x.different[0].observer, a);
    assert_eq!(x.different[0].first.status(), Some(Status::Failed));
    // The same failed shard in both: failed in both, not equal.
    let r3 = run_of(&p, std::slice::from_ref(&f2));
    let x = compare_runs(&p, &r3, &r3.clone(), &[b]);
    assert_eq!((x.equal, x.failed_both, x.different.len()), (0, 1, 0));
}

// §9.1: a selected observer missing from a run (no job) is different, and
// one the selection does not keep is not counted.
#[test]
fn s9_1_missing_observer() {
    let p = plan();
    let (a, b, c) = (obs(&p, A), obs(&p, B), obs(&p, C));
    let s = [shard(a, Status::Ok, 1), shard(b, Status::Ok, 1), shard(c, Status::Ok, 1)];
    let r1 = run_of(&p, &s);
    let r2 = run_of(&p, &s[..1]);
    let x = compare_runs(&p, &r1, &r2, &[a, b]);
    assert_eq!((x.observers, x.equal), (2, 1));
    assert_eq!(x.different.len(), 1);
    assert_eq!(x.different[0].second, Side::NoJob);
    // Missing from both is different too.
    let x = compare_runs(&p, &r2, &r2.clone(), &[a, b]);
    assert_eq!((x.equal, x.different.len()), (1, 1));
    assert_eq!((x.different[0].first, x.different[0].second), (Side::NoJob, Side::NoJob));
    // C is not selected; D is selected and in neither run.
    let x = compare_runs(&p, &r1, &r1.clone(), &[a, obs(&p, D)]);
    assert_eq!((x.observers, x.equal, x.different.len()), (2, 1, 1));
}

// §9.1, spec/model.md §9: the observer's result is the shard of its last
// entry; an earlier attempt's shard is not it.
#[test]
fn s9_1_last_entry() {
    let p = plan();
    let a = obs(&p, A);
    let old = shard(a, Status::Failed, 0);
    let new = shard(a, Status::Ok, 1);
    // The store holds the last attempt's shard (a retry replaces it).
    let retried = loaded(
        &p,
        vec![job(a, 1, End::Crashed, Some(id_of(&old))), wrote(&new)],
        std::slice::from_ref(&new),
    );
    let once = run_of(&p, std::slice::from_ref(&new));
    let x = compare_runs(&p, &retried, &once, &[a]);
    assert_eq!((x.equal, x.different.len()), (1, 0));
    // Last entry without a shard: `no shard <end>`, different even in both.
    let crashed =
        loaded(&p, vec![wrote(&new), job(a, 2, End::Crashed, None)], std::slice::from_ref(&new));
    let x = compare_runs(&p, &crashed, &crashed.clone(), &[a]);
    assert_eq!(x.different.len(), 1);
    assert_eq!(x.different[0].first, Side::NoShard(End::Crashed));
    assert_eq!(x.different[0].first.text(), "no shard crashed");
}

// §9.1: the store's shard is read as the run's only if its content id is
// the last entry's shard.
#[test]
fn s9_1_store_shard_not_the_runs() {
    let p = plan();
    let a = obs(&p, A);
    let s = shard(a, Status::Ok, 1);
    let good = run_of(&p, std::slice::from_ref(&s));
    // The record names a shard the store does not have.
    let absent = loaded(&p, vec![wrote(&s)], &[]);
    // The store's shard is another one (the record names d(7)).
    let other = loaded(&p, vec![job(a, 1, End::Completed, Some(d(7)))], std::slice::from_ref(&s));
    for (bad, want) in [(absent, Side::Absent(id_of(&s))), (other, Side::Other(d(7)))] {
        let x = compare_runs(&p, &good, &bad, &[a]);
        assert_eq!(x.different.len(), 1);
        assert_eq!(x.different[0].second, want);
        let x = compare_runs(&p, &bad, &bad.clone(), &[a]);
        assert_eq!(x.different.len(), 1, "{want:?} in both");
    }
}

// §9.1: the side texts and the output.
#[test]
fn s9_1_render() {
    let p = plan();
    let (a, b) = (obs(&p, A), obs(&p, B));
    let r1 = run_of(&p, &[shard(a, Status::Ok, 1), shard(b, Status::Ok, 1)]);
    let r2 = run_of(&p, &[shard(a, Status::Ok, 2)]);
    let x = compare_runs(&p, &r1, &r2, &[a, b]);
    let id = |s: &Shard| id_of(s).to_string();
    let text = invariants::render_a("tiny", &d(5), &x, ["/s/a1/r", "/s/a2/r"], false);
    assert_eq!(
        text,
        format!(
            "invariant a tiny\nplan {}\nobservers 2\nequal 0\nfailed_both 0\ndifferent 2\n  \
             /src/A.cpp: ok {} | ok {}\n  /src/B.cpp: ok {} | no job\nrun /s/a1/r\nrun /s/a2/r\n\
             invariant a: FAILED (2 observer(s) differ)\n",
            d(5),
            id(&shard(a, Status::Ok, 1)),
            id(&shard(a, Status::Ok, 2)),
            id(&shard(b, Status::Ok, 1))
        )
    );
    let ok = compare_runs(&p, &r1, &r1.clone(), &[a]);
    let text = invariants::render_a("tiny", &d(5), &ok, ["x", "y"], true);
    assert!(text.ends_with("run x\nrun y\ninvariant a: holds (selection)\n"), "{text}");
    // Over no observer, the invariant does not hold.
    let none = compare_runs(&p, &r1, &r1.clone(), &[]);
    assert!(!invariants::a_holds(&none));
    let text = invariants::render_a("tiny", &d(5), &none, ["x", "y"], false);
    assert!(text.ends_with("invariant a: FAILED (no observer selected)\n"), "{text}");
    assert_eq!(Side::Absent(d(1)).text(), format!("absent {}", d(1)));
    assert_eq!(Side::Other(d(1)).text(), format!("other {}", d(1)));
}

// §9.1: at most 50 `different` lines.
#[test]
fn s9_1_fifty_lines() {
    let mains: Vec<String> = (0..60).map(|i| format!("/src/{i:02}.cpp")).collect();
    let mut p = plan();
    p.observers =
        mains.iter().enumerate().map(|(i, m)| observer(m, 1000 - i as u64, i as u64)).collect();
    let r = loaded(&p, vec![], &[]);
    let x = compare_runs(&p, &r, &r.clone(), &all(&p));
    assert_eq!(x.different.len(), 60);
    let text = invariants::render_a("tiny", &d(5), &x, ["x", "y"], false);
    assert_eq!(text.lines().filter(|l| l.starts_with("  ")).count(), 50);
    assert!(text.contains("  /src/49.cpp: no job | no job\n"));
    assert!(!text.contains("/src/50.cpp"));
}

// ----------------------------------------------------- §9.2 check_identity

fn obj(members: Vec<(&str, Value)>) -> Value {
    Value::Object(members.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
}

fn int(n: u64) -> Value {
    Value::Int(i128::from(n))
}

fn s(t: &str) -> Value {
    Value::Str(t.to_owned())
}

fn none_declared() -> Digest {
    slip_model::formats::content_id(&slip_model::declared::Declared::none().to_bytes()).unwrap()
}

/// An entry of `not_verified`.
struct Nv<'a> {
    main: &'a str,
    state: &'a str,
    message: &'a str,
    findings: u64,
    indeterminate: u64,
}

fn nv<'a>(main: &'a str, state: &'a str) -> Nv<'a> {
    let message = if state == "mismatch" { "" } else { "why\nsecond line" };
    Nv { main, state, message, findings: 0, indeterminate: 0 }
}

fn mismatch(main: &str, findings: u64, indeterminate: u64) -> Nv<'_> {
    Nv { main, state: "mismatch", message: "", findings, indeterminate }
}

/// A report of `spec/verify.md` §7.1 over `selected` observers of the
/// plan `p`, listing `entries`; the counts are made to agree with them.
fn report(p: &Plan, selected: u64, entries: &[Nv]) -> Value {
    let mut counts: BTreeMap<&str, u64> = BTreeMap::new();
    let mut indeterminate_only = 0;
    for e in entries {
        *counts.entry(e.state).or_default() += 1;
        if e.state == "mismatch" && e.findings == e.indeterminate {
            indeterminate_only += 1;
        }
    }
    let mut observers = vec![("plan", int(p.observers.len() as u64)), ("selected", int(selected))];
    let verified = selected - entries.len() as u64;
    observers.push(("verified", int(verified)));
    for st in ["unchanged", "mismatch", "not_parsed", "unverifiable", "never_parsed", "failed"] {
        observers.push((st, int(counts.get(st).copied().unwrap_or(0))));
    }
    observers.push(("indeterminate_only", int(indeterminate_only)));
    let not_verified = entries
        .iter()
        .map(|e| {
            obj(vec![
                ("observer", s(&obs(p, e.main).to_string())),
                ("main", s(e.main)),
                ("state", s(e.state)),
                ("message", s(e.message)),
                ("findings", int(e.findings)),
                ("indeterminate", int(e.indeterminate)),
                ("listed", int(e.findings.min(64))),
            ])
        })
        .collect();
    let verdict = if entries.is_empty() { "verified" } else { "not_verified" };
    obj(vec![
        ("report", s("slip.verify.v1")),
        ("plan", s(&plan_id(p).to_string())),
        ("round", int(0)),
        ("declared", s(&none_declared().to_string())),
        ("rules", s(&d(3).to_string())),
        ("image", Value::Array(vec![])),
        ("limit", int(64)),
        ("locate", int(16)),
        ("verdict", s(verdict)),
        ("observers", obj(observers)),
        (
            "findings",
            obj(vec![("total", int(0)), ("listed", int(0)), ("groups", Value::Array(vec![]))]),
        ),
        ("not_verified", Value::Array(not_verified)),
        ("exemptions", Value::Array(vec![])),
        ("run", s("/store/runs/x/verify.0.1.run")),
    ])
}

/// Sets `path` (`a.b`) of the report object `v` to `to`.
fn set(v: &mut Value, path: &str, to: Value) {
    let mut cur = v;
    for k in path.split('.') {
        let Value::Object(o) = cur else { panic!("{path}") };
        cur = &mut o.iter_mut().find(|(n, _)| n == k).unwrap_or_else(|| panic!("{path}")).1;
    }
    *cur = to;
}

/// The map run of the identity tests: A, B and C `ok`, D `diagnosed`.
fn map_run(p: &Plan) -> Loaded {
    let s: Vec<Shard> = [(A, Status::Ok), (B, Status::Ok), (C, Status::Ok), (D, Status::Diagnosed)]
        .iter()
        .map(|(m, st)| shard(obs(p, m), *st, 1))
        .collect();
    run_of(p, &s)
}

// §9.2: every map_ok observer verified, D never parsed: holds.
#[test]
fn s9_2_verified() {
    let p = plan();
    let m = map_run(&p);
    let b = check_identity(&m, &report(&p, 4, &[nv(D, "never_parsed")])).unwrap();
    assert_eq!(
        (b.observers, b.map_ok, b.verified, b.never_parsed, b.not_verified.len()),
        (4, 3, 3, 1, 0)
    );
    assert!(b.holds());
    assert_eq!(b.run, b"/store/runs/x/verify.0.1.run");
}

// §9.2: each state of a map_ok observer, counted and listed in plan order.
#[test]
fn s9_2_each_state() {
    let p = plan();
    let m = map_run(&p);
    for (entry, field) in [
        (mismatch(A, 3, 3), "indeterminate_only"),
        (mismatch(A, 3, 1), "different"),
        (nv(A, "not_parsed"), "not_parsed"),
        (nv(A, "unverifiable"), "unverifiable"),
        (nv(A, "failed"), "failed"),
    ] {
        let state = entry.state;
        let b = check_identity(&m, &report(&p, 4, &[entry, nv(D, "never_parsed")])).unwrap();
        let counts = [
            ("indeterminate_only", b.indeterminate_only),
            ("different", b.different),
            ("not_parsed", b.not_parsed),
            ("unverifiable", b.unverifiable),
            ("failed", b.failed),
        ];
        for (name, n) in counts {
            assert_eq!(n, u64::from(name == field), "{field}: {name}");
        }
        assert_eq!((b.map_ok, b.verified, b.never_parsed), (3, 2, 1), "{field}");
        assert!(!b.holds(), "{field}");
        assert_eq!(b.not_verified.len(), 1);
        assert_eq!(b.not_verified[0].state, state);
        assert_eq!(b.not_verified[0].main, A.as_bytes());
        let message = &b.not_verified[0].message;
        if state == "mismatch" {
            assert!(message.starts_with("3 finding(s), "), "{message}");
        } else {
            assert_eq!(message, "why", "the first line");
        }
    }
}

// §9.2: an unverifiable or failed observer whose map shard is not ok is
// not counted against the invariant (it is not map_ok).
#[test]
fn s9_2_not_map_ok() {
    let p = plan();
    let m = map_run(&p);
    for state in ["unverifiable", "failed"] {
        let b = check_identity(&m, &report(&p, 4, &[nv(D, state)])).unwrap();
        assert_eq!((b.map_ok, b.verified, b.never_parsed, b.failed + b.unverifiable), (3, 3, 0, 0));
        assert!(b.holds(), "{state}");
    }
}

// §9.2: the output, the list in plan order and at most 50 lines.
#[test]
fn s9_2_render() {
    let p = plan();
    let m = map_run(&p);
    let b = check_identity(
        &m,
        &report(&p, 4, &[mismatch(A, 2, 1), nv(C, "not_parsed"), nv(D, "never_parsed")]),
    )
    .unwrap();
    let text = invariants::render_b("tiny", &d(5), &b, Path::new("/c/stores/b.json"), false);
    assert_eq!(
        text,
        format!(
            "invariant b tiny\nplan {}\nobservers 4\nmap_ok 3\nverified 1\nindeterminate_only 0\n\
             different 1\nnot_parsed 1\nunverifiable 0\nfailed 0\nnever_parsed 1\n  mismatch: \
             /src/A.cpp: 2 finding(s), 1 indeterminate\n  not_parsed: /src/C.cpp: why\n\
             run /store/runs/x/verify.0.1.run\nreport /c/stores/b.json\n\
             invariant b: FAILED (2 of 3 observer(s) not verified)\n",
            d(5)
        )
    );
    let ok = check_identity(&m, &report(&p, 4, &[nv(D, "never_parsed")])).unwrap();
    let text = invariants::render_b("tiny", &d(5), &ok, Path::new("r"), true);
    assert!(text.ends_with("report r\ninvariant b: holds (selection)\n"), "{text}");

    let mains: Vec<String> = (0..60).map(|i| format!("/src/{i:02}.cpp")).collect();
    let mut p = plan();
    p.observers =
        mains.iter().enumerate().map(|(i, m)| observer(m, 1000 - i as u64, i as u64)).collect();
    let shards: Vec<Shard> = p.observers.iter().map(|o| shard(o.id, Status::Ok, 1)).collect();
    let m = run_of(&p, &shards);
    let entries: Vec<Nv> = mains.iter().map(|m| nv(m, "failed")).collect();
    let b = check_identity(&m, &report(&p, 60, &entries)).unwrap();
    assert_eq!(b.not_verified.len(), 60);
    let text = invariants::render_b("tiny", &d(5), &b, Path::new("r"), false);
    assert_eq!(text.lines().filter(|l| l.starts_with("  ")).count(), 50);
    assert!(text.ends_with("invariant b: FAILED (60 of 60 observer(s) not verified)\n"));
}

// §9.2: no map_ok observer: the invariant does not hold.
#[test]
fn s9_2_no_map_ok() {
    let p = plan();
    let m = run_of(&p, &[shard(obs(&p, D), Status::Diagnosed, 0)]);
    let b = check_identity(&m, &report(&p, 1, &[nv(D, "never_parsed")])).unwrap();
    assert_eq!((b.map_ok, b.verified), (0, 0));
    assert!(!b.holds());
    let text = invariants::render_b("tiny", &d(5), &b, Path::new("r"), false);
    assert!(text.ends_with("invariant b: FAILED (no observer with an ok map shard)\n"));
}

// §9.2: a report that is not of this map run, or does not agree with
// itself, is an error, never a count.
#[test]
fn s9_2_report_errors() {
    let p = plan();
    let m = map_run(&p);
    let good = report(&p, 4, &[mismatch(A, 2, 2), nv(D, "never_parsed")]);
    let err = |v: &Value| check_identity(&m, v).unwrap_err();
    let edit = |path: &str, to: Value| {
        let mut v = good.clone();
        set(&mut v, path, to);
        err(&v)
    };
    let cases: Vec<(String, &str)> = vec![
        (err(&Value::Array(vec![])), "not a JSON object"),
        (edit("report", s("slip.verify.v2")), "not `slip.verify.v1`"),
        (edit("plan", s(&d(1).to_string())), "the report is of plan"),
        (edit("round", int(1)), "of round 1, not 0"),
        (edit("declared", s(&d(1).to_string())), "not nothing"),
        (edit("image", Value::Array(vec![s("x")])), "not the identity"),
        (edit("observers.selected", int(3)), "the map run is not of this selection"),
        (edit("observers.unchanged", int(1)), "unchanged observer(s)"),
        (edit("observers.verified", int(1)), "verified observer(s)"),
        (edit("observers.mismatch", int(2)), "`mismatch` observer(s) and lists 1"),
        (edit("observers.never_parsed", int(0)), "`never_parsed` observer(s) and lists 1"),
        (edit("observers.indeterminate_only", int(0)), "indeterminate_only observer(s)"),
        (edit("observers.failed", s("1")), "is not an unsigned number"),
        (edit("not_verified", s("x")), "`not_verified` is not an array"),
        (edit("run", int(1)), "`run` is not a path"),
    ];
    for (e, want) in &cases {
        assert!(e.contains(want), "{e} / {want}");
    }
    // A member that is missing.
    let Value::Object(mut o) = good.clone() else { unreachable!() };
    o.retain(|(k, _)| k != "observers");
    assert!(err(&Value::Object(o)).contains("no `observers`"));

    // Entries that disagree with the map run.
    let one = |entries: &[Nv]| err(&report(&p, 4, entries));
    for (e, want) in [
        (one(&[nv(D, "not_parsed")]), "its map shard is not `ok`"),
        (one(&[mismatch(D, 1, 0)]), "its map shard is not `ok`"),
        (
            one(&[nv(A, "never_parsed"), nv(D, "never_parsed")]),
            "`never_parsed`, but its map shard is `ok`",
        ),
        (one(&[]), "verified, but its map shard is not `ok`"),
        (one(&[nv(B, "failed"), nv(A, "failed"), nv(D, "never_parsed")]), "out of plan order"),
        (one(&[nv(A, "verified"), nv(D, "never_parsed")]), "in state `verified`"),
        (one(&[nv(A, "lost"), nv(D, "never_parsed")]), "`lost` is not a state"),
        (one(&[mismatch(A, 0, 0), nv(D, "never_parsed")]), "0 finding(s)"),
        (one(&[mismatch(A, 1, 2), nv(D, "never_parsed")]), "1 finding(s), 2 indeterminate"),
    ] {
        assert!(e.contains(want), "{e} / {want}");
    }
    // Repeated, another observer, another main file.
    let mut v = report(&p, 4, &[nv(A, "failed"), nv(A, "failed"), nv(D, "never_parsed")]);
    assert!(err(&v).contains("out of plan order or repeated"));
    v = good.clone();
    set(
        &mut v,
        "not_verified",
        Value::Array(vec![obj(vec![
            ("observer", s(&d(1).to_string())),
            ("main", s(A)),
            ("state", s("failed")),
            ("message", s("")),
            ("findings", int(0)),
            ("indeterminate", int(0)),
            ("listed", int(0)),
        ])]),
    );
    assert!(err(&v).contains("is not of the map run"));
    let mut v = report(&p, 4, &[nv(A, "failed"), nv(D, "never_parsed")]);
    let Value::Object(o) = &mut v else { unreachable!() };
    let nvs = &mut o.iter_mut().find(|(k, _)| k == "not_verified").unwrap().1;
    let Value::Array(a) = nvs else { unreachable!() };
    set(&mut a[0], "main", s(B));
    assert!(err(&v).contains("is not the plan's main file"));
    // A store shard that is not the map run's own is not `ok`.
    let a_id = obs(&p, A);
    let mut stale = map_run(&p);
    stale.record.jobs.iter_mut().find(|j| j.observer == a_id).unwrap().shard = Some(d(7));
    let e = check_identity(&stale, &report(&p, 4, &[nv(D, "never_parsed")])).unwrap_err();
    assert!(e.contains(&format!("{a_id} verified, but its map shard is not `ok`")), "{e}");
}

// §9.2, spec/model.md §10: numbers from 2^53 are decimal strings; paths
// that are not UTF-8 are `{"hex": …}`.
#[test]
fn s9_2_report_encodings() {
    let p = plan();
    let m = map_run(&p);
    let mut v = report(&p, 4, &[nv(D, "never_parsed")]);
    set(&mut v, "run", obj(vec![("hex", s("2f78ff"))]));
    assert_eq!(check_identity(&m, &v).unwrap().run, b"/x\xff");
    for bad in
        [obj(vec![("hex", s("2f78"))]), obj(vec![("hex", s("2f7"))]), obj(vec![("hx", s("ff"))])]
    {
        set(&mut v, "run", bad);
        assert!(check_identity(&m, &v).unwrap_err().contains("`run` is not a path"));
    }
    let mut v = report(&p, 4, &[nv(D, "never_parsed")]);
    for bad in [s("9007199254740991"), s("01"), int(1 << 53), Value::Float(1.0)] {
        set(&mut v, "round", bad);
        assert!(check_identity(&m, &v).unwrap_err().contains("not an unsigned number"));
    }
    set(&mut v, "round", s("9007199254740992"));
    assert!(check_identity(&m, &v).unwrap_err().contains("of round 9007199254740992, not 0"));
}

// ------------------------------------------------------------ the command

const KILN_ROWS: &str = "[tiny_project]\nurl = \"https://example.org/tiny.git\"\ntag = \"v1.0\"\n";
const CORPORA: &str = "[tiny]\nkiln = \"tiny_project\"\ncxx_std = 17\nbuild_type = \"Release\"\n\
     c_compiler = \"cc\"\ncxx_compiler = \"c++\"\nconfigure = []\ngenerate = [\"gen\"]\n";

/// The fake `slip`. `parse` copies the n-th template store `t<n>` into
/// `--store` and prints the plan and the run as `slip parse` does; `plan`
/// prints the plan; `verify` copies `report.json` to `--report`. Each
/// call is logged; the statuses are read from files.
const FAKE: &str = r#"#!/bin/sh
[ "$1" = --probe ] && exit 0
D=@DIR@
cmd=$1
shift
echo "$cmd $*" >> "$D/log"
store=
report=
while [ $# -gt 0 ]; do
  case "$1" in
    --store) store=$2; shift ;;
    --report) report=$2; shift ;;
  esac
  shift
done
case "$cmd" in
  --version) echo "slip 0.0.0 (kiln 1.3.0)"; exit 0 ;;
  plan) echo "plan $(cat "$D/plan")"; echo "observers 4"; exit 0 ;;
  parse)
    n=$(cat "$D/n" 2>/dev/null || echo 0)
    n=$((n + 1))
    echo $n > "$D/n"
    mkdir -p "$store" && cp -R "$D/t$n/." "$store/" || exit 9
    echo "plan $(cat "$D/plan")"
    echo "selected 4"
    echo "wall 0.001"
    echo "run $store/$(cat "$D/t$n.run")"
    exit $(cat "$D/parse.status" 2>/dev/null || echo 1) ;;
  verify)
    cp "$D/report.json" "$report" || exit 9
    echo "verdict verified"
    exit $(cat "$D/verify.status" 2>/dev/null || echo 0) ;;
esac
exit 99
"#;

/// A fixture: a repository with the `tiny` row, a configured corpus under
/// `corpora/`, and the fake `slip` at `bin/slip` with its directory `fake/`.
struct World {
    f: Fixture,
    p: Plan,
}

impl World {
    fn new(tag: &str) -> World {
        let f = Fixture::empty(tag);
        f.write("repo/bench/corpora.toml", CORPORA);
        f.write("repo/kiln/scripts/oss/projects.toml", KILN_ROWS);
        let c = rows::parse(CORPORA, KILN_ROWS).unwrap().remove(0);
        let layout = Layout::new(&f.path("corpora"), "tiny");
        std::fs::create_dir_all(&layout.src).unwrap();
        std::fs::create_dir_all(&layout.build).unwrap();
        let commit = "c".repeat(40);
        std::fs::write(layout.fetch_stamp(), corpus::fetch_stamp(&c, &commit)).unwrap();
        let cstamp = corpus::configure_stamp(&commit, &["-S".to_owned()]);
        std::fs::write(layout.configure_stamp(), &cstamp).unwrap();
        std::fs::write(layout.generate_stamp(), corpus::generate_stamp(&cstamp, &c.generate))
            .unwrap();
        std::fs::write(layout.database(), "[]\n").unwrap();
        let fake = FAKE.replace("@DIR@", &f.path("fake").display().to_string());
        let slip = executable(&f, "bin/slip", &fake, 0o755);
        common::wait_executable(&slip);
        let p = plan();
        std::fs::create_dir_all(f.path("fake")).unwrap();
        std::fs::write(f.path("fake/plan"), plan_id(&p).to_string()).unwrap();
        World { f, p }
    }

    fn obs(&self, main: &str) -> Digest {
        obs(&self.p, main)
    }

    /// Template store `t<n>` with `shards` and a record of `jobs`.
    fn template(&self, n: u32, shards: &[Shard], jobs: Vec<Job>) {
        let dir = self.f.path(&format!("fake/t{n}"));
        let st = Store::open(&dir, Access::Exclusive).unwrap();
        let id = st.publish_plan(&self.p.to_bytes()).unwrap();
        for s in shards {
            st.publish_shard(&id, Phase::Map, 0, &s.observer, &s.to_bytes()).unwrap();
        }
        let started = STARTED + u64::from(n);
        let r = record(id, started, jobs);
        let path = st.publish_run(&id, Phase::Map, 0, started, &r.to_bytes()).unwrap();
        let rel = path.strip_prefix(st.root()).unwrap().display().to_string();
        std::fs::write(self.f.path(&format!("fake/t{n}.run")), rel).unwrap();
    }

    /// Templates 1 and 2 in which every observer has its `ok` (A..C) or
    /// `diagnosed` (D) shard, the same in both unless `differ`.
    fn templates(&self, differ: bool) {
        for n in [1, 2] {
            let digest = if differ && n == 2 { 2 } else { 1 };
            let shards: Vec<Shard> =
                [(A, Status::Ok), (B, Status::Ok), (C, Status::Ok), (D, Status::Diagnosed)]
                    .iter()
                    .map(|(m, st)| shard(self.obs(m), *st, if *m == B { digest } else { 1 }))
                    .collect();
            self.template(n, &shards, shards.iter().map(wrote).collect());
        }
    }

    fn env(&self) -> Env {
        Env { corpus_dir: Some(self.f.path("corpora").into()), target_dir: None }
    }

    fn run(&self, args: &[&str]) -> (Result<bool, Failure>, String) {
        let mut a: Vec<String> = args.iter().map(|s| (*s).to_owned()).collect();
        a.extend(["--slip".to_owned(), self.f.path("bin/slip").display().to_string()]);
        let mut out = Vec::new();
        let r = invariants::run(&self.f.path("repo"), &a, &self.env(), &mut out);
        (r, String::from_utf8(out).unwrap())
    }

    fn log(&self) -> String {
        std::fs::read_to_string(self.f.path("fake/log")).unwrap_or_default()
    }

    fn dir(&self) -> PathBuf {
        self.f.path("corpora/tiny")
    }

    /// The output with the corpus directory written `<corpus>`.
    fn norm(&self, text: &str) -> String {
        text.replace(&self.dir().display().to_string(), "<corpus>")
    }
}

// §9.1: the command makes the two stores (removing what was there), runs
// `slip parse` in each with the options unchanged, and holds.
#[test]
fn s9_1_command_holds() {
    let w = World::new("inv-a-holds");
    w.templates(false);
    w.f.write("corpora/tiny/stores/a1/stale", "x");
    let (r, out) = w.run(&["a", "tiny", "-j", "3"]);
    assert_eq!(r, Ok(true), "{out}");
    let run = |n: u32| std::fs::read_to_string(w.f.path(&format!("fake/t{n}.run"))).unwrap();
    assert_eq!(
        w.norm(&out),
        format!(
            "invariant a tiny\nplan {}\nobservers 4\nequal 4\nfailed_both 0\ndifferent 0\n\
             run <corpus>/stores/a1/{}\nrun <corpus>/stores/a2/{}\ninvariant a: holds\n",
            plan_id(&w.p),
            run(1),
            run(2)
        )
    );
    assert!(!w.dir().join("stores/a1/stale").exists(), "a1 is made fresh");
    let db = w.dir().join("build/compile_commands.json").display().to_string();
    let store = |n: &str| w.dir().join("stores").join(n).display().to_string();
    assert_eq!(
        w.log(),
        format!(
            "parse --db {db} --store {} -j 3\nparse --db {db} --store {} -j 3\n",
            store("a1"),
            store("a2")
        )
    );
}

// §9.1: a different observer fails the invariant (exit status 1), and a
// selection says so in the last line.
#[test]
fn s9_1_command_fails() {
    let w = World::new("inv-a-fails");
    w.templates(true);
    let (r, out) = w.run(&["a", "tiny", "--only", "src", "--only", "x", "--limit", "04"]);
    assert_eq!(r, Ok(false), "{out}");
    let b1 = id_of(&shard(w.obs(B), Status::Ok, 1));
    let b2 = id_of(&shard(w.obs(B), Status::Ok, 2));
    assert!(out.contains(&format!("different 1\n  /src/B.cpp: ok {b1} | ok {b2}\n")), "{out}");
    assert!(out.ends_with("invariant a: FAILED (1 observer(s) differ) (selection)\n"), "{out}");
    assert!(w.log().contains(" --only src --only x --limit 04\n"), "{}", w.log());
}

// §9.1: a run with a job for an observer the selection does not keep is
// an error; so are two plans.
#[test]
fn s9_1_command_errors() {
    let w = World::new("inv-a-sel");
    w.templates(false);
    let (r, out) = w.run(&["a", "tiny", "--limit", "2"]);
    let Err(Failure::Error(e)) = r else { panic!("{r:?}") };
    assert!(e.contains("which the selection does not keep"), "{e}");
    assert_eq!(out, "");

    let w = World::new("inv-a-plan");
    w.templates(false);
    std::fs::write(w.f.path("fake/plan"), d(1).to_string()).unwrap();
    let (r, _) = w.run(&["a", "tiny"]);
    let Err(Failure::Error(e)) = r else { panic!("{r:?}") };
    assert!(e.contains("left no map run of plan"), "{e}");
}

// §9: a `slip` status other than 0 or 1 (for `verify`, 4) fails the command.
#[test]
fn s9_slip_exits_3() {
    let w = World::new("inv-exit3");
    w.templates(false);
    std::fs::write(w.f.path("fake/parse.status"), "3").unwrap();
    let (r, out) = w.run(&["a", "tiny"]);
    assert_eq!(r, Err(Failure::Error("`slip parse` exited with status 3".into())));
    assert_eq!(out, "");
}

// §9: a corpus that is not configured, an unknown corpus, a missing slip.
#[test]
fn s9_not_configured() {
    let w = World::new("inv-notconf");
    std::fs::remove_file(w.dir().join("generate.stamp")).unwrap();
    let (r, _) = w.run(&["a", "tiny"]);
    assert_eq!(
        r,
        Err(Failure::Error(
            "corpus `tiny` is not configured: run `cargo xtask corpus configure tiny`".into()
        ))
    );
    let (r, _) = w.run(&["b", "tiny"]);
    assert!(matches!(r, Err(Failure::Error(e)) if e.contains("is not configured")));
    assert_eq!(w.log(), "", "slip is not run");
    let (r, _) = w.run(&["a", "nosuch"]);
    assert!(matches!(r, Err(Failure::Error(e)) if e.contains("nosuch")));

    let w = World::new("inv-noslip");
    let mut out = Vec::new();
    let r = invariants::run(&w.f.path("repo"), &["a".into(), "tiny".into()], &w.env(), &mut out);
    assert!(
        matches!(&r, Err(Failure::Error(e)) if e.ends_with("does not exist: run `cargo build --release -p slip_cli`")),
        "{r:?}"
    );
}

// §9: usage errors.
#[test]
fn s9_usage() {
    let w = World::new("inv-usage");
    for a in [
        &[][..],
        &["c", "tiny"],
        &["a"],
        &["a", "--only"],
        &["a", "tiny", "--only"],
        &["a", "tiny", "--limit", "x"],
        &["a", "tiny", "--limit", ""],
        &["a", "tiny", "--limit", "1", "--limit", "2"],
        &["a", "tiny", "-j", "1", "-j", "2"],
        &["a", "tiny", "--memory", "1G"],
        &["a", "tiny", "extra"],
    ] {
        let mut out = Vec::new();
        let args: Vec<String> = a.iter().map(|s| (*s).to_owned()).collect();
        let r = invariants::run(&w.f.path("repo"), &args, &w.env(), &mut out);
        assert!(matches!(r, Err(Failure::Usage(_))), "{a:?}: {r:?}");
    }
    let (r, _) = w.run(&["a", "tiny", "--slip", "x"]);
    assert!(matches!(r, Err(Failure::Usage(e)) if e.contains("`--slip` is given twice")));
    assert_eq!(w.log(), "");
    // Through the executable: usage is exit status 2, an unknown corpus 1.
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["invariants", "a"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.starts_with("xtask invariants: missing corpus\nusage:"), "{stderr}");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["invariants", "a", "no-such-corpus"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
}

// §9.2: (b) makes stores/a1 when it has no map run of this slip, runs
// `slip verify`, and reads the report; with a1 there, it reuses it.
#[test]
fn s9_2_command() {
    let w = World::new("inv-b");
    w.templates(false);
    let v = report(&w.p, 4, &[nv(D, "never_parsed")]);
    std::fs::write(w.f.path("fake/report.json"), json::write_pretty(&v).unwrap()).unwrap();
    std::fs::write(w.f.path("fake/verify.status"), "4").unwrap();
    w.f.write("corpora/tiny/stores/b.json", "stale");
    let (r, out) = w.run(&["b", "tiny", "-j", "2"]);
    assert_eq!(r, Ok(true), "{out}");
    let report_path = w.dir().join("stores/b.json");
    assert_eq!(
        w.norm(&out),
        format!(
            "invariant b tiny\nplan {}\nobservers 4\nmap_ok 3\nverified 3\nindeterminate_only 0\n\
             different 0\nnot_parsed 0\nunverifiable 0\nfailed 0\nnever_parsed 1\n\
             run /store/runs/x/verify.0.1.run\nreport <corpus>/stores/b.json\ninvariant b: holds\n",
            plan_id(&w.p)
        )
    );
    let db = w.dir().join("build/compile_commands.json").display().to_string();
    let a1 = w.dir().join("stores/a1").display().to_string();
    let verify = format!(
        "verify --plan {} --store {a1} --round 0 --report {} -j 2\n",
        plan_id(&w.p),
        report_path.display()
    );
    assert_eq!(w.log(), format!("--version \nparse --db {db} --store {a1} -j 2\n{verify}"));

    // Again: a1 holds a map run of this slip over the selection, reused.
    std::fs::write(w.f.path("fake/log"), "").unwrap();
    let (r, _) = w.run(&["b", "tiny", "-j", "2"]);
    assert_eq!(r, Ok(true));
    assert_eq!(w.log(), format!("--version \nplan --db {db} --store {a1}\n{verify}"));

    // Over another selection, a1 is made again (template 2).
    std::fs::write(w.f.path("fake/log"), "").unwrap();
    let v = report(&w.p, 1, &[]);
    std::fs::write(w.f.path("fake/report.json"), json::write_pretty(&v).unwrap()).unwrap();
    let (r, _) = w.run(&["b", "tiny", "--limit", "1"]);
    let Err(Failure::Error(e)) = r else { panic!("{r:?}") };
    assert!(e.contains("which the selection does not keep"), "{e}");
    assert!(w.log().contains("\nparse --db "), "{}", w.log());
}

// §9.2: an observer not verified fails (b); a `slip verify` status other
// than 0, 1 and 4 fails the command; an invalid report is an error naming it.
#[test]
fn s9_2_command_fails() {
    let w = World::new("inv-b-fails");
    w.templates(false);
    let v = report(&w.p, 4, &[mismatch(B, 2, 2), nv(D, "never_parsed")]);
    std::fs::write(w.f.path("fake/report.json"), json::write_pretty(&v).unwrap()).unwrap();
    std::fs::write(w.f.path("fake/verify.status"), "1").unwrap();
    let (r, out) = w.run(&["b", "tiny"]);
    assert_eq!(r, Ok(false), "{out}");
    assert!(out.contains("indeterminate_only 1\n"), "{out}");
    assert!(out.contains("  mismatch: /src/B.cpp: 2 finding(s), 2 indeterminate\n"), "{out}");
    assert!(out.ends_with("invariant b: FAILED (1 of 3 observer(s) not verified)\n"), "{out}");

    std::fs::write(w.f.path("fake/verify.status"), "3").unwrap();
    let (r, _) = w.run(&["b", "tiny"]);
    assert_eq!(r, Err(Failure::Error("`slip verify` exited with status 3".into())));

    std::fs::write(w.f.path("fake/verify.status"), "0").unwrap();
    std::fs::write(w.f.path("fake/report.json"), "{").unwrap();
    let (r, _) = w.run(&["b", "tiny"]);
    let Err(Failure::Error(e)) = r else { panic!("{r:?}") };
    assert!(e.starts_with(&format!("{}: not JSON", w.dir().join("stores/b.json").display())));
}
