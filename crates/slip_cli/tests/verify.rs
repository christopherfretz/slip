//! `spec/exec.md` §8.7 to §8.9 and `spec/verify.md` §5, §7 and §7.1
//! through the built executable: `slip verify`, `slip declare` and
//! `slip units` over trees and compilation databases the tests write.
//!
//! Every verdict is asserted from the report file and the exit status
//! both, never from the text alone; "this run's shard" from the run
//! record (`spec/verify.md` §7). Each test has its own directory and
//! store, under the target directory, removed on drop. Every wait on a
//! child is bounded.
//!
//! Sources are self-contained (no system header) and use only constructs
//! the walk digests today (`docs/wp/WP-052.md` Handback: plain functions,
//! structs, namespaces, calls, overloads, enums), except where a test is
//! about an indeterminate unit.

use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use slip_model::declared::Declared;
use slip_model::formats::content_id;
use slip_model::json::{self, Value};
use slip_model::transform::Ownership;
use slip_model::verdict::{FindingKind, VerdictKind};
use slip_model::{Digest, Plan, RunRecord, Shard, Status};

const SLIP: &str = env!("CARGO_BIN_EXE_slip");

/// The bound on one `slip` command.
const BOUND: Duration = Duration::from_mins(3);

/// A subject tree `s/`, a post tree `post/`, a compilation database over
/// `s/` and a store, in a directory of their own.
struct Fixture {
    root: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
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

    /// The value of the stdout line `<key> <value>`.
    fn value(&self, key: &str) -> Option<&str> {
        self.stdout.lines().find_map(|l| l.strip_prefix(key)?.strip_prefix(' '))
    }

    fn last_line(&self) -> &str {
        self.stdout.lines().last().unwrap_or("")
    }

    fn stderr_lines(&self) -> Vec<&str> {
        self.stderr.lines().collect()
    }
}

impl Fixture {
    fn new(name: &str) -> Fixture {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("wp053-{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("s")).unwrap();
        std::fs::create_dir_all(root.join("post")).unwrap();
        let root = std::fs::canonicalize(&root).unwrap();
        Fixture { root }
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    fn text(p: &Path) -> String {
        p.to_str().unwrap().to_owned()
    }

    fn write(&self, rel: &str, text: &str) -> PathBuf {
        let p = self.path(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, text).unwrap();
        p
    }

    /// `rel` in both trees.
    fn both(&self, rel: &str, text: &str) {
        self.write(&format!("s/{rel}"), text);
        self.write(&format!("post/{rel}"), text);
    }

    /// `cdb.json`: one entry per file of `s/`, compiled from `s/`.
    fn db(&self, files: &[&str]) {
        let dir = Fixture::text(&self.path("s"));
        let items: Vec<String> = files
            .iter()
            .map(|f| {
                format!(
                    "{{\"directory\": \"{dir}\", \"file\": \"{f}\", \"command\": \"clang++ \
                     -std=c++17 -c {f}\"}}"
                )
            })
            .collect();
        self.write("cdb.json", &format!("[\n{}\n]\n", items.join(",\n")));
    }

    fn slip(&self, args: &[&str]) -> Out {
        let mut cmd = Command::new(SLIP);
        cmd.args(args).current_dir(&self.root);
        run_bounded(cmd)
    }

    /// `slip parse --db cdb.json` and the plan's id; `extra` are more options.
    fn parse_with(&self, extra: &[&str]) -> String {
        let mut args = vec!["parse", "--db", "cdb.json", "--store", "store", "-j", "2"];
        args.extend(extra);
        let out = self.slip(&args);
        assert!(matches!(out.code(), Some(0 | 1)), "{out:?}");
        out.value("plan").expect("a plan line").to_owned()
    }

    fn parse(&self) -> String {
        self.parse_with(&[])
    }

    /// `slip verify --plan <plan> --store store -j 2 <extra>`.
    fn verify(&self, plan: &str, extra: &[&str]) -> Out {
        let mut args = vec!["verify", "--plan", plan, "--store", "store", "-j", "2"];
        args.extend(extra);
        self.slip(&args)
    }

    /// `verify` with `--report report.json` and the report read back.
    fn verify_report(&self, plan: &str, extra: &[&str]) -> (Out, Value) {
        let mut args = extra.to_vec();
        args.extend(["--report", "report.json"]);
        let out = self.verify(plan, &args);
        let report = json::parse(&std::fs::read(self.path("report.json")).unwrap())
            .unwrap_or_else(|e| panic!("the report is JSON: {e}\n{out:?}"));
        (out, report)
    }

    fn plan(&self, id: &str) -> Plan {
        let bytes = std::fs::read(self.path("store/plans").join(format!("{id}.plan"))).unwrap();
        Plan::from_bytes(&bytes).unwrap()
    }

    /// The observer ids of the plan, in plan order, with their main files.
    fn observers(&self, plan: &str) -> Vec<(Digest, String)> {
        self.plan(plan)
            .observers
            .iter()
            .map(|o| (o.id, String::from_utf8(o.main.path.clone()).unwrap()))
            .collect()
    }

    fn shard_path(&self, plan: &str, phase: &str, round: u64, observer: &Digest) -> PathBuf {
        self.path("store/shards")
            .join(plan)
            .join(format!("{phase}.{round}"))
            .join(format!("{observer}.shard"))
    }

    fn shard(&self, plan: &str, phase: &str, round: u64, observer: &Digest) -> (Shard, Digest) {
        let path = self.shard_path(plan, phase, round, observer);
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        (Shard::from_bytes(&bytes).unwrap(), content_id(&bytes).unwrap())
    }

    /// The run record a `verify` or `parse` printed (`run <path>`).
    fn run_record(out: &Out) -> RunRecord {
        let path = out.value("run").expect("a run line");
        RunRecord::from_bytes(&std::fs::read(path).unwrap()).unwrap()
    }

    /// The run records of phase `verify` in the store.
    fn verify_runs(&self, plan: &str) -> usize {
        std::fs::read_dir(self.path("store/runs").join(plan)).map_or(0, |d| {
            d.filter(|e| e.as_ref().unwrap().file_name().to_str().unwrap().starts_with("verify."))
                .count()
        })
    }
}

fn run_bounded(mut cmd: Command) -> Out {
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().expect("start slip");
    let stdout = reader(child.stdout.take().unwrap());
    let stderr = reader(child.stderr.take().unwrap());
    let status = wait_bounded(&mut child);
    Out { status, stdout: stdout.join().unwrap(), stderr: stderr.join().unwrap() }
}

fn reader(mut r: impl Read + Send + 'static) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = r.read_to_end(&mut bytes);
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

fn wait_bounded(child: &mut Child) -> ExitStatus {
    let deadline = Instant::now() + BOUND;
    let mut pause = Duration::from_millis(1);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("slip did not finish within {BOUND:?}");
        }
        std::thread::sleep(pause);
        pause = (pause * 2).min(Duration::from_millis(25));
    }
}

// ----- reading the report -------------------------------------------------------

fn get<'v>(v: &'v Value, path: &str) -> &'v Value {
    path.split('.').fold(v, |v, k| v.get(k).unwrap_or_else(|| panic!("no `{path}` in the report")))
}

fn str_at<'v>(v: &'v Value, path: &str) -> &'v str {
    get(v, path).as_str().unwrap_or_else(|| panic!("`{path}` is not a string"))
}

fn u64_at(v: &Value, path: &str) -> u64 {
    get(v, path).as_u64().unwrap_or_else(|| panic!("`{path}` is not a number"))
}

/// The seven state counts of `observers`, in the order of `spec/verify.md`
/// §7's table.
fn states(report: &Value) -> [u64; 7] {
    let names = [
        "verified",
        "unchanged",
        "mismatch",
        "not_parsed",
        "unverifiable",
        "never_parsed",
        "failed",
    ];
    names.map(|n| u64_at(report, &format!("observers.{n}")))
}

fn groups(report: &Value) -> Vec<(String, String)> {
    get(report, "findings.groups")
        .as_array()
        .unwrap()
        .iter()
        .map(|g| (str_at(g, "kind").to_owned(), str_at(g, "name").to_owned()))
        .collect()
}

/// The verdict is the report's and the exit status agrees with it
/// (`spec/exec.md` §8.7): 0 verified, 1 not verified, 4 incomplete; the
/// text's last line says the same.
fn assert_verdict(out: &Out, report: &Value, verdict: &str) {
    assert_eq!(str_at(report, "verdict"), verdict, "{out:?}");
    let (code, line) = match verdict {
        "verified" => (0, "verdict verified"),
        "not_verified" => (1, "verdict not verified"),
        "incomplete" => (4, "verdict incomplete"),
        other => panic!("no verdict {other}"),
    };
    assert_eq!(out.code(), Some(code), "{out:?}");
    assert_eq!(out.last_line(), line, "{out:?}");
}

fn counter(shard: &Shard, name: &str) -> Option<u64> {
    shard.counters.iter().find(|c| c.name == name).map(|c| c.value)
}

/// A finding that is a fact about the program, not about what the walk
/// could not digest.
fn real(k: FindingKind) -> bool {
    !matches!(k, FindingKind::Indeterminate | FindingKind::DerivedIndeterminate)
}

// ----- the trees ---------------------------------------------------------------

const HEADER: &str = "#pragma once\n\
namespace old {\n\
struct S { int v; };\n\
int g(S *s);\n\
enum E { e1, e2 };\n\
}\n\
int h(int);\n\
int h(char);\n";

const U0: &str = "#include \"a.h\"\nint f0(old::S *s) { return old::g(s) + h(1); }\n";
const U1: &str = "#include \"a.h\"\nint f1() { return h(2) + old::e2; }\n";

/// Two observers over a shared header, the same in both trees.
fn two_units(name: &str) -> Fixture {
    let f = Fixture::new(name);
    f.both("a.h", HEADER);
    f.both("u0.cpp", U0);
    f.both("u1.cpp", U1);
    f.db(&["u0.cpp", "u1.cpp"]);
    f
}

/// The post tree with `old` relocated to `fresh` (every spelling).
fn relocate_in_post(f: &Fixture) {
    f.write("post/a.h", &HEADER.replace("namespace old", "namespace fresh"));
    f.write("post/u0.cpp", &U0.replace("old::", "fresh::"));
    f.write("post/u1.cpp", &U1.replace("old::", "fresh::"));
}

fn image<'a>(f: &'a Fixture, s: &'a mut Vec<String>) -> Vec<&'a str> {
    s.clear();
    s.push(Fixture::text(&f.path("s")));
    s.push(Fixture::text(&f.path("post")));
    vec!["--root", &s[0], "--post", &s[1]]
}

// ----- verify: states, verdicts and statuses ---------------------------------------

