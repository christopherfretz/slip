//! `cargo xtask perf` (spec/harness.md §7).
//!
//! Every test writes its own store in a temporary directory with
//! `slip_exec::store` and `slip_model`: a plan of four observers with
//! exclusions of three reasons, shards of every status, and a run record
//! with retried, crashed and killed jobs (the "full" run, round 0), plus a
//! cancelled run (round 1) that has a shard for only one of its two
//! observers. Files the store API refuses to publish (a shard of another
//! observer, a record at the wrong path) are written directly.
//!
//! Golden files (`tests/golden/perf/`) hold no temporary path: the summary
//! goldens are rendered with the record's path *relative to the store* as
//! the given path (and the executable is run in the store with relative
//! paths), and a baseline holds no path at all. The main files of the plan
//! are invented absolute paths that exist nowhere. `SLIP_BLESS=1` rewrites
//! the goldens.

#![cfg(target_os = "linux")]
// Observers are `a`..`d` after the main files `A`..`D`.
#![allow(clippy::many_single_char_names)]

mod common;

use common::Fixture;
use slip_exec::store::{Access, Store};
use slip_model::{
    Command as Cmd, Configuration, Counter, Digest, End, Excluded, FileRef, Job, Language,
    Observer, Origin, Outcome, Phase, Plan, Reason, Run, RunRecord, Shard, Status,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use xtask::corpus::{Corpus, rows};
use xtask::perf::{self, Failure, Loaded, RunPath};

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

fn excluded(entry: u64, reason: Reason) -> Excluded {
    Excluded {
        configuration: 0,
        entry,
        file: format!("/src/x{entry}.s").into_bytes(),
        reason,
        detail: String::new(),
    }
}

/// A plan of `mains` (cost descending in the order given) and `excluded`.
fn plan_of(mains: &[&str], excluded: Vec<Excluded>) -> Plan {
    let n = mains.len() as u64;
    let mut observers: Vec<Observer> = mains
        .iter()
        .enumerate()
        .map(|(i, m)| observer(m, (n - i as u64) * 100, i as u64))
        .collect();
    observers.sort_by(|a, b| b.cost_hint.cmp(&a.cost_hint).then(a.id.cmp(&b.id)));
    Plan {
        tool_slip: "0.0.0".into(),
        tool_kiln: "1.3.0".into(),
        configurations: vec![Configuration {
            name: "release".into(),
            database: FileRef {
                path: b"/build/compile_commands.json".to_vec(),
                size: 1,
                content: d(9),
            },
        }],
        observers,
        excluded,
    }
}

fn shard(observer: Digest, round: u64, status: Status, counters: &[(&str, u64)]) -> Shard {
    let message = if status == Status::Ok { String::new() } else { format!("{status:?}") };
    Shard {
        observer,
        phase: Phase::Map,
        round,
        outcome: Outcome {
            status,
            errors: u64::from(status == Status::Diagnosed),
            warnings: 0,
            message,
        },
        inputs: (status != Status::Failed).then(Vec::new),
        counters: counters.iter().map(|(n, v)| Counter { name: (*n).into(), value: *v }).collect(),
        digests: None,
        verdict: None,
        edits: None,
        binding: None,
        rule_use: None,
        spellings: None,
        negatives: None,
    }
}

fn job(observer: Digest, attempt: u64, end: End, cpu_ns: u64, peak_rss: u64) -> Job {
    Job {
        observer,
        attempt,
        end,
        shard: None,
        queued_ns: 0,
        wall_ns: 1_000,
        cpu_ns,
        peak_rss,
        estimate: 1 << 20,
    }
}

fn record(plan: Digest, started: u64, wall_ns: u64, jobs: Vec<Job>) -> RunRecord {
    RunRecord {
        run: Run {
            tool_slip: "0.0.0".into(),
            tool_kiln: "1.3.0".into(),
            plan,
            phase: Phase::Map,
            round: 0,
            started_unix_ns: started,
            wall_ns,
            cores: 8,
            workers_max: 2,
            memory_budget: 17_179_869_184,
            peak_rss_sum: 400_000_000,
        },
        jobs,
    }
}

/// A store under a fixture, with its plan published.
struct World {
    _f: Fixture,
    store: Store,
    plan: Plan,
    id: Digest,
}

impl World {
    fn new(tag: &str, plan: Plan) -> World {
        let f = Fixture::empty(tag);
        let store = Store::open(&f.path("store"), Access::Exclusive).unwrap();
        let id = store.publish_plan(&plan.to_bytes()).unwrap();
        World { _f: f, store, plan, id }
    }
    fn obs(&self, main: &str) -> Digest {
        self.plan.observers.iter().find(|o| o.main.path == main.as_bytes()).unwrap().id
    }
    fn shard(&self, s: &Shard) {
        self.store.publish_shard(&self.id, s.phase, s.round, &s.observer, &s.to_bytes()).unwrap();
    }
    /// Publishes `r` as the record of round `round` (which it is made to name).
    fn run(&self, round: u64, r: &RunRecord) -> PathBuf {
        let mut r = r.clone();
        r.run.round = round;
        let p = self
            .store
            .publish_run(&self.id, Phase::Map, round, r.run.started_unix_ns, &r.to_bytes())
            .unwrap();
        assert_eq!(p, self.store.run_path(&self.id, Phase::Map, round, r.run.started_unix_ns));
        p
    }
    fn root(&self) -> &Path {
        self.store.root()
    }
    /// `path` relative to the store root.
    fn rel(&self, path: &Path) -> String {
        path.strip_prefix(self.root()).unwrap().display().to_string()
    }
}

const A: &str = "/src/llvm/lib/A.cpp";
const B: &str = "/src/llvm/lib/B.cpp";
const C: &str = "/src/llvm/lib/C.cpp";
const D: &str = "/src/llvm/lib/D.cpp";
const STARTED: u64 = 1_700_000_000_000_000_000;
const CANCELLED_STARTED: u64 = 1_700_000_100_000_000_000;

/// The full run (round 0) and the cancelled run (round 1) of §7.
struct Runs {
    w: World,
    full: PathBuf,
    cancelled: PathBuf,
}

fn runs(tag: &str) -> Runs {
    let plan = plan_of(
        &[A, B, C, D],
        vec![
            excluded(4, Reason::NotACompile),
            excluded(5, Reason::NotACompile),
            excluded(6, Reason::UnsupportedFlag),
            excluded(7, Reason::MissingFile),
        ],
    );
    let w = World::new(tag, plan);
    let (a, b, c, dd) = (w.obs(A), w.obs(B), w.obs(C), w.obs(D));
    w.shard(&shard(
        a,
        0,
        Status::Ok,
        &[("inputs.bytes", 1000), ("inputs.files", 3), ("pp.tokens", 5000)],
    ));
    w.shard(&shard(
        b,
        0,
        Status::Diagnosed,
        &[("inputs.bytes", 500), ("inputs.files", 2), ("pp.tokens", 100)],
    ));
    w.shard(&shard(c, 0, Status::Rejected, &[]));
    w.shard(&shard(dd, 0, Status::Failed, &[]));
    // B and C tie on cpu (1.000 s), B and D on peak_rss (100 MB).
    let full = record(
        w.id,
        STARTED,
        4_123_500_000,
        vec![
            job(b, 1, End::Crashed, 400_000_000, 999_999),
            job(dd, 1, End::KilledTime, 3_000_000_000, 100_000_000),
            job(a, 1, End::Completed, 2_500_000_000, 300_000_000),
            job(c, 1, End::KilledMemory, 500_000_000, 4_000_000_000),
            job(b, 2, End::Completed, 600_000_000, 100_000_000),
            job(c, 2, End::Completed, 500_000_000, 0),
            job(dd, 2, End::Crashed, 0, 0),
        ],
    );
    let full = w.run(0, &full);
    w.shard(&shard(a, 1, Status::Ok, &[("inputs.bytes", 1000), ("inputs.files", 3)]));
    let cancelled = record(
        w.id,
        CANCELLED_STARTED,
        1_000_000_000,
        vec![
            job(a, 1, End::Completed, 900_000_000, 250_000_000),
            job(b, 1, End::Cancelled, 100_000_000, 0),
        ],
    );
    let cancelled = w.run(1, &cancelled);
    Runs { w, full, cancelled }
}

fn golden_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/perf").join(name)
}

