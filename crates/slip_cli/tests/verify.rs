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
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use slip_model::declared::Declared;
use slip_model::formats::content_id;
use slip_model::json::{self, Value};
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
    use std::os::unix::fs::MetadataExt;
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

// spec/verify.md §2.1, §5.2 step 1: the post-image is digested WITH the
// declared rules. A relocation through the whole path (`slip parse`, an
// edited copy of the tree, `slip declare`, `slip verify --root --post
// --declared`): with the rule every observer's verdict is `verified`
// with no finding at all; without `--declared`, or with a wrong rule, the
// relocated entities show as removed and added. One callee changed on top
// of the relocation shows a finding that is not indeterminate.
//
// The run's report under rules: every observer is `failed` and the run
// incomplete (status 4) on this base, because the verify job writes no
// `binding` and `rule_use` for a tree image under rules, which the report
// requires (`spec/landed/verify/WP-070.md` §2.1, A3).
// `docs/wp/WP-074.md` item 3 writes both; then these observers are
// `verified` and the run's verdict `verified`, and the assertions marked
// WP-074 change.
#[test]
fn s2_relocation_through_the_whole_path() {
    let f = two_units("relocation");
    let plan = f.parse();
    relocate_in_post(&f);
    f.write("rules.txt", "# the root namespace moves\nrelocate old => :: fresh\n");
    let mut s = Vec::new();
    let img = image(&f, &mut s);
    let mut args = vec![
        "declare",
        "--plan",
        &plan,
        "--store",
        "store",
        "--from",
        "rules.txt",
        "--out",
        "d.decl",
    ];
    args.extend(&img);
    let out = f.slip(&args);
    assert_eq!(out.code(), Some(0), "{out:?}");
    let bytes = std::fs::read(f.path("d.decl")).unwrap();
    let declared = Declared::from_bytes(&bytes).unwrap();
    assert_eq!(declared.rules().rules().len(), 1);
    assert_eq!(out.value("declared"), Some(content_id(&bytes).unwrap().to_string().as_str()));
    let observers = f.observers(&plan);

    // With the rule.
    let mut with = img.clone();
    with.extend(["--declared", "d.decl"]);
    let (out, report) = f.verify_report(&plan, &with);
    assert_eq!(str_at(&report, "rules"), declared.rules().id().to_string());
    for (o, main) in &observers {
        let (shard, _) = f.shard(&plan, "verify", 0, o);
        let v = shard.verdict.as_ref().unwrap();
        assert_eq!(v.kind, VerdictKind::Verified, "{main}: {:?}", v.findings);
        assert_eq!((v.total, v.rules), (0, declared.rules().id()), "{main}");
    }
    // WP-074: `failed` until the verify job writes `binding` and `rule_use`.
    assert_verdict(&out, &report, "incomplete");
    assert_eq!(states(&report), [0, 0, 0, 0, 0, 0, 2]);

    // Without it: the same post-image shows the relocated entities changed.
    let (out, report) = f.verify_report(&plan, &img);
    assert_verdict(&out, &report, "not_verified");
    assert_eq!(states(&report), [0, 0, 2, 0, 0, 0, 0]);
    let g = groups(&report);
    assert!(g.contains(&("removed".to_owned(), "old::S".to_owned())), "{g:?}");
    assert!(g.contains(&("added".to_owned(), "fresh::S".to_owned())), "{g:?}");

    // A wrong rule (the namespace moved to another name): not verified at
    // the job.
    f.write("wrong.txt", "relocate old => :: other\n");
    let mut args = vec![
        "declare",
        "--plan",
        &plan,
        "--store",
        "store",
        "--from",
        "wrong.txt",
        "--out",
        "w.decl",
    ];
    args.extend(&img);
    assert_eq!(f.slip(&args).code(), Some(0));
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
        let out = declare(&f, &plan, "t.txt", &img);
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
    std::fs::create_dir_all(f.path("s/inner")).unwrap();
    expect(
        f.verify(&plan, &["--root", "s", "--post", "s/inner"]),
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
    .unwrap();
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
        "relocate old => :: fresh\nchanged h(int)\nremoved [function] h(char)\ncreated fresh::g(fresh::S *)\n",
    );
    let mut s = Vec::new();
    let out = declare(&f, &plan, "t.txt", &image(&f, &mut s));
    assert_eq!(out.code(), Some(0), "{out:?}");
    assert_eq!(out.stderr, "");
    let bytes = std::fs::read(f.path("d.decl")).unwrap();
    let d = Declared::from_bytes(&bytes).unwrap();
    assert_eq!(
        out.stdout,
        format!(
            "declared {}\nrules {} 1\nchanged 1\ncreated 1\nremoved 1\n",
            content_id(&bytes).unwrap(),
            d.rules().id()
        )
    );
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
         struct C { C operator+(C); };\nvoid k(int);\ntemplate <class X> int tt(X) { return 0; }\n",
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
        line_error(&f, &plan, "relocate C::operator+(C) => C :: plus\n", &[]),
        "slip: t.txt:1: not renamable: `C::operator+(C)` has a name of the form `operator`, not \
         an identifier: it can be relocated with `=` only"
    );
    assert_eq!(
        line_error(&f, &plan, "relocate k(int) => :: k2\nrelocate k(int) => :: k2\n", &[]),
        "slip: t.txt:2: duplicate: the rule of `k(int)` matches what the rule of an earlier \
         `relocate` line matches"
    );
    // §2.1 (WP-103): a rule on a specialisation.
    assert_eq!(
        line_error(&f, &plan, "relocate tt<int>(int) => :: uu\n", &[]),
        "slip: t.txt:1: refused: the rule of `tt<int>(int)` is refused: a rule may not match a \
         specialisation: its key follows its template's"
    );
    // `Fallback` has no source that makes it (`slip_digest` records:
    // no walk gives a fallback key today); it is tested over records in
    // `tests/declare.rs`.
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

// §8.9 `<flags>`: `i` for an indeterminate unit (the specialisation of a
// function template is a pending area of the walk until WP-045:
// `docs/wp/WP-052.md` Handback); the warning of §8.8
// for an observer that does not parse, status 0.
#[test]
fn s8_9_units_flags_and_warning() {
    let f = Fixture::new("units-flags");
    f.both("u0.cpp", "template <class T> int t(T) { return 0; }\nint f0() { return t(1); }\n");
    f.both("u1.cpp", "int f1() { return +; }\n");
    f.db(&["u0.cpp", "u1.cpp"]);
    let plan = f.parse();
    let out = units(&f, &plan, &[]);
    let u1 = Fixture::text(&f.path("s/u1.cpp"));
    assert_eq!(out.stderr, format!("slip: warning: units: {u1}: the pre-image is diagnosed\n"));
    let lines = unit_lines(&out);
    let t = lines.iter().find(|l| l.3 == "t<int>(int)").unwrap_or_else(|| panic!("{lines:?}"));
    assert_eq!((t.0.as_str(), t.1.as_str(), t.2.as_str()), ("specialisation", "derived", "i"));
    let f0 = lines.iter().find(|l| l.3 == "f0()").unwrap_or_else(|| panic!("{lines:?}"));
    assert_eq!((f0.0.as_str(), f0.1.as_str(), f0.2.as_str()), ("function", "declared", "-"));
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