// spec/verify.md §7, §7.1; spec/exec.md §8.7: the identity, nothing
// declared: every observer verified at the sums with one parse
// (`verify.reparsed` 0); status 0; nothing on standard error; the post
// of every job as a worker sees it: `pre` 0 (the verdict compares
// against the map shard of round 0, its content id) and the declared
// changes the command published (the rules' id of the run).
#[test]
fn s7_identity_verified_one_parse_each() {
    let f = two_units("identity");
    let plan = f.parse();
    let (out, report) = f.verify_report(&plan, &[]);
    assert_verdict(&out, &report, "verified");
    assert_eq!(states(&report), [2, 0, 0, 0, 0, 0, 0]);
    assert_eq!((u64_at(&report, "observers.plan"), u64_at(&report, "observers.selected")), (2, 2));
    assert_eq!(out.stderr, "");
    assert_eq!(out.value("selected"), Some("2 of 2"));
    assert_eq!(out.value("image"), Some("identity"));
    let none = Declared::none();
    assert_eq!(str_at(&report, "declared"), content_id(&none.to_bytes()).unwrap().to_string());
    assert_eq!(str_at(&report, "rules"), none.rules().id().to_string());
    let record = Fixture::run_record(&out);
    for (o, _) in f.observers(&plan) {
        let (shard, id) = f.shard(&plan, "verify", 0, &o);
        let (_, map_id) = f.shard(&plan, "map", 0, &o);
        let v = shard.verdict.as_ref().unwrap();
        assert_eq!(v.kind, VerdictKind::Verified);
        assert_eq!(v.pre, map_id, "the job's pre is round 0 of the map phase");
        assert_eq!(v.rules, none.rules().id());
        assert_eq!(counter(&shard, "verify.reparsed"), Some(0), "one parse");
        // This run's shard: the content id of the observer's last entry.
        let last = record.jobs.iter().rfind(|j| j.observer == o).unwrap();
        assert_eq!(last.shard, Some(id));
    }
}

// spec/verify.md §7, §7.1, §5.2 steps 4–6: a changed callee in one
// observer: `mismatch` there, `verified` in the other; status 1; the
// finding named; one line on standard error; the text and the JSON
// against goldens after ids, paths and times are masked.
#[test]
fn s7_changed_callee_mismatch_localised() {
    let f = two_units("callee");
    let plan = f.parse();
    f.write("post/u0.cpp", &U0.replace("h(1)", "h('1')"));
    let mut s = Vec::new();
    let (out, report) = f.verify_report(&plan, &image(&f, &mut s));
    assert_verdict(&out, &report, "not_verified");
    assert_eq!(states(&report), [1, 0, 1, 0, 0, 0, 0]);
    let g = groups(&report);
    assert!(g.iter().any(|(k, n)| k == "changed" && n == "f0(old::S *)"), "{g:?}");
    let u0 = Fixture::text(&f.path("s/u0.cpp"));
    let nv = get(&report, "not_verified").as_array().unwrap();
    assert_eq!(nv.len(), 1);
    assert_eq!((str_at(&nv[0], "main"), str_at(&nv[0], "state")), (u0.as_str(), "mismatch"));
    let total = u64_at(&nv[0], "findings");
    assert!(total > 0);
    assert_eq!(
        out.stderr_lines(),
        [format!("slip: mismatch: {u0}: {total} finding(s), 0 indeterminate")]
    );
    let (o0, _) = f.observers(&plan)[0].clone();
    let (shard, _) = f.shard(&plan, "verify", 0, &o0);
    assert_eq!(counter(&shard, "verify.reparsed"), Some(2), "pre-image and localisation");
    golden("mismatch.txt", &mask(&out.stdout, &f.root));
    golden(
        "mismatch.json",
        &mask(&std::fs::read_to_string(f.path("report.json")).unwrap(), &f.root),
    );
}

// spec/verify.md §7 `not_parsed`: the post-image does not parse where the
// pre-image did. Status 1.
#[test]
fn s7_not_parsed() {
    let f = two_units("not-parsed");
    let plan = f.parse();
    f.write("post/u1.cpp", "#include \"a.h\"\nint f1() { return h(2) +; }\n");
    let mut s = Vec::new();
    let (out, report) = f.verify_report(&plan, &image(&f, &mut s));
    assert_verdict(&out, &report, "not_verified");
    assert_eq!(states(&report), [1, 0, 0, 1, 0, 0, 0]);
    let u1 = Fixture::text(&f.path("s/u1.cpp"));
    let lines = out.stderr_lines();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].starts_with(&format!("slip: not_parsed: {u1}: ")), "{lines:?}");
}

// spec/verify.md §7 `never_parsed`: the pre-image did not parse (its map
// shard is `diagnosed`) and neither does the post-image; nothing is known
// to have changed: incomplete, status 4.
#[test]
fn s7_never_parsed_is_incomplete() {
    let f = two_units("never-parsed");
    f.both("u1.cpp", "#include \"a.h\"\nint f1() { return h(2) +; }\n");
    let plan = f.parse();
    let (out, report) = f.verify_report(&plan, &[]);
    assert_verdict(&out, &report, "incomplete");
    assert_eq!(states(&report), [1, 0, 0, 0, 0, 1, 0]);
    let u1 = Fixture::text(&f.path("s/u1.cpp"));
    assert!(out.stderr.starts_with(&format!("slip: never_parsed: {u1}: ")), "{out:?}");
}

// spec/verify.md §7 `unverifiable`, §5.3 P1: an observer the map phase
// did not run over has no pre shard. Incomplete, status 4, the reason on
// standard error.
#[test]
fn s7_unverifiable_without_a_map_shard() {
    let f = two_units("unverifiable");
    let plan = f.parse_with(&["--only", "u0.cpp"]);
    let (out, report) = f.verify_report(&plan, &[]);
    assert_verdict(&out, &report, "incomplete");
    assert_eq!(states(&report), [1, 0, 0, 0, 1, 0, 0]);
    let u1 = Fixture::text(&f.path("s/u1.cpp"));
    assert_eq!(out.stderr_lines(), [format!("slip: unverifiable: {u1}: pre-image: no map shard")]);
}

// spec/verify.md §7 `failed`, spec/exec.md §5.4: a worker killed for its
// memory cap leaves a `failed` shard; the run is incomplete, status 4.
#[test]
fn s7_failed_by_the_memory_cap() {
    let f = two_units("failed");
    let plan = f.parse();
    let (out, report) =
        f.verify_report(&plan, &["--memory", "16G", "--observer-memory", "1M", "--only", "u1"]);
    assert_verdict(&out, &report, "incomplete");
    assert_eq!(states(&report), [0, 0, 0, 0, 0, 0, 1]);
    let u1 = Fixture::text(&f.path("s/u1.cpp"));
    assert!(out.stderr.contains(&format!("slip: failed: {u1}: ")), "{out:?}");
    let (o1, _) = f.observers(&plan)[1].clone();
    let (shard, id) = f.shard(&plan, "verify", 0, &o1);
    assert_eq!(shard.outcome.status, Status::Failed);
    let last = Fixture::run_record(&out).jobs.into_iter().rfind(|j| j.observer == o1).unwrap();
    assert_eq!(last.shard, Some(id), "the failed shard is this run's");
}

// spec/exec.md §8.7: a run over an empty selection is incomplete, with the
// warning of §8.2.
#[test]
fn s8_7_empty_selection_is_incomplete() {
    let f = two_units("empty");
    let plan = f.parse();
    let (out, report) = f.verify_report(&plan, &["--only", "nothing-matches"]);
    assert_verdict(&out, &report, "incomplete");
    assert_eq!(states(&report), [0; 7]);
    assert_eq!(out.value("selected"), Some("0 of 2"));
    assert_eq!(
        out.stderr,
        "slip: warning: the selection keeps none of the plan's 2 observer(s); the phase runs \
         over nothing\n"
    );
}

// spec/exec.md §8.7 `--round`: the pre shards are always round 0 of the
// map phase; a round run again has its shards replaced and its report
// names the round.
#[test]
fn s8_7_a_round_run_again() {
    let f = two_units("round");
    let plan = f.parse();
    let (out, report) = f.verify_report(&plan, &["--round", "5"]);
    assert_verdict(&out, &report, "verified");
    assert_eq!(u64_at(&report, "round"), 5);
    assert_eq!(out.value("round"), Some("5"));
    let (o0, _) = f.observers(&plan)[0].clone();
    let (_, first) = f.shard(&plan, "verify", 5, &o0);
    // Again, with a mismatch this time: the shard of round 5 is replaced.
    f.write("post/u0.cpp", &U0.replace("h(1)", "h('1')"));
    let mut s = Vec::new();
    let mut args = image(&f, &mut s);
    args.extend(["--round", "5"]);
    let (out, report) = f.verify_report(&plan, &args);
    assert_verdict(&out, &report, "not_verified");
    let (shard, second) = f.shard(&plan, "verify", 5, &o0);
    assert_ne!(first, second);
    assert_eq!(shard.verdict.unwrap().kind, VerdictKind::Mismatch);
    assert!(!f.shard_path(&plan, "verify", 0, &o0).exists(), "round 0 was never run");
}

// spec/exec.md §8.7 `--findings`, `--locate`: they reach every job (the
// worker's command line) and the report states them.
#[test]
fn s8_7_findings_and_locate() {
    let f = two_units("limits");
    let plan = f.parse();
    relocate_in_post(&f);
    let mut s = Vec::new();
    let mut args = image(&f, &mut s);
    args.extend(["--findings", "1", "--locate", "0", "--only", "u0"]);
    let (out, report) = f.verify_report(&plan, &args);
    assert_verdict(&out, &report, "not_verified");
    assert_eq!((u64_at(&report, "limit"), u64_at(&report, "locate")), (1, 0));
    assert_eq!(u64_at(&report, "findings.listed"), 1);
    assert!(u64_at(&report, "findings.total") > 1);
    // `--locate 0`: no name, and no third parse.
    assert_eq!(groups(&report)[0].1, "");
    let (o0, _) = f.observers(&plan)[0].clone();
    let (shard, _) = f.shard(&plan, "verify", 0, &o0);
    assert_eq!(counter(&shard, "verify.reparsed"), Some(1));
    assert_eq!(shard.verdict.unwrap().findings.len(), 1);
}