fn golden(name: &str, actual: &str) {
    let path = golden_path(name);
    if std::env::var_os("SLIP_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, actual).unwrap();
        eprintln!("blessed {}", path.display());
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}; run with SLIP_BLESS=1 to create", path.display()));
    assert_eq!(actual, expected, "{name} differs from its golden; SLIP_BLESS=1 rewrites it");
}

const KILN_ROWS: &str = "[tiny_project]\nurl = \"https://example.org/tiny.git\"\ntag = \"v1.0\"\n";

fn corpora(tag_row: &str) -> String {
    format!(
        "[tiny]\nkiln = \"tiny_project\"\ncxx_std = 17\nbuild_type = \"Release\"\n\
         c_compiler = \"cc\"\ncxx_compiler = \"c++\"\nconfigure = []\ngenerate = [\"{tag_row}\"]\n"
    )
}

fn corpus(tag: &str) -> Corpus {
    let kiln = KILN_ROWS.replace("v1.0", tag);
    rows::parse(&corpora("gen"), &kiln).unwrap().remove(0)
}

/// A repository holding `bench/corpora.toml` and Kiln's `projects.toml`.
fn repo(f: &Fixture) -> PathBuf {
    f.write("repo/bench/corpora.toml", &corpora("gen"));
    f.write("repo/kiln/scripts/oss/projects.toml", KILN_ROWS);
    f.path("repo")
}

fn strings(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| (*s).to_owned()).collect()
}

fn run_cmd(repo: &Path, a: &[&str]) -> (Result<bool, Failure>, String) {
    let mut out = Vec::new();
    let r = perf::run(repo, &strings(a), &mut out);
    (r, String::from_utf8(out).unwrap())
}

fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}

// --------------------------------------------------------- parse_run_path

