//! `spec/exec.md` §8 through the built executable: the commands, their
//! output and every exit status of §8.5, over fixture projects written to
//! a temporary directory (self-contained sources: no system headers).
//!
//! Each test has its own directory and store. Every wait on a child
//! process or a condition is bounded; none is a fixed sleep.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use slip_exec::worker::{FRAME_MAX, PROTOCOL, ToCoordinator, read_frame};
use slip_model::{Digest, End, Plan, RunRecord, Shard, Status, formats, json};

const SLIP: &str = env!("CARGO_BIN_EXE_slip");

/// The bound on one `slip` command (a debug build parsing a few small
/// translation units takes well under a second each).
const BOUND: Duration = Duration::from_mins(2);

/// A fixture project, removed on drop.
struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Fixture {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("slip-cli-{}-{n}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        // Real path: the system temp directory may be a link, and the plan
        // records real paths.
        let root = std::fs::canonicalize(&root).unwrap();
        Fixture { root }
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    fn write(&self, rel: &str, text: &str) -> PathBuf {
        let p = self.path(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, text).unwrap();
        p
    }

    /// A compilation database at `rel` whose entries are `(file, command)`
    /// in the fixture's root.
    fn db(&self, rel: &str, entries: &[(&str, &str)]) -> PathBuf {
        let dir = json_string(self.root.to_str().unwrap());
        let items: Vec<String> = entries
            .iter()
            .map(|(file, command)| {
                format!(
                    "{{\"directory\": {dir}, \"file\": {}, \"command\": {}}}",
                    json_string(file),
                    json_string(command)
                )
            })
            .collect();
        self.write(rel, &format!("[\n{}\n]\n", items.join(",\n")))
    }

    /// A project of `n` clean translation units `src/u<i>.cpp`, plus a link
    /// step (`not_a_compile`), in `cdb.json`.
    fn clean_project(&self, n: usize) -> PathBuf {
        let mut entries = Vec::new();
        let mut owned = Vec::new();
        for i in 0..n {
            self.write(
                &format!("src/u{i}.cpp"),
                &format!("#include \"common.h\"\nint f{i}() {{ return common() + {i}; }}\n"),
            );
            owned.push((format!("src/u{i}.cpp"), format!("clang++ -std=c++17 -c src/u{i}.cpp")));
        }
        self.write("src/common.h", "#pragma once\ninline int common() { return 1; }\n");
        for (f, c) in &owned {
            entries.push((f.as_str(), c.as_str()));
        }
        entries.push(("app", "clang++ src/u0.o -o app"));
        self.db("cdb.json", &entries)
    }

    fn slip(&self, args: &[&str]) -> Out {
        let mut cmd = Command::new(SLIP);
        cmd.args(args).current_dir(&self.root);
        run_bounded(cmd)
    }

    fn store(&self) -> PathBuf {
        self.path("store")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn json_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// What a finished `slip` did.
#[derive(Debug)]
struct Out {
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

impl Out {
    fn code(&self) -> Option<i32> {
        self.status.code()
    }

    fn lines(&self) -> Vec<&str> {
        self.stdout.lines().collect()
    }

    /// The value of the stdout line `<key> <value>`.
    fn value(&self, key: &str) -> Option<&str> {
        self.stdout.lines().find_map(|l| l.strip_prefix(key)?.strip_prefix(' '))
    }

    fn plan_id(&self) -> String {
        self.value("plan").expect("a plan line").to_owned()
    }
}

/// Runs `cmd` with its output captured, killing it if it outlives
/// [`BOUND`].
fn run_bounded(mut cmd: Command) -> Out {
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().expect("start slip");
    let stdout = reader(child.stdout.take().unwrap());
    let stderr = reader(child.stderr.take().unwrap());
    let status = wait_bounded(&mut child, BOUND);
    Out { status, stdout: stdout.join().unwrap(), stderr: stderr.join().unwrap() }
}

fn reader(mut r: impl Read + Send + 'static) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = r.read_to_end(&mut bytes);
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

fn wait_bounded(child: &mut Child, bound: Duration) -> ExitStatus {
    let deadline = Instant::now() + bound;
    let mut pause = Duration::from_millis(1);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("slip did not finish within {bound:?}");
        }
        std::thread::sleep(pause);
        pause = (pause * 2).min(Duration::from_millis(25));
    }
}

/// Polls `condition` until it holds, at most `bound`.
fn wait_until(what: &str, bound: Duration, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + bound;
    let mut pause = Duration::from_millis(1);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out after {bound:?} waiting for {what}");
        std::thread::sleep(pause);
        pause = (pause * 2).min(Duration::from_millis(25));
    }
}

fn plan_in_store(store: &Path, id: &str) -> Plan {
    let bytes = std::fs::read(store.join("plans").join(format!("{id}.plan"))).unwrap();
    Plan::from_bytes(&bytes).expect("a valid plan")
}

fn excluded_lines(n: [u64; 6]) -> Vec<String> {
    [
        "not_a_compile",
        "unsupported_language",
        "unsupported_flag",
        "missing_file",
        "malformed_entry",
        "unsupported_compiler",
    ]
    .iter()
    .zip(n)
    .map(|(r, n)| format!("excluded {r} {n}"))
    .collect()
}

/// The summary lines of §8.2 must be these, in this order.
fn assert_summary(out: &Out, selected: u64, shards: [u64; 4], jobs: [u64; 5]) {
    let lines = out.lines();
    let start = lines.iter().position(|l| l.starts_with("selected ")).expect("selected line");
    let mut expected = vec![format!("selected {selected}")];
    for (name, n) in ["ok", "diagnosed", "rejected", "failed"].iter().zip(shards) {
        expected.push(format!("shard {name} {n}"));
    }
    for (name, n) in
        ["completed", "crashed", "killed_memory", "killed_time", "cancelled"].iter().zip(jobs)
    {
        expected.push(format!("job {name} {n}"));
    }
    let got: Vec<&str> = lines[start..start + expected.len()].to_vec();
    assert_eq!(got, expected, "{}", out.stdout);
    let rest = &lines[start + expected.len()..];
    let wall = rest[0].strip_prefix("wall ").expect("wall line");
    let (secs, millis) = wall.split_once('.').expect("three decimals");
    assert!(secs.parse::<u64>().is_ok() && millis.len() == 3, "{wall}");
    let run = rest.last().unwrap().strip_prefix("run ").expect("run line last");
    assert!(Path::new(run).is_file(), "{run}");
    if rest.len() == 3 {
        let peak = rest[1].strip_prefix("peak_rss ").expect("peak_rss line");
        let (bytes, main) = peak.split_once(' ').unwrap();
        assert!(bytes.parse::<u64>().unwrap() > 0, "{peak}");
        assert!(Path::new(main).is_file(), "{peak}");
    } else {
        assert_eq!(rest.len(), 2, "{}", out.stdout);
    }
}

/// The run record of `out` validates and accounts for `selected` observers;
/// `slip parse` runs round 0 and its record says so (`spec/model.md` §9).
fn run_record(out: &Out) -> RunRecord {
    let bytes = std::fs::read(out.value("run").unwrap()).unwrap();
    let record = RunRecord::from_bytes(&bytes).expect("the run record validates");
    record.validate().unwrap();
    assert_eq!(record.run.round, 0);
    record
}

// ----- plan (§8.1) --------------------------------------------------------------

// §8.1: the output lines; the plan is in the store; `dump` of it is JSON
// that reads back to the same bytes; status 0 with only `not_a_compile`
// excluded.
#[test]
fn s8_1_plan_output_store_and_dump() {
    let f = Fixture::new("plan");
    f.clean_project(3);
    let out = f.slip(&["plan", "--db", "cdb.json", "--store", "store", "-j", "2"]);
    assert_eq!(out.code(), Some(0), "{out:?}");
    let id = out.plan_id();
    assert!(Digest::from_hex(&id).is_some(), "{id}");
    let mut expected = vec![format!("plan {id}"), "observers 3".to_owned()];
    expected.extend(excluded_lines([1, 0, 0, 0, 0, 0]));
    assert_eq!(out.lines(), expected);

    let file = f.store().join("plans").join(format!("{id}.plan"));
    let plan = plan_in_store(&f.store(), &id);
    assert_eq!(plan.observers.len(), 3);
    assert_eq!(plan.tool_slip, env!("CARGO_PKG_VERSION"));

    let dumped = f.slip(&["dump", file.to_str().unwrap()]);
    assert_eq!(dumped.code(), Some(0), "{dumped:?}");
    let value = json::parse(dumped.stdout.as_bytes()).expect("dump is JSON");
    assert_eq!(formats::undump(&value).unwrap(), std::fs::read(&file).unwrap());
}

// §8.1: the plan does not depend on the number of hashing threads.
#[test]
fn s8_1_same_plan_for_any_j() {
    let f = Fixture::new("plan-j");
    f.clean_project(6);
    let one = f.slip(&["plan", "--db", "cdb.json", "--store", "store", "-j1"]);
    let four = f.slip(&["plan", "--db=cdb.json", "--store=store", "-j", "4"]);
    assert_eq!((one.code(), four.code()), (Some(0), Some(0)), "{one:?} {four:?}");
    assert_eq!(one.plan_id(), four.plan_id());
}

// §8.5: `plan` is 1 when an entry is excluded for a reason other than
// `not_a_compile`, and so is `parse --db`.
#[test]
fn s8_5_plan_findings() {
    let f = Fixture::new("plan-excluded");
    f.write("src/a.cpp", "int a() { return 0; }\n");
    f.write("src/b.f90", "end\n");
    f.db(
        "cdb.json",
        &[
            ("src/a.cpp", "clang++ -c src/a.cpp"),
            ("src/b.f90", "gfortran -c src/b.f90"),
            ("src/gone.cpp", "clang++ -c src/gone.cpp"),
        ],
    );
    let out = f.slip(&["plan", "--db", "cdb.json", "--store", "store"]);
    assert_eq!(out.code(), Some(1), "{out:?}");
    assert_eq!(out.value("observers"), Some("1"));
    let excluded: Vec<&str> =
        out.lines().into_iter().filter(|l| l.starts_with("excluded ")).collect();
    assert_eq!(excluded, excluded_lines([0, 1, 0, 1, 0, 0]), "{}", out.stdout);

    // `parse --db`: the plan's findings make it 1 although every shard is
    // `ok`.
    let out = f.slip(&["parse", "--db", "cdb.json", "--store", "store"]);
    assert_eq!(out.code(), Some(1), "{out:?}");
    assert_summary(&out, 1, [1, 0, 0, 0], [1, 0, 0, 0, 0]);

    // `not_a_compile` alone is not a finding.
    f.db("link.json", &[("src/a.cpp", "clang++ -c src/a.cpp"), ("app", "clang++ a.o -o app")]);
    let out = f.slip(&["plan", "--db", "link.json", "--store", "store"]);
    assert_eq!(out.code(), Some(0), "{out:?}");
    assert_eq!(out.value("excluded not_a_compile"), Some("1"));
}

// ----- parse (§8.2) -------------------------------------------------------------

// §8.2, §8.5: all `ok` is status 0 with every summary line; every selected
// observer has a shard that `dump` renders; the run record validates.
#[test]
fn s8_2_parse_db_all_ok() {
    let f = Fixture::new("parse-ok");
    f.clean_project(3);
    let out = f.slip(&["parse", "--db", "cdb.json", "--store", "store", "-j", "2"]);
    assert_eq!(out.code(), Some(0), "{out:?}");
    let id = out.plan_id();
    assert_eq!(out.lines().iter().filter(|l| l.starts_with("plan ")).count(), 1, "not repeated");
    assert_eq!(out.value("observers"), Some("3"));
    assert_summary(&out, 3, [3, 0, 0, 0], [3, 0, 0, 0, 0]);
    assert!(out.value("peak_rss").is_some(), "a worker reports its peak: {}", out.stdout);

    let plan = plan_in_store(&f.store(), &id);
    let record = run_record(&out);
    assert_eq!(record.jobs.len(), 3);
    assert!(record.jobs.iter().all(|j| j.end == End::Completed));
    for o in &plan.observers {
        let shard =
            f.store().join("shards").join(&id).join("map.0").join(format!("{}.shard", o.id));
        let s = Shard::from_bytes(&std::fs::read(&shard).unwrap()).unwrap();
        assert_eq!(s.outcome.status, Status::Ok);
        let dumped = f.slip(&["dump", shard.to_str().unwrap()]);
        assert_eq!(dumped.code(), Some(0), "{dumped:?}");
        assert!(dumped.stdout.contains("\"SHRD\""), "{}", dumped.stdout);
    }
    // The lock is gone and `tmp/` is empty after the run.
    assert!(!f.store().join("lock").exists());
    assert_eq!(std::fs::read_dir(f.store().join("tmp")).unwrap().count(), 0);
}

// §8.2, §8.5: one translation unit with an error is status 1, `shard
// diagnosed 1` and its `slip: diagnosed: <file>: …` line.
#[test]
fn s8_2_parse_db_diagnosed() {
    let f = Fixture::new("parse-diag");
    f.clean_project(2);
    let bad = f.write("src/bad.cpp", "int bad() { return undeclared_name; }\n");
    let cdb = std::fs::read_to_string(f.path("cdb.json")).unwrap();
    let dir = json_string(f.root.to_str().unwrap());
    let cdb = cdb.replacen(
        "[\n",
        &format!(
            "[\n{{\"directory\": {dir}, \"file\": \"src/bad.cpp\", \"command\": \"clang++ -c src/bad.cpp\"}},\n"
        ),
        1,
    );
    f.write("cdb.json", &cdb);
    let out = f.slip(&["parse", "--db", "cdb.json", "--store", "store"]);
    assert_eq!(out.code(), Some(1), "{out:?}");
    assert_summary(&out, 3, [2, 1, 0, 0], [3, 0, 0, 0, 0]);
    let line = format!("slip: diagnosed: {}: ", bad.display());
    let lines: Vec<&str> = out.stderr.lines().filter(|l| l.starts_with("slip: ")).collect();
    assert_eq!(lines.len(), 1, "{}", out.stderr);
    assert!(lines[0].starts_with(&line), "{}", out.stderr);
    assert!(lines[0].contains("undeclared_name"), "{}", out.stderr);
    run_record(&out);
}

// §8.2: `--plan` by id prefix and by file; an unknown and an ambiguous
// prefix are status 3.
#[test]
fn s8_2_parse_plan_by_prefix_and_file() {
    let f = Fixture::new("parse-plan");
    f.clean_project(2);
    let planned = f.slip(&["plan", "--db", "cdb.json", "--store", "store"]);
    assert_eq!(planned.code(), Some(0), "{planned:?}");
    let id = planned.plan_id();

    let out = f.slip(&["parse", "--plan", &id[..8], "--store", "store"]);
    assert_eq!(out.code(), Some(0), "{out:?}");
    assert_eq!(out.lines()[0], format!("plan {id}"));
    assert!(out.value("observers").is_none(), "no plan-building lines: {}", out.stdout);
    assert_summary(&out, 2, [2, 0, 0, 0], [2, 0, 0, 0, 0]);

    // By file, into a fresh store: read, validated, published.
    let copy = f.path("elsewhere/my.plan");
    std::fs::create_dir_all(copy.parent().unwrap()).unwrap();
    std::fs::copy(f.store().join("plans").join(format!("{id}.plan")), &copy).unwrap();
    let out = f.slip(&["parse", "--plan", copy.to_str().unwrap(), "--store", "store2"]);
    assert_eq!(out.code(), Some(0), "{out:?}");
    assert_eq!(out.plan_id(), id);
    assert!(f.path("store2/plans").join(format!("{id}.plan")).is_file());

    // Unknown: an id whose first digit differs.
    let flipped = format!("{}{}", if id.starts_with('0') { '1' } else { '0' }, &id[1..8]);
    let out = f.slip(&["parse", "--plan", &flipped, "--store", "store"]);
    assert_eq!(out.code(), Some(3), "{out:?}");
    assert_eq!(out.stderr.lines().count(), 1, "{}", out.stderr);
    assert!(out.stderr.starts_with(&format!("slip: plan {flipped}: ")), "{}", out.stderr);

    // Ambiguous: a second plan file whose name shares 63 digits.
    let last = id.chars().last().unwrap();
    let twin = format!("{}{}", &id[..63], if last == '0' { '1' } else { '0' });
    std::fs::copy(
        f.store().join("plans").join(format!("{id}.plan")),
        f.store().join("plans").join(format!("{twin}.plan")),
    )
    .unwrap();
    let out = f.slip(&["parse", "--plan", &id[..63], "--store", "store"]);
    assert_eq!(out.code(), Some(3), "{out:?}");
    assert_eq!(out.stderr.lines().count(), 1, "{}", out.stderr);
    assert!(out.stderr.contains(&id) && out.stderr.contains(&twin), "{}", out.stderr);

    // A file that is not a plan.
    let out = f.slip(&["parse", "--plan", "cdb.json", "--store", "store"]);
    assert_eq!(out.code(), Some(3), "{out:?}");
    assert!(out.stderr.starts_with("slip: cdb.json: "), "{}", out.stderr);
}

// §8.2: `--only` keeps observers whose main file contains a substring,
// `--limit` the first n in plan order; a selection that keeps nothing
// warns and is status 0.
#[test]
fn s8_2_only_and_limit() {
    let f = Fixture::new("parse-only");
    f.clean_project(3);
    let out = f.slip(&["parse", "--db", "cdb.json", "--store", "store", "--only", "u1.cpp"]);
    assert_eq!(out.code(), Some(0), "{out:?}");
    assert_summary(&out, 1, [1, 0, 0, 0], [1, 0, 0, 0, 0]);
    let record = run_record(&out);
    let plan = plan_in_store(&f.store(), &out.plan_id());
    let u1 = plan.observers.iter().find(|o| o.main.path.ends_with(b"u1.cpp")).unwrap();
    assert_eq!(record.jobs[0].observer, u1.id);

    let out = f.slip(&[
        "parse",
        "--db",
        "cdb.json",
        "--store",
        "store",
        "--only",
        "u0",
        "--only=u2",
        "--limit",
        "1",
    ]);
    assert_eq!(out.code(), Some(0), "{out:?}");
    assert_summary(&out, 1, [1, 0, 0, 0], [1, 0, 0, 0, 0]);
    let first = plan
        .observers
        .iter()
        .find(|o| o.main.path.ends_with(b"u0.cpp") || o.main.path.ends_with(b"u2.cpp"))
        .unwrap();
    assert_eq!(run_record(&out).jobs[0].observer, first.id, "the first in plan order");

    let out = f.slip(&["parse", "--db", "cdb.json", "--store", "store", "--only", "nothing"]);
    assert_eq!(out.code(), Some(0), "{out:?}");
    assert!(out.stderr.contains("slip: warning: the selection keeps none"), "{}", out.stderr);
    assert_summary(&out, 0, [0, 0, 0, 0], [0, 0, 0, 0, 0]);

    let out = f.slip(&["parse", "--db", "cdb.json", "--store", "store", "--limit", "0"]);
    assert_eq!(out.code(), Some(0), "{out:?}");
    assert_summary(&out, 0, [0; 4], [0; 5]);
}

// ----- exit statuses (§8.5) -----------------------------------------------------

/// Status 2: one line, then the usage text.
fn assert_usage(out: &Out, line: &str) {
    assert_eq!(out.code(), Some(2), "{out:?}");
    let mut lines = out.stderr.lines();
    let first = lines.next().unwrap();
    assert!(first.starts_with("slip: ") && first.contains(line), "{}", out.stderr);
    let rest: Vec<&str> = lines.collect();
    assert_eq!(rest.join("\n") + "\n", slip_cli::args::USAGE, "{}", out.stderr);
    assert!(out.stdout.is_empty());
}

// §8.5 status 2: an unknown command, an unknown option.
#[test]
fn s8_5_usage_errors() {
    let f = Fixture::new("usage");
    assert_usage(&f.slip(&["frobnicate"]), "unknown command `frobnicate`");
    assert_usage(&f.slip(&["plan", "--db", "x.json", "--frob"]), "unknown option `--frob`");
    assert_usage(&f.slip(&[]), "no command");
    assert_usage(&f.slip(&["plan", "--db", "a.json", "--db", "b.json"]), "need names");
}

/// Status 3: exactly one line, `slip: <what>: <why>`.
fn assert_input(out: &Out, starts: &str) {
    assert_eq!(out.code(), Some(3), "{out:?}");
    let lines: Vec<&str> = out.stderr.lines().collect();
    assert_eq!(lines.len(), 1, "{}", out.stderr);
    assert!(lines[0].starts_with(starts), "{starts:?}: {}", out.stderr);
}

// §8.5 status 3: a missing database, one that is not JSON, a directory
// that is not a store, a busy store, `dump` of a file that is not a
// container.
#[test]
fn s8_5_input_errors() {
    let f = Fixture::new("input");
    f.clean_project(1);
    assert_input(
        &f.slip(&["plan", "--db", "missing.json", "--store", "store"]),
        "slip: missing.json: ",
    );
    f.write("notjson.json", "this is not JSON\n");
    let out = f.slip(&["parse", "--db", "notjson.json", "--store", "store"]);
    assert_input(&out, "slip: notjson.json: ");
    assert!(out.stderr.contains("not JSON"), "{}", out.stderr);
    assert!(!f.store().join("lock").exists(), "the lock is removed on an error");

    f.write("other/file.txt", "something\n");
    let out = f.slip(&["plan", "--db", "cdb.json", "--store", "other"]);
    assert_input(&out, &format!("slip: {}: not a Slip store", f.path("other").display()));
    let out = f.slip(&["parse", "--db", "cdb.json", "--store", "other"]);
    assert_input(&out, "slip: ");
    assert!(out.stderr.contains("not a Slip store"), "{}", out.stderr);

    // A store whose lock names a live process: this test's own.
    let planned = f.slip(&["plan", "--db", "cdb.json", "--store", "store"]);
    assert_eq!(planned.code(), Some(0), "{planned:?}");
    std::fs::write(f.store().join("lock"), format!("{}\n", std::process::id())).unwrap();
    let out = f.slip(&["parse", "--db", "cdb.json", "--store", "store"]);
    assert_input(&out, "slip: ");
    assert!(
        out.stderr.contains("busy") && out.stderr.contains(&std::process::id().to_string()),
        "{}",
        out.stderr
    );
    assert_eq!(
        std::fs::read_to_string(f.store().join("lock")).unwrap(),
        format!("{}\n", std::process::id()),
        "a busy store's lock is left alone"
    );
    // `plan` opens the store shared: the lock does not stop it.
    assert_eq!(f.slip(&["plan", "--db", "cdb.json", "--store", "store"]).code(), Some(0));

    assert_input(&f.slip(&["dump", "cdb.json"]), "slip: cdb.json: ");
    assert_input(&f.slip(&["dump", "no-such-file"]), "slip: no-such-file: ");
}

// §8.5 status 70: a panic in the coordinator prints exactly one line.
#[test]
fn s8_5_internal_error() {
    let f = Fixture::new("panic");
    let out = f.slip(&["__panic"]);
    assert_eq!(out.code(), Some(70), "{out:?}");
    assert_eq!(out.stderr.lines().count(), 1, "{}", out.stderr);
    let line = out.stderr.trim_end();
    assert!(line.starts_with("slip: internal error: "), "{line}");
    assert!(line.ends_with(')') && line.contains(".rs:"), "<message> (<file>:<line>): {line}");
    assert!(out.stdout.is_empty());
}

// §8: `--version`.
#[test]
fn s8_version() {
    let f = Fixture::new("version");
    let out = f.slip(&["--version"]);
    assert_eq!(out.code(), Some(0), "{out:?}");
    assert!(out.stdout.starts_with(&format!("slip {} (kiln ", env!("CARGO_PKG_VERSION"))));
}

// §8: `--help` prints the usage text on standard output with status 0; a
// usage error prints it on standard error with status 2.
#[test]
fn s8_help() {
    let f = Fixture::new("help");
    for flag in ["--help", "-h"] {
        let out = f.slip(&[flag]);
        assert_eq!(out.code(), Some(0), "{out:?}");
        assert_eq!(out.stdout, slip_cli::args::USAGE);
        assert!(out.stderr.is_empty(), "{out:?}");
    }
}

// ----- the worker (§8.4) --------------------------------------------------------

// §8.4, §5.2: `slip __worker` with its input closed at once prints one
// `Ready` frame and exits 0.
#[test]
fn s8_4_worker_ready_then_exit() {
    let f = Fixture::new("worker");
    let mut cmd = Command::new(SLIP);
    cmd.arg("__worker").current_dir(&f.root);
    let out = run_bounded(cmd);
    assert_eq!(out.code(), Some(0), "{out:?}");
    // Read the raw bytes again: `Out` decodes lossily.
    let mut cmd = Command::new(SLIP);
    cmd.arg("__worker").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    let mut child = cmd.spawn().unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let frames = std::thread::spawn(move || {
        let mut frames = Vec::new();
        while let Some(frame) = read_frame(&mut stdout, FRAME_MAX).unwrap() {
            frames.push(frame);
        }
        frames
    });
    assert!(wait_bounded(&mut child, BOUND).success());
    let frames = frames.join().unwrap();
    assert_eq!(frames.len(), 1, "one frame");
    match ToCoordinator::decode(&frames[0]).unwrap() {
        ToCoordinator::Ready { protocol, tool } => {
            assert_eq!(protocol, PROTOCOL);
            assert_eq!(tool, slip_cli::version_line());
        }
        other => panic!("{other:?}"),
    }
}

// ----- an interrupted parse (§7, §8.2) ------------------------------------------

/// The pids whose parent is `parent` and whose command line contains
/// `word`.
fn children_running(parent: u32, word: &str) -> Vec<u32> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir("/proc") else { return out };
    for e in entries.flatten() {
        let Some(pid) = e.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else {
            continue;
        };
        let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else { continue };
        // `pid (comm) state ppid …`: comm may hold spaces, so split after `)`.
        let Some(after) = stat.rsplit_once(')').map(|(_, a)| a) else { continue };
        let parent_pid = after.split_whitespace().nth(1).and_then(|p| p.parse::<u32>().ok());
        let cmdline = std::fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
        if parent_pid == Some(parent) && String::from_utf8_lossy(&cmdline).contains(word) {
            out.push(pid);
        }
    }
    out
}

/// Whether `pid` is gone (or a zombie nobody has reaped yet).
fn gone(pid: u32) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Err(_) => true,
        Ok(stat) => stat.rsplit_once(')').is_some_and(|(_, a)| a.trim_start().starts_with('Z')),
    }
}