// spec/exec.md §8.7 `--report`: written to a file beside it and renamed
// over what was there; nothing else is left in its directory.
#[test]
fn s8_7_report_replaced_whole() {
    let f = two_units("report");
    let plan = f.parse();
    std::fs::create_dir_all(f.path("out")).unwrap();
    f.write("out/r.json", "old contents");
    let out = f.verify(&plan, &["--report", "out/r.json"]);
    assert_eq!(out.code(), Some(0), "{out:?}");
    assert_eq!(out.value("report"), Some("out/r.json"));
    let report = json::parse(&std::fs::read(f.path("out/r.json")).unwrap()).unwrap();
    assert_eq!(str_at(&report, "verdict"), "verified");
    assert_eq!(str_at(&report, "run"), out.value("run").unwrap());
    let names: Vec<_> =
        std::fs::read_dir(f.path("out")).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(names, ["r.json"]);
    // Replaced, not rewritten: the path names a new file afterwards, and a
    // second name of the old file still names the old one. (Compared by
    // inode: the contents read back through the host's shared mount can
    // be stale for a moment after a rename.)
    let ino = |name: &str| std::fs::metadata(f.path(name)).unwrap().ino();
    std::fs::hard_link(f.path("out/r.json"), f.path("out/old.json")).unwrap();
    let before = ino("out/r.json");
    let second = f.verify(&plan, &["--report", "out/r.json"]);
    assert_eq!(second.code(), Some(0), "{second:?}");
    assert_ne!(ino("out/r.json"), before, "a new file was renamed over the path");
    assert_eq!(ino("out/old.json"), before);
    // Without `--report`, no `report` line.
    let out = f.verify(&plan, &[]);
    assert_eq!(out.value("report"), None);
}

// ----- verify: declared changes ---------------------------------------------------

/// `slip declare --owned s` over `relocate old => :: <to>` (a namespace
/// rule from the global namespace to the global namespace), written to
/// `name`: the run's files are the subject tree (`spec/verify.md` §2,
/// §2.1, §2.3).
fn relocation(f: &Fixture, plan: &str, name: &str, to: &str) -> Declared {
    f.write("reloc.txt", &format!("relocate old => :: {to}\n"));
    let mut s = Vec::new();
    let mut args = vec![
        "declare",
        "--plan",
        plan,
        "--store",
        "store",
        "--from",
        "reloc.txt",
        "--out",
        name,
        "--owned",
        "s",
    ];
    args.extend(image(f, &mut s));
    let out = f.slip(&args);
    assert_eq!(out.code(), Some(0), "{out:?}");
    let declared = Declared::from_bytes(&std::fs::read(f.path(name)).unwrap()).unwrap();
    assert_eq!(declared.rules().rules().len(), 1);
    declared
}

// spec/verify.md §2.1, §5.2 step 1: the post-image is digested WITH the
// declared rules. A relocation through the whole path (`slip parse`, an
// edited copy of the tree, the declared file, `slip verify --root --post
// --declared`). The file is made here (`relocation`), as `slip declare`
// makes it from `relocate old => :: fresh` once it gives the run's files
// (WP-111; until then it cannot write a rule: a version 2 `DECL` with
// rules must have `files`). With the rule every observer's verdict is `verified`
// with no finding at all; without `--declared`, or with a wrong rule, the
// relocated entities show as removed and added. One callee changed on top
// of the relocation shows a finding that is not indeterminate.
//
// The run's report under rules (WP-121 writes `binding` and `rule_use`):
// both observers `verified`, the run `verified`, status 0. WP-111: no
// observer is stale for the namespace rule (spec/verify.md §9.2 "Stale for
// a namespace rule": nothing owned is left in `old`, nothing is kept),
// and the report no longer has the `unchecked` member or line (until
// WP-111 it listed the rule there, the verdict unchanged).
#[test]
fn s2_relocation_through_the_whole_path() {
    let f = two_units("relocation");
    let plan = f.parse();
    relocate_in_post(&f);
    let mut s = Vec::new();
    let img = image(&f, &mut s);
    let declared = relocation(&f, &plan, "d.decl", "fresh");
    let observers = f.observers(&plan);

    // With the rule.
    let mut with = img.clone();
    with.extend(["--declared", "d.decl"]);
    let (out, report) = f.verify_report(&plan, &with);
    assert_eq!(str_at(&report, "rules"), declared.rules_id().to_string());
    for (o, main) in &observers {
        let (shard, _) = f.shard(&plan, "verify", 0, o);
        let v = shard.verdict.as_ref().unwrap();
        assert_eq!(v.kind, VerdictKind::Verified, "{main}: {:?}", v.findings);
        assert_eq!((v.total, v.rules), (0, declared.rules_id()), "{main}");
    }
    assert_verdict(&out, &report, "verified");
    assert_eq!(states(&report), [2, 0, 0, 0, 0, 0, 0]);
    assert_eq!(out.value("agreement"), Some("rules 1 stale 0 unused 0"));
    assert!(!out.stdout.contains("not checked"), "{out:?}");
    let agreement = get(&report, "agreement").as_object().unwrap();
    let members: Vec<&str> = agreement.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(members, ["rules", "stale", "unused"]);

    // Without it: the same post-image shows the relocated entities changed.
    let (out, report) = f.verify_report(&plan, &img);
    assert_verdict(&out, &report, "not_verified");
    assert_eq!(states(&report), [0, 0, 2, 0, 0, 0, 0]);
    let g = groups(&report);
    assert!(g.contains(&("removed".to_owned(), "old::S".to_owned())), "{g:?}");
    assert!(g.contains(&("added".to_owned(), "fresh::S".to_owned())), "{g:?}");

    // A wrong rule (the namespace moved to another name): not verified at
    // the job.
    relocation(&f, &plan, "w.decl", "other");
    let mut wrong = img.clone();
    wrong.extend(["--declared", "w.decl"]);
    f.verify(&plan, &wrong);
    for (o, main) in &observers {
        let (shard, _) = f.shard(&plan, "verify", 0, o);
        let v = shard.verdict.unwrap();
        assert_eq!(v.kind, VerdictKind::Mismatch, "{main}");
        assert!(v.findings.iter().any(|x| real(x.kind)), "{main}: {:?}", v.findings);
    }

    // The relocation and one callee changed: a finding that is not
    // indeterminate, in that observer only.
    f.write("post/u0.cpp", &U0.replace("old::", "fresh::").replace("h(1)", "h('1')"));
    f.verify(&plan, &with);
    let (shard, _) = f.shard(&plan, "verify", 0, &observers[0].0);
    let v = shard.verdict.unwrap();
    assert_eq!(v.kind, VerdictKind::Mismatch);
    assert!(v.findings.iter().any(|x| x.kind == FindingKind::Changed), "{:?}", v.findings);
    let (shard, _) = f.shard(&plan, "verify", 0, &observers[1].0);
    assert_eq!(shard.verdict.unwrap().kind, VerdictKind::Verified);
}

// spec/verify.md §7.1, spec/exec.md §8.7 (WP-112 R3): `--strict-names`
// on the command line. A relocation of a function that returns its
// `__PRETTY_FUNCTION__`: without the option the run is `verified` and the
// report notes the name-derived string; with it the note is a finding,
// the observer `mismatch`, the run not verified (status 1).
#[test]
fn s7_1_strict_names_on_the_command_line() {
    let f = Fixture::new("strict-names");
    let header = "#pragma once\nnamespace old {\n\
                  inline const char *who() { return __PRETTY_FUNCTION__; }\n}\n";
    f.write("s/a.h", header);
    f.write("post/a.h", &header.replace("namespace old", "namespace fresh"));
    f.write("s/u0.cpp", "#include \"a.h\"\nconst char *f0() { return old::who(); }\n");
    f.write("post/u0.cpp", "#include \"a.h\"\nconst char *f0() { return fresh::who(); }\n");
    f.db(&["u0.cpp"]);
    let plan = f.parse();
    let mut s = Vec::new();
    let img = image(&f, &mut s);
    relocation(&f, &plan, "d.decl", "fresh");
    let mut with = img.clone();
    with.extend(["--declared", "d.decl"]);

    let (out, report) = f.verify_report(&plan, &with);
    assert_verdict(&out, &report, "verified");
    assert_eq!(u64_at(&report, "names_changed.total"), 1, "{report:?}");
    assert!(out.stdout.contains("note: name-derived string changed: "), "{out:?}");
    assert_eq!(get(&report, "findings.groups").as_array().unwrap().len(), 0, "{report:?}");

    with.push("--strict-names");
    let (out, report) = f.verify_report(&plan, &with);
    assert_verdict(&out, &report, "not_verified");
    assert_eq!(states(&report), [0, 0, 1, 0, 0, 0, 0]);
    let g = get(&report, "findings.groups").as_array().unwrap();
    assert_eq!(g.len(), 1, "{report:?}");
    assert_eq!(g[0].get("kind").unwrap().as_str(), Some("names_changed"), "{report:?}");
}

// spec/verify.md §4.4, spec/digest.md §9 "The names digest" (WP-125 item
// 2): a function template passes its `__PRETTY_FUNCTION__` on, and the
// post-image respells its parameter type (`decltype(ns::S::value)` to
// `decltype(::ns::S::value)`, which the instantiation's string prints).
// Nothing is declared and every unit's digest is equal, so the run is
// `verified`; the report notes the one instantiation, by its key's name.
#[test]
fn s4_4_names_note_for_a_respelled_template_parameter() {
    let f = Fixture::new("respelled-template");
    let pre = "namespace ns { struct S { static int value; }; }\n\
               void take(const char *);\n\
               template <class T> void h(T, decltype(ns::S::value)) { take(__PRETTY_FUNCTION__); }\n\
               void u() { h(1, 1); }\n";
    f.write("s/u0.cpp", pre);
    f.write("post/u0.cpp", &pre.replace("decltype(ns::", "decltype(::ns::"));
    f.db(&["u0.cpp"]);
    let plan = f.parse();
    let mut s = Vec::new();
    let img = image(&f, &mut s);
    let (out, report) = f.verify_report(&plan, &img);
    assert_verdict(&out, &report, "verified");
    assert_eq!(u64_at(&report, "names_changed.total"), 1, "{report:?}");
    let notes: Vec<&str> = out
        .stdout
        .lines()
        .filter(|l| l.starts_with("note: name-derived string changed: "))
        .collect();
    assert_eq!(notes.len(), 1, "{out:?}");
    assert!(
        notes[0].starts_with("note: name-derived string changed: h<int>(int, int) @ "),
        "{out:?}"
    );
    assert_eq!(get(&report, "findings.groups").as_array().unwrap().len(), 0, "{report:?}");
}