/// §7 first paragraph: the store is three levels up; plan, phase, round and
/// start time come from the path.
#[test]
fn parse_run_path_good() {
    let hex = "ab".repeat(32);
    let p = format!("/x/store/runs/{hex}/map.0.17.run");
    let rp = perf::parse_run_path(Path::new(&p)).unwrap();
    assert_eq!(
        rp,
        RunPath {
            store: PathBuf::from("/x/store"),
            plan: Digest([0xab; 32]),
            phase: Phase::Map,
            round: 0,
            started_unix_ns: 17
        }
    );
    let rp =
        perf::parse_run_path(Path::new(&format!("runs/{hex}/verify.12.18446744073709551615.run")))
            .unwrap();
    assert_eq!(rp.store, PathBuf::from("."));
    assert_eq!((rp.phase, rp.round, rp.started_unix_ns), (Phase::Verify, 12, u64::MAX));
    let rp = perf::parse_run_path(Path::new(&format!("../s/runs/{hex}/map.1.2.run"))).unwrap();
    assert_eq!(rp.store, PathBuf::from("../s"));
}

/// §7: "A path of another shape … is an error", one case per part.
#[test]
fn parse_run_path_malformed() {
    let hex = "ab".repeat(32);
    let cases = [
        ("map.0.1.run".to_owned(), "not a run record path"),
        (format!("{hex}/map.0.1.run"), "not a run record path"),
        (format!("s/plans/{hex}/map.0.1.run"), "is not in a `runs` directory"),
        (format!("s/runs/{}/map.0.1.run", "AB".repeat(32)), "is not a plan id"),
        (format!("s/runs/{}/map.0.1.run", "ab".repeat(31)), "is not a plan id"),
        (format!("s/runs/{hex}/map.0.1"), "is not `<phase>.<round>.<started>.run`"),
        (format!("s/runs/{hex}/map.0.1.run.bak"), "is not `<phase>.<round>.<started>.run`"),
        (format!("s/runs/{hex}/map.0.1.log"), "is not `<phase>.<round>.<started>.run`"),
        (format!("s/runs/{hex}/build.0.1.run"), "is not a phase"),
        (format!("s/runs/{hex}/map.01.1.run"), "round `01` is not a canonical decimal u64"),
        (format!("s/runs/{hex}/map..1.run"), "round `` is not a canonical decimal u64"),
        (format!("s/runs/{hex}/map.+1.1.run"), "round `+1` is not"),
        (
            format!("s/runs/{hex}/map.0.18446744073709551616.run"),
            "start time `18446744073709551616` is not",
        ),
        (format!("s/runs/{hex}/map.0.-1.run"), "start time `-1` is not"),
        (format!("s/runs/{hex}/.."), "not a run record path"),
    ];
    for (path, want) in cases {
        let e = perf::parse_run_path(Path::new(&path)).unwrap_err();
        assert!(e.starts_with(&format!("{path}: ")), "{e}");
        assert!(e.contains(want), "{path}: {e}");
    }
}

/// §7: a record whose plan, phase, round or start time disagrees with its
/// path is an error (the store names the file by all four).
#[test]
fn record_disagreeing_with_its_path() {
    let r = runs("perf-disagree");
    let bytes = std::fs::read(&r.full).unwrap();
    let other = "cd".repeat(32);
    let put = |rel: &str| {
        let p = r.w.root().join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, &bytes).unwrap();
        p
    };
    let p = put(&format!("runs/{other}/map.0.{STARTED}.run"));
    let e = perf::load(&p).unwrap_err();
    assert!(
        e.contains(&format!("the record's plan {} is not the plan {other} of its path", r.w.id)),
        "{e}"
    );
    let p = put(&format!("runs/{}/verify.0.{STARTED}.run", r.w.id));
    let e = perf::load(&p).unwrap_err();
    assert!(e.contains("the record's phase map is not the phase verify of its path"), "{e}");
    // Renamed to another round: the record says round 0.
    let p = put(&format!("runs/{}/map.1.{STARTED}.run", r.w.id));
    let e = perf::load(&p).unwrap_err();
    assert!(e.contains("the record's round 0 is not the round 1 of its path"), "{e}");
    let p = put(&format!("runs/{}/map.0.5.run", r.w.id));
    let e = perf::load(&p).unwrap_err();
    assert!(
        e.contains(&format!("the record's start time {STARTED} is not the start time 5")),
        "{e}"
    );
    // Not a run record at all: the plan's bytes under a record's name.
    let plan = std::fs::read(r.w.store.plan_path(&r.w.id)).unwrap();
    let p = r.w.root().join(format!("runs/{}/map.0.6.run", r.w.id));
    std::fs::write(&p, plan).unwrap();
    let e = perf::load(&p).unwrap_err();
    assert!(e.starts_with(&format!("{}: not a valid run record: ", p.display())), "{e}");
    // A record whose plan is not in the store.
    let p = r.w.root().join(format!("runs/{}/map.0.{STARTED}.run", r.w.id));
    std::fs::remove_file(r.w.store.plan_path(&r.w.id)).unwrap();
    let e = perf::load(&p).unwrap_err();
    assert!(e.ends_with("the run's plan is not in the store"), "{e}");
}