/// A thread of `pid` whose kernel wait channel is `channel`.
fn waits_in(pid: u32, channel: &str) -> bool {
    let Ok(tasks) = std::fs::read_dir(format!("/proc/{pid}/task")) else { return false };
    tasks
        .flatten()
        .any(|t| std::fs::read_to_string(t.path().join("wchan")).is_ok_and(|w| w.trim() == channel))
}

// §7, §8.2: one translation unit includes a named pipe, so its worker
// blocks; `SIGINT` then ends `slip` by that signal, no worker is left, the
// store has no lock and an empty `tmp/`, and a run record with a
// `cancelled` job exists. The worker has none of `slip`'s `KILN_*`
// variables.
#[cfg(target_os = "linux")]
#[test]
fn s7_interrupted_parse() {
    use std::os::unix::process::ExitStatusExt as _;

    let f = Fixture::new("sigint");
    f.write("src/a.cpp", "#include \"pipe.h\"\nint a() { return 0; }\n");
    let made = Command::new("mkfifo").arg(f.path("src/pipe.h")).status().expect("mkfifo");
    assert!(made.success());
    f.db("cdb.json", &[("src/a.cpp", "clang++ -std=c++17 -c src/a.cpp")]);

    let mut cmd = Command::new(SLIP);
    cmd.args(["parse", "--db", "cdb.json", "--store", "store", "-j1"])
        .env("KILN_SLIP_CLI_TEST", "1")
        .current_dir(&f.root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut slip = cmd.spawn().unwrap();
    let stdout = reader(slip.stdout.take().unwrap());
    let stderr = reader(slip.stderr.take().unwrap());
    let pid = slip.id();

    // The worker is running, and blocked opening the pipe when the kernel
    // says so (`wait_for_partner`; some kernels hide wait channels).
    let mut worker = 0;
    wait_until("the worker", Duration::from_mins(1), || {
        worker = children_running(pid, "__worker").first().copied().unwrap_or(0);
        worker != 0
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    while !waits_in(worker, "wait_for_partner") && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!gone(worker), "the worker blocks on the pipe");
    // The brief's worker environment: no `KILN_*` variable of the
    // coordinator reaches the worker.
    let environ = std::fs::read(format!("/proc/{worker}/environ")).unwrap();
    assert!(environ.split(|&b| b == 0).any(|v| v.starts_with(b"PATH=")), "environ is readable");
    assert!(!environ.split(|&b| b == 0).any(|v| v.starts_with(b"KILN_")), "no KILN_ variable");

    let sent = Command::new("kill").args(["-INT", &pid.to_string()]).status().unwrap();
    assert!(sent.success());
    let status = wait_bounded(&mut slip, Duration::from_secs(30));
    let (stdout, stderr) = (stdout.join().unwrap(), stderr.join().unwrap());
    assert_eq!(status.signal(), Some(2), "killed by SIGINT: {status:?}\n{stdout}\n{stderr}");
    assert!(!stdout.contains("selected "), "no summary: {stdout}");
    assert!(stderr.contains("slip: signal 2: "), "{stderr}");

    wait_until("the worker to be gone", Duration::from_secs(10), || gone(worker));
    let store = f.store();
    assert!(!store.join("lock").exists(), "no lock");
    let tmp: Vec<_> =
        std::fs::read_dir(store.join("tmp")).unwrap().flatten().map(|e| e.path()).collect();
    assert!(tmp.is_empty(), "an empty tmp/: {tmp:?}");
    let plan_id = stdout.lines().find_map(|l| l.strip_prefix("plan ")).unwrap();
    let runs: Vec<_> =
        std::fs::read_dir(store.join("runs").join(plan_id)).unwrap().flatten().collect();
    assert_eq!(runs.len(), 1, "{runs:?}");
    let record = RunRecord::from_bytes(&std::fs::read(runs[0].path()).unwrap()).unwrap();
    assert_eq!(record.jobs.len(), 1);
    assert_eq!(record.jobs[0].end, End::Cancelled);
    assert_eq!(record.jobs[0].shard, None);
}