// spec/verify.md §2.3 `m(P')`: a `relocate` whose parent is named as the
// post-image has it (`fresh`, which only the post-images declare) is
// resolved through the post-images' records: the class moved into the
// relocated namespace under a new name verifies at every job with the
// file `slip declare` wrote, and does not without that line.
#[test]
fn s2_3_relocate_into_a_parent_of_the_post_image() {
    let f = two_units("parent");
    let plan = f.parse();
    let moved = |t: &str| {
        t.replace("namespace old", "namespace fresh")
            .replace("struct S", "struct T")
            .replace("old::S", "fresh::T")
            .replace("S *s", "T *s")
            .replace("old::", "fresh::")
    };
    f.write("post/a.h", &moved(HEADER));
    f.write("post/u0.cpp", &moved(U0));
    f.write("post/u1.cpp", &moved(U1));
    let mut s = Vec::new();
    let img = image(&f, &mut s);
    let observers = f.observers(&plan);
    for (text, verified) in [
        ("relocate old => :: fresh\nrelocate old::S => fresh :: T\n", true),
        ("relocate old => :: fresh\n", false),
    ] {
        f.write("t.txt", text);
        let mut extra = img.clone();
        extra.extend(["--owned", "s"]);
        let out = declare(&f, &plan, "t.txt", &extra);
        assert_eq!(out.code(), Some(0), "{out:?}");
        let mut args = img.clone();
        args.extend(["--declared", "d.decl"]);
        f.verify(&plan, &args);
        for (o, main) in &observers {
            let v = f.shard(&plan, "verify", 0, o).0.verdict.unwrap();
            let want = if verified { VerdictKind::Verified } else { VerdictKind::Mismatch };
            assert_eq!(v.kind, want, "{text}{main}: {:?}", v.findings);
        }
    }
}

// spec/verify.md §2.2, §7.1 `exemptions`: a change declared `changed`
// through `slip declare` (no rule, so the report is M1's and can say
// `verified`): verified with the file, status 0, the exemption used by the
// one observer that needed it; not verified without it. The job's
// `post.declared` is the published container: the verdict's `used` holds
// the key.
#[test]
fn s2_2_exemption_through_the_whole_path() {
    let f = two_units("exemption");
    let plan = f.parse();
    f.write("post/u0.cpp", &U0.replace("h(1)", "h(1) + 1"));
    f.write("changed.txt", "changed f0(old::S *)\n");
    let mut s = Vec::new();
    let img = image(&f, &mut s);
    let mut args = vec![
        "declare",
        "--plan",
        &plan,
        "--store",
        "store",
        "--from",
        "changed.txt",
        "--out",
        "c.decl",
    ];
    args.extend(&img);
    let out = f.slip(&args);
    assert_eq!(out.code(), Some(0), "{out:?}");
    assert_eq!(out.value("changed"), Some("1"));
    let declared = Declared::from_bytes(&std::fs::read(f.path("c.decl")).unwrap()).unwrap();
    let key = declared.changed()[0];

    let mut with = img.clone();
    with.extend(["--declared", "c.decl"]);
    let (out, report) = f.verify_report(&plan, &with);
    assert_verdict(&out, &report, "verified");
    assert_eq!(states(&report), [2, 0, 0, 0, 0, 0, 0]);
    let ex = get(&report, "exemptions").as_array().unwrap();
    assert_eq!(ex.len(), 1);
    assert_eq!(
        (str_at(&ex[0], "list"), str_at(&ex[0], "key")),
        ("changed", key.to_string().as_str())
    );
    assert_eq!(u64_at(&ex[0], "observers"), 1);
    assert_eq!(
        out.value("exemption"),
        Some(format!("changed {} observers 1", &key.to_string()[..16]).as_str())
    );
    let (o0, _) = f.observers(&plan)[0].clone();
    let (shard, _) = f.shard(&plan, "verify", 0, &o0);
    assert_eq!(shard.verdict.unwrap().used, vec![key]);
    // The post shard's inputs are the post tree's files (the job's roots).
    let post = Fixture::text(&f.path("post"));
    let inputs = f.shard(&plan, "verify", 0, &o0).0.inputs.unwrap();
    assert!(inputs.iter().any(|i| i.path == format!("{post}/u0.cpp").into_bytes()), "{inputs:?}");

    let (out, report) = f.verify_report(&plan, &img);
    assert_verdict(&out, &report, "not_verified");
    assert_eq!(get(&report, "exemptions").as_array().unwrap().len(), 0);
}

// ----- verify: input errors (spec/exec.md §8.7's table, §8.5) ------------------------

#[test]
fn s8_7_input_errors() {
    let f = two_units("input");
    let plan = f.parse();
    let s = Fixture::text(&f.path("s"));
    let store = Fixture::text(&f.path("store"));
    let expect = |out: Out, line: String| {
        assert_eq!(out.code(), Some(3), "{out:?}");
        assert_eq!(out.stderr, format!("{line}\n"), "{out:?}");
        assert_eq!(out.stdout, "", "{out:?}");
    };
    let runs = f.verify_runs(&plan);

    // The declared file.
    expect(
        f.verify(&plan, &["--declared", "missing.decl"]),
        "slip: missing.decl: No such file or directory (os error 2)".into(),
    );
    f.write("bad.decl", "not a container");
    let why = Declared::from_bytes(b"not a container").unwrap_err();
    expect(f.verify(&plan, &["--declared", "bad.decl"]), format!("slip: bad.decl: {why}"));
    // A container of another kind is no DECL.
    let plan_file = f.path("store/plans").join(format!("{plan}.plan"));
    let plan_text = Fixture::text(&plan_file);
    let why = Declared::from_bytes(&std::fs::read(&plan_file).unwrap()).unwrap_err();
    expect(f.verify(&plan, &["--declared", &plan_text]), format!("slip: {plan_text}: {why}"));

    // The image.
    expect(
        f.verify(&plan, &["--root", "nope", "--post", "post"]),
        "slip: verify: image: nope: No such file or directory (os error 2)".into(),
    );
    f.write("a-file", "");
    expect(
        f.verify(&plan, &["--root", "s", "--post", "a-file"]),
        "slip: verify: image: a-file: not a directory".into(),
    );
    // A subject root under another subject root (a post root under a
    // subject root is an image: WP-052's G5 decides what it may hold).
    std::fs::create_dir_all(f.path("s/inner")).unwrap();
    std::fs::create_dir_all(f.path("a-dir")).unwrap();
    expect(
        f.verify(&plan, &["--root", "s", "--post", "post", "--root", "s/inner", "--post", "a-dir"]),
        format!("slip: verify: image: {s}/inner lies under {s}"),
    );
    expect(
        f.verify(&plan, &["--root", "s", "--post", "s"]),
        format!("slip: verify: image: {s} is a root of the image twice"),
    );

    // The declared file is read before the image is made.
    expect(
        f.verify(&plan, &["--declared", "missing.decl", "--root", "nope", "--post", "post"]),
        "slip: missing.decl: No such file or directory (os error 2)".into(),
    );

    // The plan (§8.5's forms).
    expect(
        f.verify("ffff0000", &[]),
        format!("slip: plan ffff0000: no plan in the store {store} has an id starting with it"),
    );

    // No map run of the plan: a plan published by `slip plan` only.
    let g = two_units("input-no-map");
    let out = g.slip(&["plan", "--db", "cdb.json", "--store", "store"]);
    let other = out.value("plan").unwrap().to_owned();
    let gs = Fixture::text(&g.path("store"));
    expect(
        g.verify(&other, &[]),
        format!(
            "slip: verify: plan {other} has no map run in the store {gs}; run slip parse first"
        ),
    );
    // ... after the image is made.
    expect(
        g.verify(&other, &["--root", "nope", "--post", "post"]),
        "slip: verify: image: nope: No such file or directory (os error 2)".into(),
    );

    // The round of a transform's run is refused; another round is not.
    publish_transform_run(&f.path("store"), &plan, 3);
    expect(
        f.verify(&plan, &["--round", "3"]),
        format!(
            "slip: verify: round 3 of plan {plan} is the round of a transform's run; slip verify \
             does not replace its shards"
        ),
    );
    assert!(!f.path("store/shards").join(&plan).join("verify.3").exists());
    assert_eq!(f.verify_runs(&plan), runs, "no input error ran the phase");
    assert_eq!(f.verify(&plan, &["--round", "4"]).code(), Some(0));
}

// spec/verify.md §2.1 (WP-103): a declared file whose rule table holds a
// rule §2.1 refuses (here a rule on a specialisation) is an input error
// of `verify` before any job is planned and before the image is made:
// status 3, the rule's index and the reason, no run, nothing published.
#[test]
fn s2_1_refused_rule_is_an_input_error() {
    use slip_model::keys::{KeyKind, Name, Parent, RULE_ON_SPECIALISATION, Rule};
    let f = two_units("refused");
    let plan = f.parse();
    let id = |s: &str| Name::Identifier(s.as_bytes().to_vec());
    let rule = |kind, from: &str, to: &str| Rule {
        parent: Parent::Root,
        kind,
        name: id(from),
        disc: None,
        to_parent: Parent::Root,
        to_name: id(to),
    };
    let declared = Declared::new(
        vec![rule(KeyKind::Namespace, "old", "fresh"), rule(KeyKind::Specialisation, "W", "W2")],
        vec![],
        vec![],
        vec![],
    )
    .unwrap()
    .with_files(Some(Ownership {
        owned: vec![f.root.to_str().unwrap().as_bytes().to_vec()],
        readonly: vec![],
    }));
    assert_eq!(declared.rules().refused(), Some((1, RULE_ON_SPECIALISATION)));
    std::fs::write(f.path("r.decl"), declared.to_bytes()).unwrap();
    let published =
        |f: &Fixture| std::fs::read_dir(f.path("store/declared")).map_or(0, Iterator::count);
    let before = published(&f);
    for extra in [
        &["--declared", "r.decl"][..],
        &["--declared", "r.decl", "--root", "nope", "--post", "post"],
    ] {
        let out = f.verify(&plan, extra);
        assert_eq!(out.code(), Some(3), "{out:?}");
        assert_eq!(out.stderr, format!("slip: r.decl: rule 1: {RULE_ON_SPECIALISATION}\n"));
        assert_eq!(out.stdout, "");
    }
    assert_eq!(f.verify_runs(&plan), 0, "no job was planned");
    assert_eq!(published(&f), before, "the refused file is not published");
}

/// A transform run record of `plan` and `round`, made with `slip_model`
/// and published with `Store::publish_transform_run`.
fn publish_transform_run(store: &Path, plan: &str, round: u64) {
    use slip_model::transform_run::TransformRun;
    let plan = Digest::from_hex(plan).unwrap();
    let record = TransformRun {
        tool_slip: "s".into(),
        tool_kiln: "k".into(),
        plan,
        round,
        transform: Digest([0x77; 32]),
        edits: None,
        declared: None,
        overlay: None,
        result: None,
        started_unix_ns: 7,
        wall_ns: 1,
        steps: vec![],
    };
    let store = slip_exec::store::Store::open(store, slip_exec::store::Access::Shared).unwrap();
    store.publish_transform_run(&plan, round, 7, &record.to_bytes()).unwrap();
}

// ----- declare (spec/exec.md §8.8, spec/verify.md §2.3) -------------------------------