// --------------------------------------------------------------- counters

/// §7.1: every row of the counter table, zeros included.
#[test]
fn counters_every_row() {
    let r = runs("perf-counters");
    let loaded = perf::load(&r.full).unwrap();
    let got = perf::counters(&loaded).unwrap();
    let want: BTreeMap<String, u64> = [
        ("inputs.bytes", 1500),
        ("inputs.files", 5),
        ("plan.excluded.malformed_entry", 0),
        ("plan.excluded.missing_file", 1),
        ("plan.excluded.not_a_compile", 2),
        ("plan.excluded.unsupported_compiler", 0),
        ("plan.excluded.unsupported_flag", 1),
        ("plan.excluded.unsupported_language", 0),
        ("plan.observers", 4),
        ("pp.tokens", 5100),
        ("run.observers", 4),
        ("shard.diagnosed", 1),
        ("shard.failed", 1),
        ("shard.ok", 1),
        ("shard.rejected", 1),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v))
    .collect();
    assert_eq!(got, want);
}

/// §7.1: an observer of the record with no shard in the store is an error
/// (a cancelled run).
#[test]
fn counters_missing_shard() {
    let r = runs("perf-missing");
    let loaded = perf::load(&r.cancelled).unwrap();
    assert_eq!(loaded.missing_shards(), 1);
    let e = perf::counters(&loaded).unwrap_err();
    assert!(e.contains("1 observer(s) of the record have no shard in the store"), "{e}");
    assert!(e.contains(&r.w.obs(B).to_string()), "names the observer: {e}");
}

/// §7.1: a shard that names another observer, phase or round, and one that
/// does not validate, are errors naming the shard file.
#[test]
fn shard_of_another_observer_or_invalid() {
    let r = runs("perf-badshard");
    let (a, b) = (r.w.obs(A), r.w.obs(B));
    let path = r.w.store.shard_path(&r.w.id, Phase::Map, 0, &b);
    std::fs::write(&path, shard(a, 0, Status::Ok, &[]).to_bytes()).unwrap();
    let e = perf::load(&r.full).unwrap_err();
    assert!(e.starts_with(&format!("{}: the shard names observer {a}", path.display())), "{e}");
    assert!(e.contains("the store is not the run's"), "{e}");
    std::fs::write(&path, shard(b, 1, Status::Ok, &[]).to_bytes()).unwrap();
    let e = perf::load(&r.full).unwrap_err();
    assert!(e.contains("phase map, round 1; the run's is"), "{e}");
    let mut v = shard(b, 0, Status::Ok, &[]);
    v.phase = Phase::Verify;
    std::fs::write(&path, v.to_bytes()).unwrap();
    let e = perf::load(&r.full).unwrap_err();
    assert!(e.contains("phase verify, round 0"), "{e}");
    std::fs::write(&path, b"not a shard").unwrap();
    let e = perf::load(&r.full).unwrap_err();
    assert!(e.starts_with(&format!("{}: not a valid shard: ", path.display())), "{e}");
}

/// §7.1: a counter sum that overflows `u64` is an error, and so
/// is a shard counter named like a counter §7.1 computes.
#[test]
fn counters_overflow_and_collision() {
    let w = World::new("perf-overflow", plan_of(&[A, B], vec![]));
    let (a, b) = (w.obs(A), w.obs(B));
    w.shard(&shard(a, 0, Status::Ok, &[("pp.tokens", u64::MAX)]));
    w.shard(&shard(b, 0, Status::Ok, &[("pp.tokens", 1)]));
    let p = w.run(
        0,
        &record(w.id, 1, 1, vec![job(a, 1, End::Completed, 0, 0), job(b, 1, End::Completed, 0, 0)]),
    );
    let loaded = perf::load(&p).unwrap();
    let e = perf::counters(&loaded).unwrap_err();
    assert_eq!(e, "counter `pp.tokens` summed over the shards overflows a 64-bit unsigned integer");
    let e = perf::render_summary("x", &loaded).unwrap_err();
    assert!(e.contains("overflows"), "{e}");
    w.shard(&shard(b, 0, Status::Ok, &[("run.observers", 1)]));
    let loaded = perf::load(&p).unwrap();
    let e = perf::counters(&loaded).unwrap_err();
    assert!(
        e.ends_with(
            "shard counter `run.observers` has the name of a counter of spec/harness.md §7.1"
        ),
        "{e}"
    );
}

/// §7 loading: a record observer that is not in the plan is an error.
#[test]
fn record_observer_not_in_plan() {
    let w = World::new("perf-notinplan", plan_of(&[A], vec![]));
    let p = w.run(0, &record(w.id, 1, 1, vec![job(d(7), 1, End::Crashed, 0, 0)]));
    let e = perf::load(&p).unwrap_err();
    assert!(e.ends_with(&format!("observer {} of the record is not in the plan", d(7))), "{e}");
}

// ------------------------------------------------- measured and percentile