/// `slip declare --plan <plan> --store store --from <from> --out d.decl`
/// over the identity image and `extra`.
fn declare(f: &Fixture, plan: &str, from: &str, extra: &[&str]) -> Out {
    let mut args =
        vec!["declare", "--plan", plan, "--store", "store", "--from", from, "--out", "d.decl"];
    args.extend(extra);
    f.slip(&args)
}

// §8.8: the output, and the container written.
#[test]
fn s8_8_declare_output() {
    let f = two_units("declare-out");
    let plan = f.parse();
    relocate_in_post(&f);
    f.write(
        "t.txt",
        "relocate old => :: fresh\nchanged h(int)\nremoved [function] h(char)\ncreated fresh::g(fresh::S *)\nkept h(int)\n",
    );
    let mut s = Vec::new();
    let mut extra = image(&f, &mut s);
    extra.extend(["--owned", "s"]);
    let out = declare(&f, &plan, "t.txt", &extra);
    assert_eq!(out.code(), Some(0), "{out:?}");
    assert_eq!(out.stderr, "");
    let bytes = std::fs::read(f.path("d.decl")).unwrap();
    let d = Declared::from_bytes(&bytes).unwrap();
    assert_eq!(
        out.stdout,
        format!(
            "declared {}\nrules {} 1\nchanged 1\ncreated 1\nremoved 1\nkept 1\n",
            content_id(&bytes).unwrap(),
            d.rules_id()
        )
    );
    assert_eq!(d.files(), Some(&Ownership { owned: vec![real_path(&f, "s")], readonly: vec![] }));
}

/// The one line of an error of §2.3: `slip: <text file>:<line>: <error>:
/// <detail>`, status 1, nothing written.
fn line_error(f: &Fixture, plan: &str, text: &str, extra: &[&str]) -> String {
    f.write("t.txt", text);
    let _ = std::fs::remove_file(f.path("d.decl"));
    let out = declare(f, plan, "t.txt", extra);
    assert_eq!(out.code(), Some(1), "{out:?}");
    assert_eq!(out.stdout, "", "{out:?}");
    assert!(!f.path("d.decl").exists(), "nothing is written on an error");
    let lines = out.stderr_lines();
    assert_eq!(lines.len(), 1, "{lines:?}");
    lines[0].to_owned()
}

// §2.3 "Errors": each error, with its line number and its exact line; the
// first syntax error wins over an earlier resolution error.
#[test]
fn s2_3_declare_errors() {
    let f = Fixture::new("declare-errors");
    f.both(
        "a.h",
        "#pragma once\nstruct S { int v; };\nint S_;\nstruct T {};\nint T;\n\
         struct C { C operator+(C); };\nvoid k(int);\ntemplate <class X> int tt(X) { return 0; }\n\
         extern \"C\" int cfn(int);\n",
    );
    f.both("u0.cpp", "#include \"a.h\"\nint f0() { return tt(1); }\n");
    f.db(&["u0.cpp"]);
    let plan = f.parse();
    assert_eq!(
        line_error(&f, &plan, "\n# a comment\nfrobnicate S\n", &[]),
        "slip: t.txt:3: syntax: unknown keyword `frobnicate`"
    );
    assert_eq!(
        line_error(&f, &plan, "changed nothing_here\nrelocate S => :: \n", &[]),
        "slip: t.txt:2: syntax: `` is neither an identifier nor `=`"
    );
    assert_eq!(
        line_error(&f, &plan, "changed k(int)\nchanged nothing_here\n", &[]),
        "slip: t.txt:2: no unit: no unit of the pre-images is named `nothing_here`, and no \
         unit's name contains `nothing_here`"
    );
    assert_eq!(
        line_error(&f, &plan, "changed T\n", &[]),
        "slip: t.txt:1: ambiguous: units named `T` in the pre-images have 2 keys, of the kinds \
         class, variable"
    );
    assert_eq!(
        line_error(&f, &plan, "changed [class] T\nchanged [variable] T\nchanged [class] T\n", &[]),
        "slip: t.txt:3: duplicate: a second `changed` line for `T`"
    );
    assert_eq!(
        line_error(&f, &plan, "relocate C::operator+(C) => C :: plus\n", &["--owned", "s"]),
        "slip: t.txt:1: not renamable: `C::operator+(C)` has a name of the form `operator`, not \
         an identifier: it can be relocated with `=` only"
    );
    assert_eq!(
        line_error(
            &f,
            &plan,
            "relocate k(int) => :: k2\nrelocate k(int) => :: k2\n",
            &["--owned", "s"]
        ),
        "slip: t.txt:2: duplicate: the rule of `k(int)` matches what the rule of an earlier \
         `relocate` line matches"
    );
    // §2.1 (WP-103): a rule on a specialisation.
    assert_eq!(
        line_error(&f, &plan, "relocate tt<int>(int) => :: uu\n", &["--owned", "s"]),
        "slip: t.txt:1: refused: the rule of `tt<int>(int)` is refused: a rule may not match a \
         specialisation: its key follows its template's"
    );
    // WP-111: a dependency entity (the header read-only), and a line that
    // relocates the new parent of a line above it.
    assert_eq!(
        line_error(&f, &plan, "relocate S => :: S2\n", &["--owned", "s", "--readonly", "s/a.h"]),
        "slip: t.txt:1: dependency entity: `S` is a dependency entity (a declaration of it is in \
         a read-only file, or it is a specialisation or an implicit member of one): no rule \
         renames it"
    );
    // §2.3: an entity with C language linkage is renamed with the parent
    // `c_linkage`; a parent that resolves to nothing is still an error.
    assert_eq!(
        line_error(
            &f,
            &plan,
            "changed k(int)\nrelocate cfn(int) => nowhere :: cfn2\n",
            &["--owned", "s"]
        ),
        "slip: t.txt:2: no unit: no unit of the post-images is named `nowhere`, and no unit's \
         name contains `nowhere`"
    );
    assert_eq!(
        line_error(
            &f,
            &plan,
            "relocate k(int) => C :: k2\nrelocate S => :: C\n",
            &["--owned", "s"]
        ),
        "slip: t.txt:2: order: the rule of `S` relocates `C`, or a namespace or class around it, \
         which line 1 resolved as a new parent without it: this line must come before line 1"
    );
    // `Fallback` has no source that makes it (`slip_digest` records:
    // no walk gives a fallback key today); it is tested over records in
    // `tests/declare.rs`.
}

// §2.3 "Errors", WP-111 review F1: the errors and the note of "One way to
// each key" (`spec/verify.md` §2.1), over `s2_3_declare_errors`'s files.
#[test]
fn s2_3_declare_one_way_to_each_key() {
    let f = Fixture::new("declare-one-way");
    f.both(
        "a.h",
        "#pragma once\nstruct S { int v; };\nint S_;\nstruct T {};\nint T;\n\
         struct C { C operator+(C); };\nvoid k(int);\ntemplate <class X> int tt(X) { return 0; }\n\
         extern \"C\" int cfn(int);\n",
    );
    f.both("u0.cpp", "#include \"a.h\"\nint f0() { return tt(1); }\n");
    f.db(&["u0.cpp"]);
    let plan = f.parse();
    // WP-111 review F1: two lines whose rules produce one key, the
    // message naming the earlier line; an identity rule, refused.
    assert_eq!(
        line_error(
            &f,
            &plan,
            "relocate k(int) => :: k2\nchanged S\nrelocate k(int) => :: k3\n",
            &["--owned", "s"]
        ),
        "slip: t.txt:3: duplicate: the rule of `k(int)` produces the key the rule of line 1 \
         produces: two entities of the post-image cannot both be this one of the pre-image (a \
         namespace split in two is declared per member or per nested namespace)"
    );
    assert_eq!(
        line_error(&f, &plan, "relocate S => :: =\n", &["--owned", "s"]),
        "slip: t.txt:1: refused: the rule of `S` is refused: a rule may not produce the key it \
         matches: it changes nothing"
    );
    // §2.3, §8.8: a C function moved without a new name writes no rule,
    // and a note says so; the status is 0.
    f.write("t.txt", "changed k(int)\nrelocate cfn(int) => :: =\n");
    let _ = std::fs::remove_file(f.path("d.decl"));
    let out = declare(&f, &plan, "t.txt", &["--owned", "s"]);
    assert_eq!(out.code(), Some(0), "{out:?}");
    assert_eq!(
        out.stderr,
        "slip: note: t.txt:2: the entity has C language linkage and keeps its name: the move \
         changes no key, and the line writes no rule\n"
    );
    assert!(out.stdout.contains("\nchanged 1\n"), "{out:?}");
    assert!(out.stdout.lines().nth(1).is_some_and(|l| l.ends_with(" 0")), "{out:?}");
}

// §8.8: input errors (status 3, nothing written): an unreadable text, a
// text that is not UTF-8, an unwritable `--out`, a selection of none or
// more than 64 observers, a refused image.
#[test]
fn s8_8_declare_input_errors() {
    let f = two_units("declare-input");
    let plan = f.parse();
    let expect = |out: Out, line: &str| {
        assert_eq!(out.code(), Some(3), "{out:?}");
        assert_eq!(out.stderr, format!("{line}\n"));
        assert_eq!(out.stdout, "");
        assert!(!f.path("d.decl").exists());
    };
    expect(
        declare(&f, &plan, "missing.txt", &[]),
        "slip: missing.txt: No such file or directory (os error 2)",
    );
    f.write("latin1.txt", "");
    std::fs::write(f.path("latin1.txt"), b"changed caf\xe9\n").unwrap();
    let out = declare(&f, &plan, "latin1.txt", &[]);
    assert_eq!(out.code(), Some(3), "{out:?}");
    assert!(out.stderr.starts_with("slip: latin1.txt: the text is not UTF-8 ("), "{out:?}");
    f.write("t.txt", "changed h(int)\n");
    let out = f.slip(&[
        "declare",
        "--plan",
        &plan,
        "--store",
        "store",
        "--from",
        "t.txt",
        "--out",
        "no/such/dir/d.decl",
    ]);
    assert_eq!(out.code(), Some(3), "{out:?}");
    assert_eq!(out.stderr, "slip: no/such/dir/d.decl: No such file or directory (os error 2)\n");
    expect(
        declare(&f, &plan, "t.txt", &["--only", "nothing-matches"]),
        "slip: declare: the selection keeps 0 of the plan's 2 observer(s); declare parses every \
         one in this process and needs from 1 to 64",
    );
    expect(
        declare(&f, &plan, "t.txt", &["--root", "s", "--post", "s"]),
        &format!(
            "slip: declare: image: {} is a root of the image twice",
            Fixture::text(&f.path("s"))
        ),
    );

    // 65 observers: refused before any parse.
    let g = Fixture::new("declare-65");
    g.both("a.h", "int h(int);\n");
    let files: Vec<String> = (0..65).map(|i| format!("u{i}.cpp")).collect();
    for name in &files {
        g.both(name, "#include \"a.h\"\n");
    }
    g.db(&files.iter().map(String::as_str).collect::<Vec<_>>());
    let out = g.slip(&["plan", "--db", "cdb.json", "--store", "store"]);
    let big = out.value("plan").unwrap().to_owned();
    g.write("t.txt", "changed h(int)\n");
    let out = declare(&g, &big, "t.txt", &[]);
    assert_eq!(out.code(), Some(3), "{out:?}");
    assert_eq!(
        out.stderr,
        "slip: declare: the selection keeps 65 of the plan's 65 observer(s); declare parses \
         every one in this process and needs from 1 to 64\n"
    );
    let out = declare(&g, &big, "t.txt", &["--limit", "64"]);
    assert_eq!(out.code(), Some(0), "64 is within the bound: {out:?}");
}