fn completed(rss: &[u64]) -> RunRecord {
    let jobs = rss.iter().map(|&r| job(d(1), 1, End::Completed, 1, r)).collect();
    record(d(0), 0, 0, jobs)
}

/// §7.1 nearest rank: `ceil(p × n / 100)`, 0 for an empty list.
#[test]
fn percentile_nearest_rank() {
    assert_eq!(perf::percentile(&[], 50), 0);
    assert_eq!(perf::percentile(&[7], 0), 7);
    assert_eq!(perf::percentile(&[7], 50), 7);
    assert_eq!(perf::percentile(&[3, 9], 50), 3);
    assert_eq!(perf::percentile(&[3, 9], 51), 9);
    assert_eq!(perf::percentile(&[3, 9], 100), 9);
    assert_eq!(perf::percentile(&[3, 9], 1000), 9);
}

/// §7.1 measured over 0, 1, 2, 100 and 101 jobs.
#[test]
fn measured_over_job_counts() {
    // (jobs, max, p50, p90, p99), peak_rss of job i being i + 1.
    let cases = [
        (0, 0, 0, 0, 0),
        (1, 1, 1, 1, 1),
        (2, 2, 1, 2, 2),
        (100, 100, 50, 90, 99),
        (101, 101, 51, 91, 100),
    ];
    for (n, max, p50, p90, p99) in cases {
        let rss: Vec<u64> = (1..=n).rev().collect();
        let m = perf::measured(&completed(&rss)).unwrap();
        assert_eq!(
            (m.peak_rss_max, m.peak_rss_p50, m.peak_rss_p90, m.peak_rss_p99),
            (max, p50, p90, p99),
            "{n} jobs"
        );
        assert_eq!(m.cpu_ns, n, "cpu_s sums every job");
    }
}

/// §7.1: jobs that did not complete, or have `peak_rss` 0, are left out of
/// the percentiles (but their cpu counts); the rest come from the record.
#[test]
fn measured_leaves_out_incomplete_and_unknown() {
    let mut r = completed(&[10, 20]);
    r.jobs.push(job(d(2), 1, End::Crashed, 5, 1_000));
    r.jobs.push(job(d(3), 1, End::KilledMemory, 5, 2_000));
    r.jobs.push(job(d(4), 1, End::Cancelled, 5, 3_000));
    r.jobs.push(job(d(5), 1, End::Completed, 5, 0));
    let m = perf::measured(&r).unwrap();
    assert_eq!((m.peak_rss_max, m.peak_rss_p50, m.peak_rss_p90, m.peak_rss_p99), (20, 10, 20, 20));
    assert_eq!(m.cpu_ns, 22);
    assert_eq!(
        (m.wall_ns, m.peak_rss_sum, m.cores, m.workers_max, m.memory_budget),
        (0, 400_000_000, 8, 2, 17_179_869_184)
    );
    r.jobs.push(job(d(6), 1, End::Completed, u64::MAX, 1));
    assert_eq!(
        perf::measured(&r).unwrap_err(),
        "cpu_ns summed over the jobs overflows a 64-bit unsigned integer"
    );
}

/// §7.1: seconds have three decimals, rounded half up, from
/// integer nanoseconds.
#[test]
fn seconds_rounding() {
    for (ns, want) in [
        (0, "0.000"),
        (499_999, "0.000"),
        (500_000, "0.001"),
        (1_234_499_999, "1.234"),
        (1_234_500_000, "1.235"),
        (999_500_000, "1.000"),
        (u64::MAX, "18446744073.710"),
    ] {
        assert_eq!(perf::seconds(ns), want, "{ns}");
    }
}

// -------------------------------------------------------------- summarize

/// §7.2 golden: the full run, with ties broken by observer id.
#[test]
fn summarize_full() {
    let r = runs("perf-sum-full");
    let loaded = perf::load(&r.full).unwrap();
    let text = perf::render_summary(&r.w.rel(&r.full), &loaded).unwrap();
    golden("summary-full.txt", &text);
    // B and C tie on cpu, B and D on peak_rss: each pair in id order.
    let (b, c, dd) = (r.w.obs(B), r.w.obs(C), r.w.obs(D));
    let order = |x: Digest, y: Digest, xs: &str, ys: &str, section: &str| {
        let body = &text[text.find(section).unwrap()..];
        let (px, py) = (body.find(xs).unwrap(), body.find(ys).unwrap());
        assert_eq!(px < py, x < y, "{section}: {xs} / {ys}");
    };
    order(b, c, &format!("1.000 {B}"), &format!("1.000 {C}"), "top cpu");
    order(b, dd, &format!("100000000 {B}"), &format!("100000000 {D}"), "top peak_rss");
}

/// §7.2 golden: a cancelled run prints what the record gives, counts the
/// shards present and adds `missing shards`.
#[test]
fn summarize_cancelled() {
    let r = runs("perf-sum-cancel");
    let loaded = perf::load(&r.cancelled).unwrap();
    let text = perf::render_summary(&r.w.rel(&r.cancelled), &loaded).unwrap();
    golden("summary-cancelled.txt", &text);
    assert!(text.contains("shard failed 0\nmissing shards 1\njob completed 1\n"), "{text}");
}