// §8.8: an image that does not parse contributes no record and is a
// warning, which does not change the status.
#[test]
fn s8_8_declare_warning_for_an_image_that_does_not_parse() {
    let f = two_units("declare-warn");
    f.both("u1.cpp", "#include \"a.h\"\nint f1() { return h(2) +; }\n");
    let plan = f.parse();
    let out = declare(&f, &plan, "t.txt", &[]);
    assert_eq!(out.code(), Some(3), "no text yet");
    f.write("t.txt", "changed f0(old::S *)\n");
    let out = declare(&f, &plan, "t.txt", &[]);
    assert_eq!(out.code(), Some(0), "{out:?}");
    let u1 = Fixture::text(&f.path("s/u1.cpp"));
    assert_eq!(out.stderr, format!("slip: warning: declare: {u1}: the pre-image is diagnosed\n"));
    assert_eq!(out.value("changed"), Some("1"));
    // A name only the unit that does not parse has is no unit.
    f.write("t.txt", "changed f1()\n");
    let out = declare(&f, &plan, "t.txt", &[]);
    assert_eq!(out.code(), Some(1), "{out:?}");
    assert!(out.stderr.contains("slip: t.txt:1: no unit: "), "{out:?}");

    // Post-images, which a `relocate` with a parent and a `created` line
    // both need: one that does not parse is warned about once, and the
    // second pass does not parse it again; the others give the records.
    f.write("post/u0.cpp", "#include \"a.h\"\nint f0(old::S *s) { return +; }\n");
    f.write("post/u1.cpp", U1);
    f.write("t.txt", "relocate old::S => old :: S2\ncreated h(int)\n");
    let mut s = Vec::new();
    let mut extra = image(&f, &mut s);
    extra.extend(["--owned", "s"]);
    let out = declare(&f, &plan, "t.txt", &extra);
    let u0 = Fixture::text(&f.path("s/u0.cpp"));
    assert_eq!(
        out.stderr_lines().iter().filter(|l| l.starts_with("slip: warning: ")).collect::<Vec<_>>(),
        [
            &format!("slip: warning: declare: {u0}: the post-image is diagnosed").as_str(),
            &format!("slip: warning: declare: {u1}: the pre-image is diagnosed").as_str(),
        ],
        "{out:?}"
    );
    assert_eq!(out.code(), Some(0), "{out:?}");
    assert_eq!(
        (out.value("created"), out.value("rules").map(|r| &r[65..])),
        (Some("1"), Some("1"))
    );
}

/// The real path of `rel` in the fixture, as `Ownership` holds it.
fn real_path(f: &Fixture, rel: &str) -> Vec<u8> {
    std::fs::canonicalize(f.path(rel)).unwrap().to_str().unwrap().as_bytes().to_vec()
}

// §8.8, spec/verify.md §2.1, §2.3: the run's files. A `relocate` line
// without `--owned` is a usage error before anything is read but the text
// (a plan that does not exist is not looked at); without a `relocate`
// line the options are optional and, when given, carried; entries are
// made real paths (a symbolic link resolved), and one that does not
// exist, or that is both owned and read-only, is an input error.
#[test]
fn s8_8_declare_files() {
    let f = two_units("declare-files");
    let plan = f.parse();
    f.write("t.txt", "changed h(int)\nrelocate old => :: fresh\n");
    for p in [plan.as_str(), "no-such-plan"] {
        let out = declare(&f, p, "t.txt", &[]);
        assert_eq!(out.code(), Some(2), "{out:?}");
        assert_eq!(out.stderr_lines()[0], "slip: a relocate line needs the run's files: --owned");
        assert!(!f.path("d.decl").exists());
    }
    // A syntax error in the text is reported first: it is the text's.
    f.write("t.txt", "relocate old => fresh\n");
    assert_eq!(declare(&f, &plan, "t.txt", &[]).code(), Some(1));

    // No rule: the options are checked, and not written (`spec/verify.md`
    // §2 rule 6: `files` is present exactly when there is a rule).
    f.write("t.txt", "changed h(int)\n");
    let read =
        |f: &Fixture| Declared::from_bytes(&std::fs::read(f.path("d.decl")).unwrap()).unwrap();
    assert_eq!(declare(&f, &plan, "t.txt", &[]).code(), Some(0));
    assert_eq!(read(&f).files(), None);
    std::os::unix::fs::symlink("s", f.path("link")).unwrap();
    let out = declare(&f, &plan, "t.txt", &["--owned", "link"]);
    assert_eq!(out.code(), Some(0), "{out:?}");
    assert_eq!(read(&f).files(), None);
    // With a rule, as real paths: `link` is `s`, given twice.
    f.write("t.txt", "changed h(int)\nrelocate old => :: fresh\n");
    // Given out of order: sorted.
    let out = declare(
        &f,
        &plan,
        "t.txt",
        &["--owned", "link", "--readonly", "link/u1.cpp", "--readonly", "s/a.h", "--owned", "s"],
    );
    assert_eq!(out.code(), Some(0), "{out:?}");
    let files = Ownership {
        owned: vec![real_path(&f, "s")],
        readonly: vec![real_path(&f, "s/a.h"), real_path(&f, "s/u1.cpp")],
    };
    assert_eq!(read(&f).files(), Some(&files));

    let _ = std::fs::remove_file(f.path("d.decl"));
    let out = declare(&f, &plan, "t.txt", &["--owned", "nope"]);
    assert_eq!(out.code(), Some(3), "{out:?}");
    assert_eq!(out.stderr, "slip: nope: No such file or directory (os error 2)\n");
    let out = declare(&f, &plan, "t.txt", &["--owned", "s", "--readonly", "link"]);
    assert_eq!(out.code(), Some(3), "{out:?}");
    assert_eq!(
        out.stderr,
        format!("slip: {}: given as --owned and as --readonly\n", Fixture::text(&f.path("s")))
    );
    assert!(!f.path("d.decl").exists());
}

// spec/verify.md §2.1, §2.3: `relocate` naming a namespace only a
// read-only file declares. `declare` accepts the line (`DependencyEntity`
// is for an entity that is not a namespace: a namespace's rule applies in
// the owned view, WP-111); its rule matches nothing, since every member of
// the namespace is a dependency entity: every observer mismatches, the
// relocated entities shown as removed and added; the run is not verified
// and reports the rule unused. A member of it, relocated, is refused.
#[test]
fn s2_1_rule_naming_a_dependency_entity_matches_nothing() {
    let f = two_units("dependency-entity");
    let plan = f.parse();
    relocate_in_post(&f);
    f.write("t.txt", "relocate old => :: fresh\n");
    let mut s = Vec::new();
    let img = image(&f, &mut s);
    let mut extra = img.clone();
    extra.extend(["--owned", "s", "--readonly", "s/a.h"]);
    let out = declare(&f, &plan, "t.txt", &extra);
    assert_eq!(out.code(), Some(0), "{out:?}");
    assert_eq!(out.value("rules").map(|r| &r[65..]), Some("1"));
    let mut with = img.clone();
    with.extend(["--declared", "d.decl"]);
    let (out, report) = f.verify_report(&plan, &with);
    for (o, main) in &f.observers(&plan) {
        let v = f.shard(&plan, "verify", 0, o).0.verdict.unwrap();
        assert_eq!(v.kind, VerdictKind::Mismatch, "{main}");
    }
    // The run (WP-121): both observers `mismatch`, status 1, and the rule
    // unused, since no observer used it (spec/verify.md §9.2).
    assert_verdict(&out, &report, "not_verified");
    assert_eq!(states(&report), [0, 0, 2, 0, 0, 0, 0]);
    assert_eq!(out.value("agreement"), Some("rules 1 stale 0 unused 1"));
    assert_eq!(out.value("unused rule"), Some("0"));
    f.write("t.txt", "relocate old::S => old :: S2\n");
    let out = declare(&f, &plan, "t.txt", &extra);
    assert_eq!(out.code(), Some(1), "{out:?}");
    assert!(
        out.stderr.starts_with("slip: t.txt:1: dependency entity: `old::S` is a dependency entity"),
        "{out:?}"
    );
}

// WP-111 (WP-053 review F2): an `--owned` entry that covers no file a
// selected pre-image read (here the post tree) is a warning, the status
// unchanged; one that covers a file is not.
#[test]
fn s8_8_declare_owned_entry_that_covers_nothing() {
    let f = two_units("owned-nothing");
    let plan = f.parse();
    relocate_in_post(&f);
    f.write("t.txt", "relocate old => :: fresh\n");
    let mut s = Vec::new();
    let img = image(&f, &mut s);
    let mut extra = img.clone();
    extra.extend(["--owned", "post"]);
    let out = declare(&f, &plan, "t.txt", &extra);
    assert_eq!(out.code(), Some(0), "{out:?}");
    assert_eq!(
        out.stderr,
        format!(
            "slip: warning: declare: --owned {} covers no file the selected observers' pre-images read\n",
            Fixture::text(&f.path("post"))
        )
    );
    let mut extra = img.clone();
    extra.extend(["--owned", "s", "--owned", "post"]);
    let out = declare(&f, &plan, "t.txt", &extra);
    assert_eq!(out.stderr_lines().len(), 1, "{out:?}");
    let mut extra = img.clone();
    extra.extend(["--owned", "s"]);
    let out = declare(&f, &plan, "t.txt", &extra);
    assert_eq!((out.code(), out.stderr.as_str()), (Some(0), ""), "{out:?}");
}