/// §7.2: efficiency is `-` when wall time × workers is 0; top lists hold
/// at most ten observers; a main path is one line of text.
#[test]
fn summarize_edges() {
    let r = runs("perf-sum-edges");
    let mut loaded: Loaded = perf::load(&r.full).unwrap();
    loaded.record.run.workers_max = 0;
    let text = perf::render_summary("given", &loaded).unwrap();
    assert!(text.contains("\nefficiency -\n"), "{text}");
    loaded.record.run.workers_max = 2;
    loaded.record.run.wall_ns = 0;
    assert!(perf::render_summary("given", &loaded).unwrap().contains("\nefficiency -\n"));

    let mains: Vec<String> = (0..12).map(|i| format!("/src/m{i:02}.cpp")).collect();
    let refs: Vec<&str> = mains.iter().map(String::as_str).collect();
    let mut plan = plan_of(&refs, vec![]);
    plan.observers[0].main.path = b"/src/odd\\name\n\xff.cpp".to_vec();
    let w = World::new("perf-sum-top", plan);
    let jobs: Vec<Job> = w
        .plan
        .observers
        .iter()
        .enumerate()
        .map(|(i, o)| {
            job(o.id, 1, End::Completed, i as u64 * 1_000_000, if i == 0 { 5 } else { 1 })
        })
        .collect();
    for o in &w.plan.observers {
        w.shard(&shard(o.id, 0, Status::Ok, &[]));
    }
    let p = w.run(0, &record(w.id, 3, 1, jobs));
    let text = perf::render_summary("given", &perf::load(&p).unwrap()).unwrap();
    let top: Vec<&str> = text
        .split("top cpu\n")
        .nth(1)
        .unwrap()
        .split("top peak_rss\n")
        .next()
        .unwrap()
        .lines()
        .collect();
    assert_eq!(top.len(), 10);
    assert_eq!(top[0], "  0.011 /src/m11.cpp");
    assert!(text.contains("top peak_rss\n  5 /src/odd\\\\name\\x0a\\xff.cpp\n"), "{text}");
}

/// §7.2: two records in one invocation, through the executable run in the
/// store with relative paths, separated by an empty line.
#[test]
fn summarize_two_records_through_the_executable() {
    let r = runs("perf-sum-two");
    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["perf", "summarize", &r.w.rel(&r.full), &r.w.rel(&r.cancelled)])
        .current_dir(r.w.root())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let full = std::fs::read_to_string(golden_path("summary-full.txt")).unwrap();
    let cancelled = std::fs::read_to_string(golden_path("summary-cancelled.txt")).unwrap();
    assert_eq!(String::from_utf8(out.stdout).unwrap(), format!("{full}\n{cancelled}"));
}

// ----------------------------------------------------------------- record

/// §7.3 golden: the baseline of the full run.
#[test]
fn record_golden() {
    let r = runs("perf-rec");
    let loaded = perf::load(&r.full).unwrap();
    let text = perf::render_baseline(&corpus("v1.0"), &loaded).unwrap();
    golden("baseline-tiny.json", &text);
}

/// §7.3: `record` refuses a record that does not cover the plan.
#[test]
fn record_refuses_partial_coverage() {
    let r = runs("perf-rec-partial");
    let loaded = perf::load(&r.cancelled).unwrap();
    let e = perf::render_baseline(&corpus("v1.0"), &loaded).unwrap_err();
    assert!(e.contains("the record covers 2 of the plan's 4 observers"), "{e}");
    let f = Fixture::empty("perf-rec-partial-repo");
    let repo = repo(&f);
    let (res, out) = run_cmd(&repo, &["record", "tiny", s(&r.cancelled)]);
    assert!(matches!(res, Err(Failure::Error(_))), "{res:?}");
    assert_eq!(out, "");
    assert!(!repo.join("bench/baselines/tiny.json").exists(), "nothing is written");
}

/// §7.3: `record` writes the baseline (no absolute path, no time of day),
/// prints its path, replaces an existing one; `check` of the same run is
/// then `ok`.
#[test]
fn record_then_check_is_ok() {
    let r = runs("perf-rec-check");
    let f = Fixture::empty("perf-rec-check-repo");
    let repo = repo(&f);
    let target = repo.join("bench/baselines/tiny.json");
    f.write("repo/bench/baselines/tiny.json", "old");
    let (res, out) = run_cmd(&repo, &["record", "tiny", s(&r.full)]);
    assert_eq!(res, Ok(true));
    assert_eq!(out, format!("{}\n", target.display()));
    let text = std::fs::read_to_string(&target).unwrap();
    assert_eq!(text, std::fs::read_to_string(golden_path("baseline-tiny.json")).unwrap());
    assert!(!text.contains("\"/") && !text.contains(s(f.root())), "no absolute path: {text}");
    assert!(!text.contains(&STARTED.to_string()), "no time of day: {text}");
    let leftovers: Vec<_> = std::fs::read_dir(target.parent().unwrap()).unwrap().collect();
    assert_eq!(leftovers.len(), 1, "no temporary file is left");
    let (res, out) = run_cmd(&repo, &["check", "tiny", s(&r.full)]);
    assert_eq!(res, Ok(true), "{out}");
    assert_eq!(
        out,
        "wall_s 4.124 (\u{d7}1.00)\ncpu_s 7.500 (\u{d7}1.00)\npeak_rss_max 300000000 (\u{d7}1.00)\n\
         peak_rss_p50 100000000 (\u{d7}1.00)\npeak_rss_p90 300000000 (\u{d7}1.00)\n\
         peak_rss_p99 300000000 (\u{d7}1.00)\npeak_rss_sum 400000000 (\u{d7}1.00)\ncores 8 (\u{d7}1.00)\n\
         workers_max 2 (\u{d7}1.00)\nmemory_budget 17179869184 (\u{d7}1.00)\nperf check: tiny: ok\n"
    );
}

// ------------------------------------------------------------------ check

/// The full run's baseline with `edit` applied to its text, checked against
/// the full run with the corpus pinned at `tag`.
fn check_edited(r: &Runs, tag: &str, edit: impl Fn(String) -> String) -> (String, bool) {
    let loaded = perf::load(&r.full).unwrap();
    let text = edit(perf::render_baseline(&corpus("v1.0"), &loaded).unwrap());
    let baseline = perf::read_baseline(text.as_bytes()).unwrap();
    perf::check(&baseline, &corpus(tag), &loaded).unwrap()
}

/// §7.3: a changed, a missing and an extra counter each fail the check with
/// its line.
#[test]
fn check_counter_differences() {
    let r = runs("perf-check-counters");
    let (report, ok) =
        check_edited(&r, "v1.0", |t| t.replace("\"inputs.bytes\": 1500", "\"inputs.bytes\": 1501"));
    assert!(!ok);
    assert!(report.starts_with("inputs.bytes: baseline 1501, run 1500\nwall_s "), "{report}");
    assert!(report.ends_with("perf check: tiny: FAILED (1 difference(s))\n"), "{report}");
    // In the baseline only: the run is missing it.
    let (report, ok) = check_edited(&r, "v1.0", |t| {
        t.replace("\"inputs.bytes\": 1500,", "\"inputs.bytes\": 1500,\n    \"lex.tokens\": 3,")
    });
    assert!(!ok);
    assert!(report.starts_with("lex.tokens: baseline 3, run -\n"), "{report}");
    // In the run only: extra.
    let (report, ok) = check_edited(&r, "v1.0", |t| t.replace("    \"pp.tokens\": 5100,\n", ""));
    assert!(!ok);
    assert!(report.starts_with("pp.tokens: baseline -, run 5100\n"), "{report}");
}

/// §7.3: a different pin fails the check.
#[test]
fn check_different_pin() {
    let r = runs("perf-check-pin");
    let (report, ok) = check_edited(&r, "v2.0", |t| t);
    assert!(!ok);
    assert!(report.starts_with("pin: baseline v1.0, run v2.0\nwall_s "), "{report}");
    assert!(report.ends_with("perf check: tiny: FAILED (1 difference(s))\n"), "{report}");
}

/// §7.3: different tool versions and measured values are printed and do
/// not fail the check.
#[test]
fn check_tools_and_measured_are_informational() {
    let r = runs("perf-check-info");
    let (report, ok) = check_edited(&r, "v1.0", |t| {
        t.replace("\"slip\": \"0.0.0\"", "\"slip\": \"0.1.0\"")
            .replace("\"kiln\": \"1.3.0\"", "\"kiln\": \"1.2.0\"")
            .replace("\"wall_s\": 4.124", "\"wall_s\": 2.0")
            .replace("\"cores\": 8", "\"cores\": 0")
            .replace("\"peak_rss_max\": 300000000", "\"peak_rss_max\": 600000000")
    });
    assert!(ok, "{report}");
    assert!(
        report.starts_with("slip: baseline 0.1.0, run 0.0.0\nkiln: baseline 1.2.0, run 1.3.0\n"),
        "{report}"
    );
    assert!(report.contains("\nwall_s 4.124 (\u{d7}2.06)\n"), "{report}");
    assert!(report.contains("\npeak_rss_max 300000000 (\u{d7}0.50)\n"), "{report}");
    assert!(report.contains("\ncores 8 (\u{d7}-)\n"), "a zero baseline has no ratio: {report}");
    assert!(report.ends_with("perf check: tiny: ok\n"), "{report}");
}