// WP-111, "Added after the reviews of WP-069 and WP-110": `declare` decides
// a dependency entity by the real path of its file. The header is in
// `dep`, reached through the link `s/inc -> dep`: read-only given by its
// real path (`--readonly s/dep`) and given through the link (`--readonly
// s/inc`, made the real path), `relocate` of a struct in it is
// `DependencyEntity`; with no `--readonly`, accepted.
#[test]
fn s2_3_dependency_entity_through_a_link_each_way() {
    let f = Fixture::new("dep-link");
    f.both("dep/a.h", HEADER);
    std::os::unix::fs::symlink("dep", f.path("s/inc")).unwrap();
    let unit = |t: &str| t.replace("\"a.h\"", "\"inc/a.h\"");
    f.both("u0.cpp", &unit(U0));
    f.both("u1.cpp", &unit(U1));
    f.db(&["u0.cpp", "u1.cpp"]);
    let plan = f.parse();
    f.write("t.txt", "relocate old::S => old :: S2\n");
    for readonly in ["s/dep", "s/inc"] {
        let line = line_error(
            &f,
            &plan,
            "relocate old::S => old :: S2\n",
            &["--owned", "s", "--readonly", readonly],
        );
        assert!(
            line.starts_with("slip: t.txt:1: dependency entity: `old::S`"),
            "{readonly}: {line}"
        );
    }
    let out = declare(&f, &plan, "t.txt", &["--owned", "s"]);
    assert_eq!(out.code(), Some(0), "{out:?}");
}

// spec/verify.md §2.3 "created": resolved in the post-images walked under
// the file's rules WITH the run's files. A function added in the
// relocated namespace has the mapped key (`old::k()`) only when its file
// is owned; the exemption then holds at the job and both observers
// verify. Walked with every file read-only, the key would be
// `fresh::k()`'s and `u0` would mismatch (an added unit).
#[test]
fn s2_2_created_resolved_under_the_rules_and_files() {
    let f = two_units("created-rules");
    let plan = f.parse();
    relocate_in_post(&f);
    let u0 = U0.replace("old::", "fresh::");
    f.write("post/u0.cpp", &format!("{u0}namespace fresh {{ int k() {{ return 0; }} }}\n"));
    f.write("t.txt", "relocate old => :: fresh\ncreated fresh::k()\n");
    let mut s = Vec::new();
    let img = image(&f, &mut s);
    let mut extra = img.clone();
    extra.extend(["--owned", "s"]);
    let out = declare(&f, &plan, "t.txt", &extra);
    assert_eq!(out.code(), Some(0), "{out:?}");
    assert_eq!(out.value("created"), Some("1"));
    let mut with = img.clone();
    with.extend(["--declared", "d.decl"]);
    f.verify(&plan, &with);
    for (o, main) in &f.observers(&plan) {
        let v = f.shard(&plan, "verify", 0, o).0.verdict.unwrap();
        assert_eq!(v.kind, VerdictKind::Verified, "{main}: {:?}", v.findings);
    }
}

// spec/verify.md §2.1: ownership is of real paths. The header is reached
// through `inc`, a symbolic link under the owned root to `dep`, which is
// read-only: the header is read-only (decided by its real path, not by the
// path the front end spells), the rule matches nothing there, and the
// observers mismatch; with `dep` owned, verified.
#[test]
fn s2_1_read_only_file_through_a_link_under_the_owned_root() {
    let f = Fixture::new("link-readonly");
    f.both("dep/a.h", HEADER);
    for tree in ["s", "post"] {
        std::os::unix::fs::symlink("dep", f.path(&format!("{tree}/inc"))).unwrap();
    }
    let unit = |t: &str| t.replace("\"a.h\"", "\"inc/a.h\"");
    f.both("u0.cpp", &unit(U0));
    f.both("u1.cpp", &unit(U1));
    f.db(&["u0.cpp", "u1.cpp"]);
    let plan = f.parse();
    f.write("post/dep/a.h", &HEADER.replace("namespace old", "namespace fresh"));
    f.write("post/u0.cpp", &unit(U0).replace("old::", "fresh::"));
    f.write("post/u1.cpp", &unit(U1).replace("old::", "fresh::"));
    f.write("t.txt", "relocate old => :: fresh\n");
    let mut s = Vec::new();
    let img = image(&f, &mut s);
    let observers = f.observers(&plan);
    for (readonly, want) in [(true, VerdictKind::Mismatch), (false, VerdictKind::Verified)] {
        let mut extra = img.clone();
        extra.extend(["--owned", "s"]);
        if readonly {
            extra.extend(["--readonly", "s/dep"]);
        }
        let out = declare(&f, &plan, "t.txt", &extra);
        assert_eq!(out.code(), Some(0), "{out:?}");
        let mut with = img.clone();
        with.extend(["--declared", "d.decl"]);
        f.verify(&plan, &with);
        for (o, main) in &observers {
            let v = f.shard(&plan, "verify", 0, o).0.verdict.unwrap();
            assert_eq!(v.kind, want, "readonly {readonly}, {main}: {:?}", v.findings);
        }
    }
}

// spec/verify.md §5.3 G6 (review F1), through `slip declare` and `slip
// verify`: the dependency reached through the link `s/inc -> dep`, which
// the post tree has as a real directory, its namespace relocated too.
// `declare --owned s --readonly s/inc` writes `dep` read-only; the verify
// job refuses the image (`unverifiable`, with the reason) instead of
// taking `post/inc/d.h` for an owned file. A `created` line makes
// `declare` parse the post-image under the rule: the same guard, an input
// error.
#[test]
fn g6_link_in_subject_directory_in_post_end_to_end() {
    const DEP: &str = "#pragma once\nnamespace old {\nstruct D { int a; };\n}\n";
    const MAIN: &str = "#include \"a.h\"\n#include \"inc/d.h\"\nint f0(old::S *s, old::D *d) { return old::g(s) + d->a; }\n";
    let f = Fixture::new("g6");
    f.write("s/a.h", HEADER);
    f.write("s/dep/d.h", DEP);
    std::os::unix::fs::symlink("dep", f.path("s/inc")).unwrap();
    f.write("s/u0.cpp", MAIN);
    f.db(&["u0.cpp"]);
    let plan = f.parse();
    f.write("post/a.h", &HEADER.replace("namespace old", "namespace fresh"));
    f.write("post/inc/d.h", &DEP.replace("namespace old", "namespace fresh"));
    f.write("post/u0.cpp", &MAIN.replace("old::", "fresh::"));
    f.write("t.txt", "relocate old => :: fresh\n");
    let mut s = Vec::new();
    let img = image(&f, &mut s);
    let mut extra = img.clone();
    extra.extend(["--owned", "s", "--readonly", "s/inc"]);
    let out = declare(&f, &plan, "t.txt", &extra);
    assert_eq!(out.code(), Some(0), "{out:?}");
    let d = Declared::from_bytes(&std::fs::read(f.path("d.decl")).unwrap()).unwrap();
    assert_eq!(d.files().unwrap().readonly, [real_path(&f, "s/dep")]);
    let reason = format!(
        "post-image: {} stands for {}, which the subject reaches through the link {}",
        Fixture::text(&f.path("post/inc/d.h")),
        Fixture::text(&f.path("s/inc/d.h")),
        Fixture::text(&f.path("s/inc"))
    );
    let mut with = img.clone();
    with.extend(["--declared", "d.decl"]);
    let (out, report) = f.verify_report(&plan, &with);
    for (o, main) in &f.observers(&plan) {
        let v = f.shard(&plan, "verify", 0, o).0.verdict.unwrap();
        assert_eq!(
            (v.kind, v.reason.as_str()),
            (VerdictKind::Unverifiable, reason.as_str()),
            "{main}"
        );
    }
    assert_eq!(states(&report), [0, 0, 0, 0, 1, 0, 0], "{out:?}");

    f.write("t.txt", "relocate old => :: fresh\ncreated fresh::D\n");
    let out = declare(&f, &plan, "t.txt", &extra);
    assert_eq!(out.code(), Some(3), "{out:?}");
    assert_eq!(
        out.stderr,
        format!("slip: declare: {}: {reason}\n", Fixture::text(&f.path("s/u0.cpp")))
    );
}

// G6 (iii) through the executable (the review's G6-F1 and G6-F2): the
// post tree at `b/post`, a level below the subject `s`; `../ext` from each
// is another directory, whose `k.h` defines another macro. Through a link
// `inc -> ../ext` in both trees, then by `#include "../ext/k.h"`: each
// shard `unverifiable`, the run `incomplete` (status 4), and `slip
// declare` refuses the post-image it parses for a `created` line (status
// 3).
#[test]
fn g6_route_leaving_the_root_end_to_end() {
    for (n, include) in [(0, "inc/k.h"), (1, "../ext/k.h")] {
        let f = Fixture::new(&format!("g6-leave-{n}"));
        let main = format!("#include \"{include}\"\nnamespace old {{ int f() {{ return K; }} }}\n");
        f.write("ext/k.h", "#pragma once\n#define K 1\n");
        f.write("b/ext/k.h", "#pragma once\n#define K 1\n#define K2 1\n");
        f.write("s/u0.cpp", &main);
        f.write("b/post/u0.cpp", &main.replace("return K;", "return K2;").replace("old", "fresh"));
        if n == 0 {
            std::os::unix::fs::symlink("../ext", f.path("s/inc")).unwrap();
            std::os::unix::fs::symlink("../ext", f.path("b/post/inc")).unwrap();
        }
        f.db(&["u0.cpp"]);
        let plan = f.parse();
        let img = ["--root", "s", "--post", "b/post"];
        let reason = format!(
            "post-image: looks for {include} in ., which leaves the post root: {} here, {} from \
             the subject",
            Fixture::text(&f.path("b/ext/k.h")),
            Fixture::text(&f.path("ext/k.h"))
        );
        let (out, report) = f.verify_report(&plan, &img);
        assert_eq!(out.code(), Some(4), "{out:?}");
        assert_eq!(out.last_line(), "verdict incomplete", "{out:?}");
        assert_eq!(states(&report), [0, 0, 0, 0, 1, 0, 0], "{out:?}");
        for (o, main) in &f.observers(&plan) {
            let v = f.shard(&plan, "verify", 0, o).0.verdict.unwrap();
            assert_eq!(
                (v.kind, v.reason.as_str()),
                (VerdictKind::Unverifiable, reason.as_str()),
                "{main}"
            );
        }
        // A `created` line: the post-image parsed under the rules.
        f.write("t.txt", "relocate old => :: fresh\ncreated fresh::f\n");
        let mut extra = img.to_vec();
        extra.extend(["--owned", "s"]);
        let out = declare(&f, &plan, "t.txt", &extra);
        assert_eq!(out.code(), Some(3), "{out:?}");
        assert_eq!(
            out.stderr,
            format!("slip: declare: {}: {reason}\n", Fixture::text(&f.path("s/u0.cpp")))
        );
    }
}

// WP-121 item 5 through the executable: `slip declare` + `slip verify`.