/// §7.3: a baseline is read strictly; each error names the member.
#[test]
fn check_rejects_malformed_baselines() {
    let r = runs("perf-check-bad");
    let loaded = perf::load(&r.full).unwrap();
    let good = perf::render_baseline(&corpus("v1.0"), &loaded).unwrap();
    let err = |t: String| perf::read_baseline(t.as_bytes()).unwrap_err();
    assert_eq!(err(good.replacen('{', "{\"extra\": 1, ", 1)), "at extra: unknown member");
    assert_eq!(err(good.replace("\"pin\": \"v1.0\",\n", "")), "no member `pin`");
    assert_eq!(err(good.replace("\"pin\": \"v1.0\"", "\"pin\": 1")), "at pin: expected a string");
    assert_eq!(
        err(good.replace("\"inputs.files\": 5", "\"inputs.files\": -5")),
        "at counters.inputs.files: expected an integer in 0..2^64"
    );
    assert_eq!(
        err(good.replace("\"inputs.files\": 5", "\"inputs.files\": 5.0")),
        "at counters.inputs.files: expected an integer in 0..2^64"
    );
    assert_eq!(
        err(good.replace("\"wall_s\": 4.124", "\"wall_s\": 4")),
        "at measured.wall_s: expected a non-negative number of seconds with a fraction"
    );
    assert_eq!(
        err(good.replace("\"cores\": 8", "\"cores\": \"8\"")),
        "at measured.cores: expected an integer in 0..2^64"
    );
    assert_eq!(err(good.replace("    \"cores\": 8,\n", "")), "at measured: no member `cores`");
    assert_eq!(
        err(good.replace("\"cores\": 8", "\"cores\": 8, \"speed\": 1")),
        "at measured.speed: unknown member"
    );
    assert!(err("[]".to_owned()).starts_with("at the top level"));
    assert!(err("{".to_owned()).starts_with("not JSON: "));
}

/// §7.3 through `perf::run`: a check failure is exit status 1 (`Ok(false)`);
/// a missing baseline and a baseline of another corpus are errors naming the
/// file.
#[test]
fn check_through_run() {
    let r = runs("perf-check-run");
    let f = Fixture::empty("perf-check-run-repo");
    let repo = repo(&f);
    let (res, _) = run_cmd(&repo, &["check", "tiny", s(&r.full)]);
    match res {
        Err(Failure::Error(e)) => {
            assert!(e.starts_with("bench/baselines/tiny.json: cannot read: "), "{e}");
        }
        other => panic!("{other:?}"),
    }
    let loaded = perf::load(&r.full).unwrap();
    let text = perf::render_baseline(&corpus("v1.0"), &loaded).unwrap();
    f.write("repo/bench/baselines/tiny.json", &text.replace("\"shard.ok\": 1", "\"shard.ok\": 2"));
    let (res, out) = run_cmd(&repo, &["check", "tiny", s(&r.full)]);
    assert_eq!(res, Ok(false));
    assert!(out.starts_with("shard.ok: baseline 2, run 1\n"), "{out}");
    f.write(
        "repo/bench/baselines/tiny.json",
        &text.replace("\"corpus\": \"tiny\"", "\"corpus\": \"llvm\""),
    );
    let (res, _) = run_cmd(&repo, &["check", "tiny", s(&r.full)]);
    assert_eq!(
        res,
        Err(Failure::Error(
            "bench/baselines/tiny.json: the baseline is of corpus `llvm`, not `tiny`".into()
        ))
    );
    f.write("repo/bench/baselines/tiny.json", "{}");
    let (res, _) = run_cmd(&repo, &["check", "tiny", s(&r.full)]);
    assert_eq!(res, Err(Failure::Error("bench/baselines/tiny.json: no member `corpus`".into())));
}

// ------------------------------------------------------- the executable

fn xtask(a: &[&str]) -> (Option<i32>, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_xtask")).arg("perf").args(a).output().unwrap();
    assert!(out.stdout.is_empty(), "{}", String::from_utf8_lossy(&out.stdout));
    (out.status.code(), String::from_utf8(out.stderr).unwrap())
}

/// §7: usage errors exit 2 with `xtask perf: …` and the usage.
#[test]
fn usage_errors_exit_2() {
    for a in
        [&[][..], &["summarize"], &["record", "llvm"], &["check", "llvm", "a", "b"], &["bogus"]]
    {
        let (code, err) = xtask(a);
        assert_eq!(code, Some(2), "{a:?}: {err}");
        assert!(err.starts_with("xtask perf: "), "{err}");
        assert!(err.contains("\nusage: cargo xtask perf summarize"), "{err}");
    }
}

/// §7: an unknown corpus and an unreadable record exit 1 (the corpus is
/// looked up in this checkout's `bench/corpora.toml`).
#[test]
fn unknown_corpus_and_unreadable_record_exit_1() {
    let f = Fixture::empty("perf-bin-err");
    let missing = f.path(&format!("store/runs/{}/map.0.1.run", "ab".repeat(32)));
    let (code, err) = xtask(&["check", "no-such-corpus", s(&missing)]);
    assert_eq!(code, Some(1));
    assert!(
        err.starts_with("xtask perf: no corpus `no-such-corpus` in bench/corpora.toml (corpora: "),
        "{err}"
    );
    let (code, err) = xtask(&["summarize", s(&missing)]);
    assert_eq!(code, Some(1));
    assert!(err.starts_with(&format!("xtask perf: {}: cannot read: ", missing.display())), "{err}");
    let (code, err) = xtask(&["summarize", "not/a/record"]);
    assert_eq!(code, Some(1));
    assert!(err.starts_with("xtask perf: not/a/record: "), "{err}");
}