/// A fixture of `files` (the same in both trees), parsed, with `post`
/// written over the post tree, and the declared file of `text` (`--owned
/// s`) at `d.decl`: the plan and the image's words.
fn declared_run(
    name: &str,
    files: &[(&str, &str)],
    post: &[(&str, String)],
    text: &str,
) -> (Fixture, String) {
    let f = Fixture::new(name);
    for (rel, t) in files {
        f.both(rel, t);
    }
    let mains: Vec<&str> = files
        .iter()
        .map(|(r, _)| *r)
        .filter(|r| Path::new(r).extension() == Some("cpp".as_ref()))
        .collect();
    f.db(&mains);
    let plan = f.parse();
    for (rel, t) in post {
        f.write(&format!("post/{rel}"), t);
    }
    f.write("t.txt", text);
    let mut s = Vec::new();
    let mut extra = image(&f, &mut s);
    extra.extend(["--owned", "s"]);
    let out = declare(&f, &plan, "t.txt", &extra);
    assert_eq!(out.code(), Some(0), "{out:?}");
    (f, plan)
}

/// `slip verify --declared d.decl` over the fixture's image.
fn verify_declared(f: &Fixture, plan: &str) -> (Out, Value) {
    let mut s = Vec::new();
    let mut args = image(f, &mut s);
    args.extend(["--declared", "d.decl"]);
    f.verify_report(plan, &args)
}

// spec/verify.md §9.2, §9.3, spec/exec.md §8.7 (WP-121 item 5): a
// namespace relocation over two observers sharing a header: both
// `verified`, the run verified (status 0). WP-111: neither observer is
// stale for the rule (§9.2 "Stale for a namespace rule": every owned
// member of `old` moved, none is kept) and no line says it is not checked
// (until WP-111 the rule was listed `unchecked`, the verdict unchanged).
// The wrong edit beside it (one value changed in `u1`): that observer
// `mismatch`, the run not verified (status 1).
#[test]
fn s9_3_relocation_declared_and_verified() {
    const H: &str = "namespace old { struct S { int x; }; inline int f(S s) { return s.x; } }\n";
    const U0: &str = "#include \"a.h\"\nint u0() { old::S s{1}; return f(s); }\n";
    const U1: &str = "#include \"a.h\"\nint u1() { return old::f(old::S{2}); }\n";
    for wrong in [false, true] {
        let fresh = |s: &str| s.replace("old", "fresh");
        let u1 = if wrong { fresh(U1).replace("S{2}", "S{3}") } else { fresh(U1) };
        let (f, plan) = declared_run(
            &format!("reloc-{wrong}"),
            &[("a.h", H), ("u0.cpp", U0), ("u1.cpp", U1)],
            &[("a.h", fresh(H)), ("u0.cpp", fresh(U0)), ("u1.cpp", u1)],
            "relocate old => :: fresh\n",
        );
        let (out, report) = verify_declared(&f, &plan);
        if wrong {
            assert_verdict(&out, &report, "not_verified");
            assert_eq!(states(&report), [1, 0, 1, 0, 0, 0, 0], "{out:?}");
        } else {
            assert_verdict(&out, &report, "verified");
            assert_eq!(states(&report), [2, 0, 0, 0, 0, 0, 0], "{out:?}");
        }
        assert_eq!(out.value("agreement"), Some("rules 1 stale 0 unused 0"));
        assert!(!out.stdout.contains("not checked"), "{out:?}");
    }
}

// spec/verify.md §9.2 (WP-121 item 5): a rename (`relocate S => :: T`, a
// rule of kind `class`) over two observers, `u1` declaring its own `S`
// and not edited: each observer `verified`, `u1` stale for the rule
// ("A stale observer is one finding of the run"), the run not verified
// (status 1). The wrong edit beside it: `u0` `mismatch`.
#[test]
fn s9_2_rename_declared_with_a_stale_observer() {
    const U0: &str = "#include \"a.h\"\nint u0() { S s{1}; return s.x; }\n";
    const U1: &str = "struct S { int y; };\nint u1() { S s{2}; return s.y; }\n";
    for wrong in [false, true] {
        let u0 = U0.replace("S s", "T s");
        let u0 = if wrong { u0.replace("{1}", "{3}") } else { u0 };
        let (f, plan) = declared_run(
            &format!("rename-{wrong}"),
            &[("a.h", "struct S { int x; };\n"), ("u0.cpp", U0), ("u1.cpp", U1)],
            &[("a.h", "struct T { int x; };\n".to_owned()), ("u0.cpp", u0)],
            "relocate S => :: T\n",
        );
        let (out, report) = verify_declared(&f, &plan);
        assert_verdict(&out, &report, "not_verified");
        let want = if wrong { [1, 0, 1, 0, 0, 0, 0] } else { [2, 0, 0, 0, 0, 0, 0] };
        assert_eq!(states(&report), want, "{out:?}");
        assert_eq!(out.value("agreement"), Some("rules 1 stale 1 unused 0"));
        let u1 = Fixture::text(&f.path("s/u1.cpp"));
        assert!(
            out.stdout.contains(&format!("\nstale rule 0 direct 1 kept_via 0 {u1}\n")),
            "{out:?}"
        );
    }
}

// ----- units (spec/exec.md §8.9) ------------------------------------------------------

fn units(f: &Fixture, plan: &str, extra: &[&str]) -> Out {
    let mut args = vec!["units", "--plan", plan, "--store", "store"];
    args.extend(extra);
    let out = f.slip(&args);
    assert_eq!(out.code(), Some(0), "{out:?}");
    out
}

/// The `(kind, class, flags, name)` of each line, checking the key's form.
fn unit_lines(out: &Out) -> Vec<(String, String, String, String)> {
    out.stdout
        .lines()
        .map(|l| {
            let mut w = l.splitn(5, ' ');
            let key = w.next().unwrap();
            assert!(key.len() == 16 && key.bytes().all(|b| b.is_ascii_hexdigit()), "{l}");
            let mut next = || w.next().unwrap_or_else(|| panic!("{l}")).to_owned();
            (next(), next(), next(), next())
        })
        .collect()
}

// §8.9: the pre-image's records, sorted by name, once for the two
// observers that share a header; with `--post-image`, the post-image's;
// `--match` (any of several); the flags.
#[test]
fn s8_9_units() {
    let f = two_units("units");
    let plan = f.parse();
    relocate_in_post(&f);
    let pre = unit_lines(&units(&f, &plan, &[]));
    let names: Vec<&str> = pre.iter().map(|l| l.3.as_str()).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(names, sorted, "sorted by name");
    for n in ["old::S", "old::g(old::S *)", "h(int)", "h(char)", "f0(old::S *)", "f1()"] {
        assert_eq!(names.iter().filter(|x| **x == n).count(), 1, "{n} once: {names:?}");
    }
    assert!(
        pre.contains(&("class".into(), "declared".into(), "-".into(), "old::S".into())),
        "{pre:?}"
    );
    assert!(pre.contains(&("namespace".into(), "tu".into(), "-".into(), "old".into())), "{pre:?}");
    assert!(!names.iter().any(|n| n.starts_with("fresh")));

    let mut s = Vec::new();
    let mut args = image(&f, &mut s);
    args.push("--post-image");
    let post = unit_lines(&units(&f, &plan, &args));
    assert!(post.iter().any(|l| l.3 == "fresh::S") && !post.iter().any(|l| l.3.starts_with("old")));
    // Without the image, `--post-image` is the subject as it is.
    let same = unit_lines(&units(&f, &plan, &["--post-image"]));
    assert_eq!(same, pre);

    let m = unit_lines(&units(&f, &plan, &["--match", "h(", "--match", "::S"]));
    let names: Vec<&str> = m.iter().map(|l| l.3.as_str()).collect();
    assert_eq!(
        names,
        ["f0(old::S *)", "h(char)", "h(int)", "old::S", "old::S::v", "old::g(old::S *)"]
    );
    let only = unit_lines(&units(&f, &plan, &["--only", "u1.cpp", "--match", "f"]));
    assert_eq!(only.iter().map(|l| l.3.as_str()).collect::<Vec<_>>(), ["f1()"]);
}

// §8.9 `<flags>`: `i` for an indeterminate unit (a function whose body
// declares a variable-length array, an extension the walk does not digest:
// `f6(int)` below; any construct the walk leaves indeterminate will do, and
// this one changes when the walk digests it); `-` for one it determines;
// the warning of §8.8 for an observer that does not parse, status 0.
#[test]
fn s8_9_units_flags_and_warning() {
    let f = Fixture::new("units-flags");
    f.both("u0.cpp", "int f0() { return 1; }\nint f6(int n) { int a[n]; return sizeof(a); }\n");
    f.both("u1.cpp", "int f1() { return +; }\n");
    f.db(&["u0.cpp", "u1.cpp"]);
    let plan = f.parse();
    let out = units(&f, &plan, &[]);
    let u1 = Fixture::text(&f.path("s/u1.cpp"));
    assert_eq!(out.stderr, format!("slip: warning: units: {u1}: the pre-image is diagnosed\n"));
    let lines = unit_lines(&out);
    let flags = |name: &str| {
        let l = lines.iter().find(|l| l.3 == name).unwrap_or_else(|| panic!("{lines:?}"));
        (l.0.clone(), l.1.clone(), l.2.clone())
    };
    assert_eq!(flags("f6(int)"), ("function".into(), "declared".into(), "i".into()));
    assert_eq!(flags("f0()"), ("function".into(), "declared".into(), "-".into()));
}

// ----- goldens -------------------------------------------------------------------

/// `text` with the fixture's root as `<root>`, every run of 64 hexadecimal
/// digits as `<id>` and of 16 as `<key>`, runs of 15 or more decimal
/// digits (times) as `<ns>`, and the values of `wall` and `peak_rss`
/// masked.
fn mask(text: &str, root: &Path) -> String {
    let text = text.replace(root.to_str().unwrap(), "<root>");
    let mut out = String::new();
    let mut run = String::new();
    let flush = |run: &mut String, out: &mut String| {
        let hex = run.bytes().all(|b| b.is_ascii_hexdigit());
        let digits = run.bytes().all(|b| b.is_ascii_digit());
        match run.len() {
            64 if hex => out.push_str("<id>"),
            16 if hex => out.push_str("<key>"),
            n if n >= 15 && digits => out.push_str("<ns>"),
            _ => out.push_str(run),
        }
        run.clear();
    };
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            run.push(c);
        } else {
            flush(&mut run, &mut out);
            out.push(c);
        }
    }
    flush(&mut run, &mut out);
    out.lines()
        .map(|l| {
            if l.starts_with("wall ") {
                "wall <s>".to_owned()
            } else if l.starts_with("peak_rss ") {
                // Which observer peaked is the machine's.
                "peak_rss <n> <main>".to_owned()
            } else {
                l.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

fn golden(name: &str, actual: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/verify").join(name);
    if std::env::var_os("SLIP_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e} (SLIP_BLESS=1 writes it)", path.display()));
    assert_eq!(actual, expected, "{name} differs from its golden; SLIP_BLESS=1 rewrites it");
}
