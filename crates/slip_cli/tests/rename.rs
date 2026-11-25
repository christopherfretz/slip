//! `slip map`, `check` and `rename` through the executable
//! (`spec/edits.md` §8, §10; `spec/exec.md` §8.11, §8.13), over small
//! trees written by the tests. Work package WP-076, "Tests".
//!
//! Every tree is under the work root (`common::work_root()`); paths are spelled
//! as the directory entries are.

#![cfg(target_os = "linux")]

mod common;

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use slip_exec::store::{Access, Store};
use slip_model::declared::Declared;
use slip_model::edits::Piece;
use slip_model::editset::{EditSet, SetState};
use slip_model::formats::content_id;
use slip_model::result::{State, TransformResult};
use slip_model::transform_run::StepName::{Apply, Conclude, Map, Reduce, Scan, Verify, Write};
use slip_model::transform_run::{StepName, TransformRun};
use slip_model::{Digest, Phase, Plan, Shard};

const SLIP: &str = env!("CARGO_BIN_EXE_slip");
const BOUND: Duration = Duration::from_mins(3);

struct Tree {
    root: PathBuf,
}

impl Tree {
    fn new(name: &str) -> Tree {
        Tree::new_in(&common::work_root(), name)
    }

    /// A tree under `base`: the work root, or the folding root for a test
    /// about spelling on a volume that folds (`common::folding_root`).
    fn new_in(base: &Path, name: &str) -> Tree {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let root = base.join(format!("wp076-{}-{n}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        Tree { root: std::fs::canonicalize(&root).unwrap() }
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    fn write(&self, rel: &str, text: &str) {
        let p = self.path(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.path(rel)).unwrap()
    }

    /// `cdb.json` compiling each of `units` (relative paths) as C++17.
    fn db(&self, units: &[&str]) {
        let dir = self.root.to_str().unwrap();
        let items: Vec<String> = units
            .iter()
            .map(|u| {
                format!(
                    "{{\"directory\": \"{dir}\", \"file\": \"{u}\", \"command\": \"clang++ \
                     -std=c++17 -c {u}\"}}"
                )
            })
            .collect();
        self.write("cdb.json", &format!("[\n{}\n]\n", items.join(",\n")));
    }

    /// The three-file example: `f` declared in a header, defined in one
    /// source file, called in another (the README's).
    fn three_files(name: &str) -> Tree {
        let t = Tree::new(name);
        t.write("src/h.h", "#pragma once\nint f(int);\n");
        t.write("src/a.cpp", "#include \"h.h\"\nint f(int x) { return x + 1; }\n");
        t.write("src/b.cpp", "#include \"h.h\"\nint g() { return f(2); }\n");
        t.db(&["src/a.cpp", "src/b.cpp"]);
        t
    }

    /// Every file under `src/`, by relative path.
    fn snapshot(&self) -> BTreeMap<String, String> {
        let mut m = BTreeMap::new();
        let mut dirs = vec![self.path("src")];
        while let Some(d) = dirs.pop() {
            for e in std::fs::read_dir(&d).unwrap().flatten() {
                let p = e.path();
                if e.file_type().unwrap().is_dir() {
                    dirs.push(p);
                } else if e.file_type().unwrap().is_file() {
                    let rel = p.strip_prefix(&self.root).unwrap().to_str().unwrap().to_owned();
                    m.insert(rel, std::fs::read_to_string(&p).unwrap_or_default());
                }
            }
        }
        m
    }

    /// The entries of the tree's root but the store.
    fn top(&self) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(&self.root)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| !n.starts_with("store"))
            .collect();
        v.sort();
        v
    }

    fn slip(&self, args: &[&str]) -> Out {
        let mut cmd = Command::new(SLIP);
        cmd.args(args).current_dir(&self.root);
        run_bounded(cmd)
    }

    /// A plan of `cdb.json` in `store`; its id.
    fn plan(&self, store: &str) -> String {
        let out = self.slip(&["plan", "--db", "cdb.json", "--store", store]);
        assert_eq!(out.code(), Some(0), "{out:?}");
        out.value("plan").unwrap().to_owned()
    }

    fn store(&self, name: &str) -> Store {
        Store::open(&self.path(name), Access::Shared).unwrap()
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

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

    fn value(&self, key: &str) -> Option<&str> {
        self.stdout.lines().find_map(|l| l.strip_prefix(key)?.strip_prefix(' '))
    }

    /// The id of the line `<key> <id>` or `<key> <id>: …`.
    fn id(&self, key: &str) -> String {
        let v = self.value(key).unwrap_or_else(|| panic!("no `{key}` line: {self:?}"));
        v.split(':').next().unwrap().to_owned()
    }

    fn has_line(&self, prefix: &str) -> bool {
        self.stdout.lines().any(|l| l.starts_with(prefix))
    }

    /// Standard output with every 64-digit id as `<id>`.
    fn forms(&self) -> Vec<String> {
        self.stdout.lines().map(ids_masked).collect()
    }
}

fn ids_masked(line: &str) -> String {
    let b = line.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < b.len() {
        let hex = b[i..].iter().take_while(|c| c.is_ascii_hexdigit()).count();
        if hex == 64 {
            out.push_str("<id>");
            i += 64;
        } else {
            out.push(b[i] as char);
            i += 1;
        }
    }
    out
}

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
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("slip did not finish within {bound:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn rename(t: &Tree, plan: &str, extra: &[&str]) -> Out {
    let mut args = vec![
        "rename", "--plan", plan, "--from", "f", "--to", "renamed", "--owned", "src", "--store",
        "store", "-j", "2",
    ];
    args.extend_from_slice(extra);
    t.slip(&args)
}

const POST_A: &str = "#include \"h.h\"\nint renamed(int x) { return x + 1; }\n";
const POST_B: &str = "#include \"h.h\"\nint g() { return renamed(2); }\n";
const POST_H: &str = "#pragma once\nint renamed(int);\n";

fn summary(written: &str) -> Vec<String> {
    let mut v: Vec<String> = [
        "transform <id>",
        "round 1",
        "map: selected 2, ok 2, not ok 0, failed runs 0",
        "edits <id>",
        "state accepted",
        "edit 3 in 3 file(s); shared 1; most observers 2; nested 0",
        "kept 0; unresolved 0",
        "declared <id>: rules 1, exempt 0",
        "overlay <id>: files 3, spellings 1",
        "verify: verified 2, unchanged 0, mismatch 0, not_parsed 0, unverifiable 0, \
         never_parsed 0, failed 0",
        "agreement: stale 0, unused rules 0",
        "spellings: passed 1, failed 0",
        "residue: dependent 0, not_compiled 0, not_planned 0; mentions 0",
        "result <id>",
        "state verified",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    if !written.is_empty() {
        v.push(written.to_owned());
    }
    v
}

// §10.1, §10.5: the three-file example. `--write` leaves the post-image,
// status 0; the output is §10.5's forms, line by line.
#[test]
fn s10_rename_write() {
    let t = Tree::three_files("write");
    let plan = t.plan("store");
    let out = rename(&t, &plan, &["--write"]);
    assert_eq!(out.code(), Some(0), "{out:?}");
    assert_eq!(out.forms(), summary("written 3 file(s)"), "{out:?}");
    assert_eq!(t.read("src/a.cpp"), POST_A);
    assert_eq!(t.read("src/b.cpp"), POST_B);
    assert_eq!(t.read("src/h.h"), POST_H);
}

// §10.5: with `--db`, the plan lines of `parse` come first.
#[test]
fn s10_5_rename_with_db_prints_the_plan_lines_first() {
    let t = Tree::three_files("db");
    let out = t.slip(&[
        "rename", "--db", "cdb.json", "--from", "::f", "--to", "renamed", "--owned", "src",
        "--store", "store",
    ]);
    assert_eq!(out.code(), Some(0), "{out:?}");
    let forms = out.forms();
    assert_eq!(forms[0], "plan <id>");
    assert_eq!(forms[1], "observers 2");
    assert_eq!(forms[8..], summary("")[..], "{out:?}");
}

// §7.2: `--patch` writes the patch and leaves the tree.
#[test]
fn s10_rename_patch() {
    let t = Tree::three_files("patch");
    let plan = t.plan("store");
    let before = t.snapshot();
    let out = rename(&t, &plan, &["--patch", "out.patch", "--relative-to", "."]);
    assert_eq!(out.code(), Some(0), "{out:?}");
    assert_eq!(out.forms(), summary("patch out.patch"), "{out:?}");
    assert_eq!(t.snapshot(), before);
    let patch = t.read("out.patch");
    assert!(patch.contains("+int renamed(int);"), "{patch}");
    assert!(patch.contains("--- a/src/h.h") || patch.contains("src/h.h"), "{patch}");
}

// §10.1: without `--write` or `--patch` nothing changes outside the store.
#[test]
fn s10_rename_changes_nothing_but_the_store() {
    let t = Tree::three_files("dry");
    let plan = t.plan("store");
    let (before, top) = (t.snapshot(), t.top());
    let out = rename(&t, &plan, &[]);
    assert_eq!(out.code(), Some(0), "{out:?}");
    assert_eq!(out.forms(), summary(""), "{out:?}");
    assert_eq!(t.snapshot(), before);
    assert_eq!(t.top(), top);
}

// §8.1, §10.5: a use in a macro body is refused: status 1, a `refusal`
// line, no `overlay` line, state `refused`; nothing written.
#[test]
fn s8_1_a_refused_run() {
    let t = Tree::three_files("refused");
    t.write("src/b.cpp", "#include \"h.h\"\n#define CALL(x) f(x)\nint g() { return CALL(2); }\n");
    let plan = t.plan("store");
    let before = t.snapshot();
    let out = rename(&t, &plan, &["--write"]);
    assert_eq!(out.code(), Some(1), "{out:?}");
    assert_eq!(out.value("state"), Some("refused"), "{out:?}");
    assert!(out.has_line("refusal "), "{out:?}");
    assert!(!out.has_line("overlay "), "{out:?}");
    assert!(!out.has_line("verify:"), "{out:?}");
    assert!(out.stdout.lines().any(|l| l == "state refused"));
    assert_eq!(t.snapshot(), before);
    assert!(out.stderr.contains("does not permit `--write`"), "{out:?}");
}

// §5.4: a name that one observer resolves to the target and another to
// something else in one shared header is contested: refused.
#[test]
fn s5_4_a_contested_name() {
    let t = Tree::new("contested");
    t.write("src/h.h", "#pragma once\nint f();\n");
    t.write("src/n.h", "#pragma once\nnamespace n { int f(); }\nusing n::f;\n");
    t.write("src/use.h", "inline int u() { return f(); }\n");
    t.write("src/a.cpp", "#include \"h.h\"\n#include \"use.h\"\nint f() { return 1; }\n");
    t.write("src/b.cpp", "#include \"n.h\"\n#include \"use.h\"\nint b() { return u(); }\n");
    t.db(&["src/a.cpp", "src/b.cpp"]);
    let plan = t.plan("store");
    let out = rename(&t, &plan, &[]);
    assert_eq!(out.code(), Some(1), "{out:?}");
    assert!(out.has_line("refused contested "), "{out:?}");
    assert!(out.has_line("refusal contested"), "{out:?}");
}

// §8.5: `--only` selects fewer observers than the plan has: a partial
// result, not written without `--allow-partial`, written with it.
#[test]
fn s8_5_a_partial_result() {
    let t = Tree::three_files("partial");
    t.write("src/c.cpp", "int c() { return 3; }\n");
    t.db(&["src/a.cpp", "src/b.cpp", "src/c.cpp"]);
    let plan = t.plan("store");
    let only = ["--only", "a.cpp", "--only", "b.cpp"];
    let before = t.snapshot();
    let out = rename(&t, &plan, &[&only[..], &["--write"]].concat());
    assert_eq!(out.code(), Some(1), "{out:?}");
    assert_eq!(out.value("map:"), Some("selected 2, ok 2, not ok 0, failed runs 0"));
    assert!(out.stdout.lines().any(|l| l == "state verified"), "{out:?}");
    assert!(out.stderr.contains("does not permit `--write`"), "{out:?}");
    assert_eq!(t.snapshot(), before);
    let out = rename(&t, &plan, &[&only[..], &["--write", "--allow-partial"]].concat());
    assert_eq!(out.code(), Some(0), "{out:?}");
    assert_eq!(t.read("src/h.h"), POST_H);
}

// §8.2, §8.5: an observer that does not parse. Without `--allow-unparsed`
// the result is `incomplete` (status 4); with it, the state the others give.
#[test]
fn s8_5_an_observer_that_does_not_parse() {
    let t = Tree::three_files("unparsed");
    t.write("src/c.cpp", "int broken( {\n");
    t.db(&["src/a.cpp", "src/b.cpp", "src/c.cpp"]);
    let plan = t.plan("store");
    let out = rename(&t, &plan, &["--write"]);
    assert_eq!(out.code(), Some(4), "{out:?}");
    assert_eq!(out.value("map:"), Some("selected 3, ok 2, not ok 1, failed runs 0"));
    assert!(out.stdout.lines().any(|l| l == "state incomplete"), "{out:?}");
    assert_eq!(t.read("src/h.h"), "#pragma once\nint f(int);\n");
    assert!(out.stderr.contains("c.cpp"), "the observer's line: {out:?}");
    let out = rename(&t, &plan, &["--write", "--allow-unparsed"]);
    assert_eq!(out.code(), Some(0), "{out:?}");
    assert!(out.stdout.lines().any(|l| l == "state verified"), "{out:?}");
    assert_eq!(t.read("src/h.h"), POST_H);
}

// §8.4: residue in a file no observer compiles: `verified_with_residue`,
// status 1 with and without `--allow-residue`; written only with it.
#[test]
fn s8_4_residue_in_a_file_no_observer_compiles() {
    let t = Tree::three_files("residue");
    t.write("src/unused.cpp", "int f(int);\nint z = f(1);\n");
    let plan = t.plan("store");
    let out = rename(&t, &plan, &["--write"]);
    assert_eq!(out.code(), Some(1), "{out:?}");
    assert!(out.stdout.lines().any(|l| l == "state verified_with_residue"), "{out:?}");
    assert_eq!(
        out.value("residue:"),
        Some("dependent 0, not_compiled 0, not_planned 2; mentions 0"),
        "{out:?}"
    );
    let unused = t.path("src/unused.cpp");
    let line = format!("residue not_planned: {}:2:9", unused.display());
    assert!(out.stdout.lines().any(|l| l == line), "{line}: {out:?}");
    assert_eq!(t.read("src/h.h"), "#pragma once\nint f(int);\n");
    let out = rename(&t, &plan, &["--write", "--allow-residue", "--relative-to", "src"]);
    assert_eq!(out.code(), Some(1), "{out:?}");
    assert!(out.has_line("written 3 file(s)"), "{out:?}");
    assert!(out.stdout.lines().any(|l| l == "residue not_planned: unused.cpp:1:5"), "{out:?}");
    assert_eq!(t.read("src/h.h"), POST_H);
    assert_eq!(t.read("src/unused.cpp"), "int f(int);\nint z = f(1);\n");
}

// §8.4, WP-118 (design row 39; second review N-a, X12): a rebuilt file is
// not scanned for residue: it is not owned, and its spelling is the
// build's to change. `src/gen/unused.cpp` spells `f` as `unused.cpp`
// does in the test above; declared rebuilt, it is no residue.
#[test]
fn s8_4_rebuilt_files_are_not_scanned() {
    let t = Tree::three_files("residue-rebuilt");
    t.write("src/gen/unused.cpp", "int f(int);\nint z = f(1);\n");
    let plan = t.plan("store");
    let out = rename(&t, &plan, &["--rebuilt", "src/gen"]);
    let unused = t.path("src/gen/unused.cpp").display().to_string();
    assert!(!out.stdout.contains(&unused), "{out:?}");
    assert_eq!(
        out.value("residue:"),
        Some("dependent 0, not_compiled 0, not_planned 0; mentions 0"),
        "{out:?}"
    );
}

// §8.4: a mention is listed and changes no state.
#[test]
fn s8_4_a_mention() {
    let t = Tree::three_files("mention");
    t.write("src/b.cpp", "#include \"h.h\"\nint g() { return f(2); } // calls f\n");
    let plan = t.plan("store");
    let out = rename(&t, &plan, &["--relative-to", "."]);
    assert_eq!(out.code(), Some(0), "{out:?}");
    assert_eq!(
        out.value("residue:"),
        Some("dependent 0, not_compiled 0, not_planned 0; mentions 1")
    );
    assert!(out.stdout.lines().any(|l| l == "residue mention: src/b.cpp:2:35"), "{out:?}");
    assert!(out.stdout.lines().any(|l| l == "state verified"), "{out:?}");
}

// §3, §10.1, exec.md §8.5: the usage errors, status 2, before the store
// is opened.
#[test]
fn s10_usage_errors() {
    let t = Tree::three_files("usage");
    let base = ["rename", "--plan", "abcd", "--owned", "src", "--store", "store"];
    let cases: &[(&[&str], &str)] = &[
        (&["--to", "g"], "`rename` needs `--from`"),
        (&["--from", "f"], "`rename` needs `--to`"),
        (&["--from", "f", "--to", "int"], "`--to`: `int`: a keyword of C++"),
        (&["--from", "f", "--to", "f"], "`--to`: `f`: the new name is the target's last"),
        (&["--from", "a::", "--to", "g"], "`--from`: `a::`: not an identifier"),
        (&["--from", "a b", "--to", "g"], "`--from`: `a b`: not an identifier"),
        (&["--from", "f", "--to", "g", "--kind", "type"], "`--kind`: `type`"),
        (&["--from", "f", "--to", "g", "--write", "--patch", "p"], "cannot both be given"),
    ];
    for (extra, message) in cases {
        let args = [&base[..], extra].concat();
        let out = t.slip(&args);
        assert_eq!(out.code(), Some(2), "{args:?}: {out:?}");
        assert!(out.stderr.starts_with("slip: "), "{out:?}");
        assert!(out.stderr.contains(message), "{args:?}: {message}: {out:?}");
        assert!(out.stderr.contains("usage:"), "{out:?}");
    }
    let out = t.slip(&["rename", "--plan", "abcd", "--from", "f", "--to", "g"]);
    assert_eq!(out.code(), Some(2), "{out:?}");
    assert!(out.stderr.contains("needs `--owned`"), "{out:?}");
    assert!(!t.path("store").exists(), "no store was made");
    let out = t.slip(&["map", "--plan", "abcd", "--transform", "x.cpp", "--owned", "src"]);
    assert_eq!(out.code(), Some(2), "{out:?}");
    assert!(
        out.stderr.starts_with("slip: map: --transform needs a slip built with slip_script\n"),
        "{out:?}"
    );
    assert!(!t.path("store").exists(), "no store was made");
}

// §10.1, §10.5: `map`'s three lines and its status with an observer that
// does not parse; `slip verify` on the round of a `map` is refused
// (exec.md §8.7); `next_round` after a `map` gives the next number.
#[test]
fn s10_1_map_alone() {
    let t = Tree::three_files("map");
    t.write("src/c.cpp", "int broken( {\n");
    t.db(&["src/a.cpp", "src/b.cpp", "src/c.cpp"]);
    let plan = t.plan("store");
    let out = t.slip(&[
        "map", "--plan", &plan, "--from", "f", "--to", "renamed", "--owned", "src", "--store",
        "store",
    ]);
    assert_eq!(out.code(), Some(1), "{out:?}");
    assert_eq!(
        out.forms(),
        ["transform <id>", "round 1", "map: selected 3, ok 2, not ok 1, failed runs 0"]
    );
    assert!(out.stderr.contains("c.cpp"), "{out:?}");
    let store = t.store("store");
    let plan_id = Digest::from_hex(&plan).unwrap();
    assert_eq!(store.next_round(&plan_id).unwrap(), 2);
    let records = transform_runs(&t, "store", &plan);
    assert_eq!(records.len(), 1);
    assert_eq!(steps(&records[0]), [Map]);
    drop(store);
    let out = t.slip(&["verify", "--plan", &plan, "--round", "1", "--store", "store"]);
    assert_eq!(out.code(), Some(3), "{out:?}");
    assert!(out.stderr.contains("is the round of a transform's run"), "{out:?}");
}

/// The transform run records of `plan` in `store`, in name order.
fn transform_runs(t: &Tree, store: &str, plan: &str) -> Vec<TransformRun> {
    let dir = t.path(store).join("runs").join(plan);
    let mut names: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().unwrap().to_str().unwrap().starts_with("transform."))
        .collect();
    names.sort();
    names.iter().map(|p| TransformRun::from_bytes(&std::fs::read(p).unwrap()).unwrap()).collect()
}

fn steps(r: &TransformRun) -> Vec<StepName> {
    r.steps.iter().map(|s| s.name).collect()
}

/// Every phase run record of `plan` in `store` by content id.
fn phase_runs(t: &Tree, store: &str, plan: &str) -> Vec<Digest> {
    std::fs::read_dir(t.path(store).join("runs").join(plan))
        .unwrap()
        .flatten()
        .filter(|e| !e.file_name().to_str().unwrap().starts_with("transform."))
        .map(|e| content_id(&std::fs::read(e.path()).unwrap()).unwrap())
        .collect()
}

// §10.6: one `RUNT` per invocation of `map`, `rename` and `check`, its
// steps in order, the phase records it names in the store.
#[test]
fn s10_6_the_run_records() {
    let t = Tree::three_files("runt");
    let plan = t.plan("store");
    let out = rename(&t, &plan, &["--write"]);
    assert_eq!(out.code(), Some(0), "{out:?}");
    let records = transform_runs(&t, "store", &plan);
    assert_eq!(records.len(), 1);
    let r = &records[0];
    assert_eq!(steps(r), [Map, Reduce, Apply, Verify, Scan, Conclude, Write]);
    assert_eq!(r.round, 1);
    assert_eq!(r.result.map(|d| d.to_string()), Some(out.id("result")));
    assert_eq!(r.edits.map(|d| d.to_string()), Some(out.id("edits")));
    assert_eq!(r.declared.map(|d| d.to_string()), Some(out.id("declared")));
    assert_eq!(r.overlay.map(|d| d.to_string()), Some(out.id("overlay")));
    let phases = phase_runs(&t, "store", &plan);
    for s in &r.steps {
        assert_eq!(s.run.is_some(), matches!(s.name, Map | Verify), "{s:?}");
        if let Some(id) = s.run {
            assert!(phases.contains(&id), "{s:?} names a record in the store");
        }
    }
    assert!(r.wall_ns >= r.steps.iter().map(|s| s.wall_ns).sum::<u64>());

    // `check` on the round: a second record of round 1.
    let out = t.slip(&[
        "check",
        "--plan",
        &plan,
        "--round",
        "1",
        "--edits",
        &r.edits.unwrap().to_string()[..8],
        "--store",
        "store",
    ]);
    // The tree is the post-image now: the edit set's files are stale.
    assert_eq!(out.code(), Some(3), "{out:?}");
    let records = transform_runs(&t, "store", &plan);
    assert_eq!(records.len(), 2, "a check that fails still publishes its record");
}

// §10.2: the stages one after another give the ids and the tree
// `rename --write` gives in a fresh store; `check` given the run's own
// edit set gives the same result id.
#[test]
fn s10_2_the_stages_give_renames_ids() {
    let t = Tree::three_files("stages");
    let before = t.snapshot();
    let plan_a = t.plan("store-a");
    let whole = t.slip(&[
        "rename", "--plan", &plan_a, "--from", "f", "--to", "renamed", "--owned", "src", "--store",
        "store-a", "--write",
    ]);
    assert_eq!(whole.code(), Some(0), "{whole:?}");
    let after = t.snapshot();
    for (rel, text) in &before {
        t.write(rel, text);
    }

    let plan = t.plan("store-b");
    assert_eq!(plan, plan_a);
    let map = t.slip(&[
        "map", "--plan", &plan, "--from", "f", "--to", "renamed", "--owned", "src", "--store",
        "store-b",
    ]);
    assert_eq!(map.code(), Some(0), "{map:?}");
    assert_eq!(map.id("transform"), whole.id("transform"));
    let reduce = t.slip(&["reduce", "--plan", &plan, "--round", "1", "--store", "store-b"]);
    assert_eq!(reduce.code(), Some(0), "{reduce:?}");
    assert_eq!(reduce.id("edits"), whole.id("edits"));
    assert_eq!(reduce.id("declared"), whole.id("declared"));
    let edits = reduce.id("edits");
    let check = t
        .slip(&["check", "--plan", &plan, "--round", "1", "--edits", &edits, "--store", "store-b"]);
    assert_eq!(check.code(), Some(0), "{check:?}");
    for key in ["transform", "edits", "declared", "overlay", "result"] {
        assert_eq!(check.id(key), whole.id(key), "{key}");
    }
    // Again: the same result.
    let again = t.slip(&[
        "check",
        "--plan",
        &plan,
        "--round",
        "1",
        "--edits",
        &edits[..6],
        "--declared",
        &reduce.id("declared")[..6],
        "--store",
        "store-b",
    ]);
    assert_eq!(again.id("result"), whole.id("result"), "{again:?}");
    let apply =
        t.slip(&["apply", "--result", &check.id("result"), "--write", "--store", "store-b"]);
    assert_eq!(apply.code(), Some(0), "{apply:?}");
    assert_eq!(t.snapshot(), after);
    let steps_b: Vec<Vec<StepName>> =
        transform_runs(&t, "store-b", &plan).iter().map(steps).collect();
    assert!(steps_b.contains(&vec![Map]), "{steps_b:?}");
    assert!(steps_b.contains(&vec![Apply, Verify, Scan, Conclude]), "{steps_b:?}");
}

/// The edit set `id` of `store`.
fn edit_set(t: &Tree, store: &str, id: &str) -> EditSet {
    let p = t.path(store).join("edits").join(format!("{id}.edts"));
    EditSet::from_bytes(&std::fs::read(p).unwrap()).unwrap()
}

/// `set` written to `name` in the tree; `check` of round 1 with it.
fn check_with(t: &Tree, plan: &str, set: &EditSet, name: &str) -> Out {
    set.validate().unwrap();
    std::fs::write(t.path(name), set.to_bytes()).unwrap();
    t.slip(&["check", "--plan", plan, "--round", "1", "--edits", name, "--store", "store"])
}

fn file_of(set: &EditSet, t: &Tree, rel: &str) -> u64 {
    set.file_index(t.path(rel).to_str().unwrap().as_bytes()).unwrap()
}

// §10.2: an edit set that is wrong on purpose. Without the definition's
// edit, `a.cpp` still declares `f` under its old name: `not_verified`
// with a `stale` line naming it.
#[test]
fn s10_2_check_without_an_edit_is_stale() {
    let t = Tree::three_files("missing-edit");
    let plan = t.plan("store");
    let out = rename(&t, &plan, &[]);
    assert_eq!(out.code(), Some(0), "{out:?}");
    let mut set = edit_set(&t, "store", &out.id("edits"));
    let a = file_of(&set, &t, "src/a.cpp");
    // The set names only files some record names: a.cpp goes, the files
    // after it move down by one.
    assert!(set.kept.is_empty() && set.unresolved.is_empty() && set.refusals.is_empty());
    assert!(set.changed.is_empty() && set.reports.is_empty() && set.failures.is_empty());
    set.edits.retain(|e| e.file != a);
    set.files.remove(a as usize);
    for e in &mut set.edits {
        if e.file > a {
            e.file -= 1;
        }
        assert!(e.pieces.iter().all(|p| !matches!(p, Piece::Copy(_))));
    }
    let out = check_with(&t, &plan, &set, "less.edts");
    assert_eq!(out.code(), Some(1), "{out:?}");
    assert!(out.stdout.lines().any(|l| l == "state not_verified"), "{out:?}");
    let suffix =
        format!(": still declared under its old name in {}", t.path("src/a.cpp").display());
    assert!(
        out.stdout.lines().any(|l| l.starts_with("stale ") && l.ends_with(&suffix)),
        "{suffix}: {out:?}"
    );
}

// §10.2: an edit added at a `kept` range (a name another observer
// resolved to something else): `not_verified`.
#[test]
fn s10_2_check_with_an_edit_at_a_kept_range() {
    let t = Tree::three_files("kept");
    t.write("src/n.h", "#pragma once\nnamespace n { int f(int); }\n");
    t.write("src/c.cpp", "#include \"n.h\"\nint c() { return n::f(3); }\n");
    t.db(&["src/a.cpp", "src/b.cpp", "src/c.cpp"]);
    let plan = t.plan("store");
    let out = rename(&t, &plan, &[]);
    assert_eq!(out.code(), Some(0), "{out:?}");
    // `n::f` at its declaration in n.h and at its use in c.cpp.
    assert_eq!(out.value("kept"), Some("2; unresolved 0"), "{out:?}");
    let mut set = edit_set(&t, "store", &out.id("edits"));
    let kept = set.kept[0];
    let mut edit = set.edits[0].clone();
    edit.file = kept.file;
    edit.lo = kept.lo;
    edit.hi = kept.hi;
    edit.pieces = vec![Piece::Literal(b"renamed".to_vec())];
    set.kept.remove(0);
    set.edits.push(edit);
    set.edits.sort_by_key(|e| (e.file, e.lo));
    let out = check_with(&t, &plan, &set, "more.edts");
    assert_eq!(out.code(), Some(1), "{out:?}");
    assert!(out.stdout.lines().any(|l| l == "state not_verified"), "{out:?}");
}

// §10.2, verify.md §10: a spelled piece whose key is replaced by another
// entity's: `not_verified` with a `spelling denotes_other` line.
#[test]
fn s10_2_check_with_a_spelling_of_another_entity() {
    let t = Tree::three_files("spelling");
    let plan = t.plan("store");
    let out = rename(&t, &plan, &[]);
    assert_eq!(out.code(), Some(0), "{out:?}");
    // `g`'s key: the subject of a rename of `g` in another store.
    let plan_g = t.plan("store-g");
    let g = t.slip(&[
        "rename", "--plan", &plan_g, "--from", "g", "--to", "gg", "--owned", "src", "--store",
        "store-g",
    ]);
    let g_key = edit_set(&t, "store-g", &g.id("edits")).subjects[0].id;
    let mut set = edit_set(&t, "store", &out.id("edits"));
    let mut replaced = 0;
    for e in &mut set.edits {
        for p in &mut e.pieces {
            if let Piece::Spelled { key, .. } = p {
                *key = g_key;
                replaced += 1;
            }
        }
    }
    assert_eq!(replaced, 1);
    let out = check_with(&t, &plan, &set, "other.edts");
    assert_eq!(out.code(), Some(1), "{out:?}");
    assert!(out.stdout.lines().any(|l| l == "state not_verified"), "{out:?}");
    let prefix = format!("spelling denotes_other: {}:2:18: meant ", t.path("src/b.cpp").display());
    assert!(out.stdout.lines().any(|l| l.starts_with(&prefix)), "{prefix}: {out:?}");
}

// WP-076 "Tests": a second declaration of a variable in another source
// file that the rename is made to miss: the `stale` line names the
// observer.
#[test]
fn s9_2_a_second_declaration_missed() {
    let t = Tree::new("variable");
    t.write("src/h.h", "#pragma once\nextern int v;\n");
    t.write("src/a.cpp", "#include \"h.h\"\nint v = 1;\n");
    t.write("src/b.cpp", "#include \"h.h\"\nextern int v;\nint g() { return v; }\n");
    t.db(&["src/a.cpp", "src/b.cpp"]);
    let plan = t.plan("store");
    let out = t.slip(&[
        "rename", "--plan", &plan, "--from", "v", "--to", "w", "--kind", "variable", "--owned",
        "src", "--store", "store",
    ]);
    assert_eq!(out.code(), Some(0), "{out:?}");
    let mut set = edit_set(&t, "store", &out.id("edits"));
    let b = file_of(&set, &t, "src/b.cpp");
    let n = set.edits.len();
    // The redeclaration on line 2 of b.cpp, not the use on line 3.
    let second_line = "#include \"h.h\"\n".len() as u64;
    set.edits.retain(|e| !(e.file == b && e.lo < second_line + 14));
    assert_eq!(set.edits.len(), n - 1);
    let out = check_with(&t, &plan, &set, "missed.edts");
    assert_eq!(out.code(), Some(1), "{out:?}");
    let line =
        format!("stale w: still declared under its old name in {}", t.path("src/b.cpp").display());
    assert!(
        out.stdout.lines().any(|l| l.starts_with("stale ") && l.ends_with("src/b.cpp")),
        "{line}: {out:?}"
    );
}

/// A change made to an edit set.
type Change = fn(&mut EditSet, Digest);

// §10.2: the edit set's plan, round and transform must be the round's.
#[test]
fn s10_2_check_refuses_another_rounds_set() {
    let t = Tree::three_files("other-round");
    let plan = t.plan("store");
    let out = rename(&t, &plan, &[]);
    let set = edit_set(&t, "store", &out.id("edits"));
    let other = Digest([0x5a; 32]);
    let changes: [(&str, Change); 3] = [
        ("plan", |s, d| s.plan = d),
        ("round", |s, _| s.round = 2),
        ("transform", |s, d| s.transform = d),
    ];
    for (field, change) in changes {
        let mut wrong = set.clone();
        change(&mut wrong, other);
        let out = check_with(&t, &plan, &wrong, &format!("{field}.edts"));
        assert_eq!(out.code(), Some(3), "{field}: {out:?}");
        let why = format!("its {field} is not the round's");
        assert!(out.stderr.contains(&why), "{why}: {out:?}");
    }
    // Decision 1 (fix round, should-fix 5): each refusal published its
    // record of the round, with no step.
    let records = transform_runs(&t, "store", &plan);
    assert_eq!(records.len(), 4);
    for r in &records[1..] {
        assert!(r.steps.is_empty(), "{r:?}");
        assert_eq!(r.result, None);
    }
}

// §1 rule 3: `-j 1` and `-j 4` in fresh stores give the same five ids and
// the same standard output.
#[test]
fn s1_determinism_of_the_threads() {
    let t = Tree::three_files("threads");
    t.write("src/c.cpp", "#include \"h.h\"\nint c() { return f(3) + f(4); } // f\n");
    t.write("src/unused.h", "int f(int);\n");
    t.db(&["src/a.cpp", "src/b.cpp", "src/c.cpp"]);
    let mut outs = Vec::new();
    for (store, j) in [("store-1", "1"), ("store-4", "4")] {
        let plan = t.plan(store);
        let out = t.slip(&[
            "rename", "--plan", &plan, "--from", "f", "--to", "renamed", "--owned", "src",
            "--store", store, "-j", j,
        ]);
        assert_eq!(out.code(), Some(1), "{out:?}");
        outs.push(out);
    }
    for key in ["transform", "edits", "declared", "overlay", "result"] {
        assert_eq!(outs[0].id(key), outs[1].id(key), "{key}");
    }
    assert_eq!(outs[0].stdout, outs[1].stdout.replace("store-4", "store-1"));
}

// §8.5: the result decoded from the store is the one printed.
#[test]
fn s8_5_the_published_result() {
    let t = Tree::three_files("result");
    let plan = t.plan("store");
    let out = rename(&t, &plan, &[]);
    let id = out.id("result");
    let bytes = std::fs::read(t.path("store").join("results").join(format!("{id}.rslt"))).unwrap();
    let r = TransformResult::from_bytes(&bytes).unwrap();
    assert_eq!(r.state, State::Verified);
    assert!(!r.partial);
    assert_eq!(r.tally.verified, 2);
    let decl = t.path("store").join("declared").join(format!("{}.decl", out.id("declared")));
    let d = Declared::from_bytes(&std::fs::read(decl).unwrap()).unwrap();
    assert_eq!(d.rules().rules().len(), 1);
    let plan_bytes = std::fs::read(t.path("store").join("plans").join(format!("{plan}.plan")));
    assert_eq!(Plan::from_bytes(&plan_bytes.unwrap()).unwrap().observers.len(), 2);
}

// ----- interruption (exec.md §7, §10.6) ----------------------------------------

/// The pids of `pid`'s children whose command line contains `arg`.
fn children_with(pid: u32, arg: &str) -> Vec<u32> {
    let Ok(dir) = std::fs::read_dir("/proc") else { return Vec::new() };
    dir.flatten()
        .filter_map(|e| e.file_name().to_str()?.parse::<u32>().ok())
        .filter(|p| {
            let stat = std::fs::read_to_string(format!("/proc/{p}/stat")).unwrap_or_default();
            let parent = stat.rsplit_once(')').and_then(|(_, r)| r.split_whitespace().nth(1));
            parent == Some(&pid.to_string())
                && std::fs::read(format!("/proc/{p}/cmdline"))
                    .is_ok_and(|c| String::from_utf8_lossy(&c).contains(arg))
        })
        .collect()
}

/// Starts `slip args` in the tree, waits for a worker, interrupts it with
/// `SIGINT` and returns its output.
fn interrupted(t: &Tree, args: &[&str]) -> Out {
    let mut cmd = Command::new(SLIP);
    cmd.args(args)
        .current_dir(&t.root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().unwrap();
    let stdout = reader(child.stdout.take().unwrap());
    let stderr = reader(child.stderr.take().unwrap());
    let pid = child.id();
    let deadline = Instant::now() + Duration::from_mins(1);
    while children_with(pid, "__worker").is_empty() {
        assert!(Instant::now() < deadline, "no worker started");
        std::thread::sleep(Duration::from_millis(20));
    }
    // The worker blocks on the pipe; give it the time to get there.
    std::thread::sleep(Duration::from_millis(300));
    let sent = Command::new("kill").args(["-INT", &pid.to_string()]).status().unwrap();
    assert!(sent.success());
    let status = wait_bounded(&mut child, Duration::from_mins(1));
    Out { status, stdout: stdout.join().unwrap(), stderr: stderr.join().unwrap() }
}

fn mkfifo(t: &Tree, rel: &str) {
    let made = Command::new("mkfifo").arg(t.path(rel)).status().expect("mkfifo");
    assert!(made.success());
}

// §10.6, exec.md §7: `rename` interrupted during its map phase: killed by
// the signal, a transform run record of the round, no edit set (WP-157),
// no result, the tree untouched.
#[test]
fn s10_6_rename_interrupted_in_map() {
    use std::os::unix::process::ExitStatusExt as _;
    let t = Tree::three_files("sigint-map");
    t.write("src/b.cpp", "#include \"h.h\"\n#include \"pipe.h\"\nint g() { return f(2); }\n");
    let plan = t.plan("store");
    mkfifo(&t, "src/pipe.h");
    let out = interrupted(
        &t,
        &[
            "rename", "--plan", &plan, "--from", "f", "--to", "renamed", "--owned", "src",
            "--store", "store", "--write", "-j", "2",
        ],
    );
    assert_eq!(out.status.signal(), Some(2), "{out:?}");
    let records = transform_runs(&t, "store", &plan);
    assert_eq!(records.len(), 1, "{out:?}");
    assert_eq!(steps(&records[0]), [Map]);
    assert_eq!(records[0].result, None);
    // WP-157, item 2: `rename`'s reduce step is not reached after a map
    // phase that did not finish (the pool returns no summary then): no
    // edit set, and the record names none.
    assert_eq!(records[0].edits, None);
    assert_eq!(edit_sets(&t), 0);
    let results = t.path("store").join("results");
    assert!(!results.exists() || std::fs::read_dir(&results).unwrap().next().is_none());
    assert_eq!(t.read("src/h.h"), "#pragma once\nint f(int);\n");
    assert!(!t.path("store").join("lock").exists());
}

// §10.6, WP-076 "Cancellation": `check` interrupted during its verify
// phase still concludes and publishes an `incomplete` result, with its
// record; killed by the signal.
#[test]
fn s10_6_check_interrupted_in_verify() {
    use std::os::unix::process::ExitStatusExt as _;
    let t = Tree::three_files("sigint-verify");
    t.write("src/other.h", "inline int other() { return 0; }\n");
    t.write("src/b.cpp", "#include \"h.h\"\n#include \"other.h\"\nint g() { return f(2); }\n");
    let plan = t.plan("store");
    let map = t.slip(&[
        "map", "--plan", &plan, "--from", "f", "--to", "renamed", "--owned", "src", "--store",
        "store",
    ]);
    assert_eq!(map.code(), Some(0), "{map:?}");
    let reduce = t.slip(&["reduce", "--plan", &plan, "--round", "1", "--store", "store"]);
    assert_eq!(reduce.code(), Some(0), "{reduce:?}");
    // `other.h` is no file of the overlay: the verify job of `b.cpp`
    // reads it from disk, and blocks.
    std::fs::remove_file(t.path("src/other.h")).unwrap();
    mkfifo(&t, "src/other.h");
    let out = interrupted(
        &t,
        &[
            "check",
            "--plan",
            &plan,
            "--round",
            "1",
            "--edits",
            &reduce.id("edits"),
            "--store",
            "store",
            "-j",
            "2",
        ],
    );
    assert_eq!(out.status.signal(), Some(2), "{out:?}");
    let records = transform_runs(&t, "store", &plan);
    assert_eq!(records.len(), 2, "{out:?}");
    let r = &records[1];
    assert_eq!(steps(r), [Apply, Verify, Conclude], "{r:?}");
    let id = r.result.expect("a result");
    let bytes = std::fs::read(t.path("store").join("results").join(format!("{id}.rslt"))).unwrap();
    let result = TransformResult::from_bytes(&bytes).unwrap();
    assert_eq!(result.state, State::Incomplete);
    assert_eq!(result.verdict, None);
    assert!(out.stdout.lines().any(|l| l == "state incomplete"), "{out:?}");
    assert!(!t.path("store").join("lock").exists());
}

// §1 rule 3, §8.4: the scan step's residue does not depend on the threads:
// one thread and eight over many files give the residue the result holds.
#[test]
fn s8_4_residue_does_not_depend_on_the_threads() {
    use slip_cli::transform::residue::{extensions, residue};
    let t = Tree::three_files("residue-threads");
    for i in 0..24 {
        t.write(&format!("src/extra/e{i:02}.h"), "int f(int); // f\nint g(int f);\n");
    }
    let plan = t.plan("store");
    let out = rename(&t, &plan, &[]);
    assert_eq!(out.code(), Some(1), "{out:?}");
    let set = edit_set(&t, "store", &out.id("edits"));
    let trfm = t.path("store").join("transforms").join(format!("{}.trfm", out.id("transform")));
    let transform = slip_model::Transform::from_bytes(&std::fs::read(trfm).unwrap()).unwrap();
    let result = t.path("store").join("results").join(format!("{}.rslt", out.id("result")));
    let result = TransformResult::from_bytes(&std::fs::read(result).unwrap()).unwrap();
    assert_eq!(result.residue.len(), 24 * 3, "two not_planned and a mention a file");
    // The files the observers read, as the run gives them: the residue of
    // these files is the result's.
    let mut read: Vec<Vec<u8>> = ["src/a.cpp", "src/b.cpp", "src/h.h"]
        .iter()
        .map(|r| t.path(r).to_str().unwrap().as_bytes().to_vec())
        .collect();
    read.sort();
    let ext = extensions(None);
    let one = residue(&transform, &set, &read, &[], &ext, 1).unwrap();
    let eight = residue(&transform, &set, &read, &[], &ext, 8).unwrap();
    assert_eq!(one, eight);
    assert_eq!(one, result.residue);
}

// §10.2, WP-076 "Decisions" 7: a refused set given to `check` with
// `--declared` concludes `refused`: no overlay is built, no verify phase
// runs, the declared changes are not read.
#[test]
fn s10_2_check_of_a_refused_set_with_declared() {
    let t = Tree::three_files("refused-declared");
    t.write("src/b.cpp", "#include \"h.h\"\n#define CALL(x) f(x)\nint g() { return CALL(2); }\n");
    let plan = t.plan("store");
    let out = rename(&t, &plan, &[]);
    assert_eq!(out.value("state"), Some("refused"), "{out:?}");
    std::fs::write(t.path("none.decl"), Declared::none().to_bytes()).unwrap();
    let out = t.slip(&[
        "check",
        "--plan",
        &plan,
        "--round",
        "1",
        "--edits",
        &out.id("edits"),
        "--declared",
        "none.decl",
        "--store",
        "store",
    ]);
    assert_eq!(out.code(), Some(1), "{out:?}");
    assert!(!out.has_line("overlay "), "{out:?}");
    assert!(!out.has_line("declared "), "{out:?}");
    assert!(!out.has_line("verify:"), "{out:?}");
    assert!(out.stdout.lines().any(|l| l == "state refused"), "{out:?}");
    let records = transform_runs(&t, "store", &plan);
    assert_eq!(steps(records.last().unwrap()), [Conclude]);
}

/// Reads the fifo at `path` until its writer has written and closed it,
/// within `bound`; what was read.
fn drain_fifo(path: &Path, bound: Duration) -> Vec<u8> {
    use std::io::ErrorKind;
    use std::os::unix::fs::OpenOptionsExt as _;
    const O_NONBLOCK: i32 = 0o4000;
    let mut f = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(O_NONBLOCK)
        .open(path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let deadline = Instant::now() + bound;
    let mut got = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        assert!(Instant::now() < deadline, "the fifo was not written and closed");
        match f.read(&mut buf) {
            Ok(0) if !got.is_empty() => return got,
            Ok(0) => std::thread::sleep(Duration::from_millis(10)),
            Ok(n) => got.extend_from_slice(&buf[..n]),
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => panic!("{}: {e}", path.display()),
        }
    }
}

// WP-076 fix round, B1 (§8.4, §8.5; exec.md §7): `rename --write`
// interrupted between its verify phase and its scan. The report's
// temporary file is a fifo, so the command is held after the verify
// phase, with every verdict in, until the test reads it; the signal
// arrives meanwhile. The scan does not run, and a scan that did not run
// concludes `incomplete`, never `verified`: nothing is written, and the
// result cannot be applied. The signal path waits for the result to be
// published (`PUBLISH_WAIT`): at 0 there is no result and this fails.
#[test]
fn s10_6_rename_interrupted_after_its_verify_phase() {
    use std::os::unix::process::ExitStatusExt as _;
    let t = Tree::three_files("sigint-scan");
    let plan = t.plan("store");
    let before = t.snapshot();
    let mut cmd = Command::new(SLIP);
    cmd.args([
        "rename", "--plan", &plan, "--from", "f", "--to", "renamed", "--owned", "src", "--store",
        "store", "-j", "2", "--report", "rep.json", "--write",
    ])
    .current_dir(&t.root)
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    let mut child = cmd.spawn().unwrap();
    let pid = child.id();
    // The map phase alone takes far longer than this: the fifo is there
    // before the report is written (the read below proves it).
    let tmp = format!(".rep.json.{pid}.tmp");
    mkfifo(&t, &tmp);
    let stdout = reader(child.stdout.take().unwrap());
    let stderr = reader(child.stderr.take().unwrap());
    let runs = t.path("store").join("runs").join(&plan);
    let deadline = Instant::now() + Duration::from_mins(1);
    let verified = || {
        std::fs::read_dir(&runs).is_ok_and(|d| {
            d.flatten().any(|e| e.file_name().to_str().unwrap().starts_with("verify.1."))
        })
    };
    while !verified() {
        assert!(Instant::now() < deadline, "no verify phase record");
        std::thread::sleep(Duration::from_millis(20));
    }
    std::thread::sleep(Duration::from_millis(300));
    let sent = Command::new("kill").args(["-INT", &pid.to_string()]).status().unwrap();
    assert!(sent.success());
    // Well inside the signal path's wait for the result.
    std::thread::sleep(Duration::from_secs(1));
    let report = drain_fifo(&t.path(&tmp), Duration::from_secs(10));
    assert!(!report.is_empty());
    let status = wait_bounded(&mut child, Duration::from_mins(1));
    let out = Out { status, stdout: stdout.join().unwrap(), stderr: stderr.join().unwrap() };
    assert_eq!(out.status.signal(), Some(2), "{out:?}");
    let records = transform_runs(&t, "store", &plan);
    assert_eq!(records.len(), 1, "{out:?}");
    let r = &records[0];
    assert_eq!(steps(r), [Map, Reduce, Apply, Verify, Conclude], "{r:?}");
    let id = r.result.expect("a result");
    let bytes = std::fs::read(t.path("store").join("results").join(format!("{id}.rslt"))).unwrap();
    let result = TransformResult::from_bytes(&bytes).unwrap();
    assert_eq!(result.state, State::Incomplete);
    assert_eq!(result.verdict, None);
    assert!(result.residue.is_empty());
    assert_eq!(t.snapshot(), before, "nothing written");
    assert!(!t.path("store").join("lock").exists());
    let apply = t.slip(&["apply", "--result", &id.to_string(), "--store", "store", "--write"]);
    assert_eq!(apply.code(), Some(1), "{apply:?}");
    assert_eq!(t.snapshot().get("src/h.h"), before.get("src/h.h"));
}

// ----- unchanged by identity (§8.2; WP-076 fix round, B2) -------------------

/// The reviewer's probe: `f` only in `a.cpp`; the set given to `check`
/// adds `h.h` with an edit in a group only `b.cpp` compiles, and `b.cpp`
/// reaches `h.h` as `spelled`. No rules: `b.cpp` read an overlay file by
/// identity, so it is not `unchanged`, and the result is not `verified`.
/// `None` when the volume does not make `spelled` the file `h.h` (a
/// case-sensitive volume, for `H.h`).
fn unchanged_by_identity(name: &str, spelled: &str, link: bool) -> Option<TransformResult> {
    // A hard link only under the work root; another case only where the
    // volume folds (the folding root, which never holds a link).
    let t = if link { Tree::new(name) } else { Tree::new_in(&common::folding_root(), name) };
    let h = "#pragma once\n#ifdef B_SIDE\ninline int k() { return 1; }\n#endif\n";
    t.write("src/h.h", h);
    if link {
        // Identity; `rename` without `--write` replaces neither name (spec/harness.md §11.4).
        std::fs::hard_link(t.path("src/h.h"), t.path(&format!("src/{spelled}"))).unwrap();
    } else if !t.path(&format!("src/{spelled}")).exists() {
        return None;
    }
    t.write("src/a.cpp", "#include \"h.h\"\nint f(int x) { return x + 1; }\n");
    t.write(
        "src/b.cpp",
        &format!("#define B_SIDE\n#include \"{spelled}\"\nint g() {{ return k(); }}\n"),
    );
    t.db(&["src/a.cpp", "src/b.cpp"]);
    let plan = t.plan("store");
    let out = rename(&t, &plan, &[]);
    let mut set = edit_set(&t, "store", &out.id("edits"));
    assert_eq!(set.files.len(), 1, "{out:?}");
    for e in &mut set.edits {
        e.pieces = vec![Piece::Literal(b"f".to_vec())];
    }
    let mut fr = set.files[0].clone();
    fr.path = t.path("src/h.h").to_str().unwrap().as_bytes().to_vec();
    fr.size = h.len() as u64;
    fr.content = slip_model::hash::content_digest(h.as_bytes());
    set.files.push(fr);
    set.files.sort_by(|a, b| a.path.cmp(&b.path));
    let hf = file_of(&set, &t, "src/h.h");
    let af = file_of(&set, &t, "src/a.cpp");
    for e in &mut set.edits {
        e.file = af;
    }
    let mut extra = set.edits[0].clone();
    let at = h.find("return 1").unwrap() as u64 + 7;
    extra.file = hf;
    extra.lo = at;
    extra.hi = at + 1;
    extra.pieces = vec![Piece::Literal(b"2".to_vec())];
    set.edits.push(extra);
    set.edits.sort_by_key(|e| (e.file, e.lo));
    set.validate().unwrap();
    std::fs::write(t.path("none.decl"), Declared::none().to_bytes()).unwrap();
    std::fs::write(t.path("k.edts"), set.to_bytes()).unwrap();
    let out = t.slip(&[
        "check",
        "--plan",
        &plan,
        "--round",
        "1",
        "--edits",
        "k.edts",
        "--declared",
        "none.decl",
        "--store",
        "store",
    ]);
    let id = out.id("result");
    let bytes = std::fs::read(t.path("store").join("results").join(format!("{id}.rslt"))).unwrap();
    let r = TransformResult::from_bytes(&bytes).unwrap();
    assert_eq!(r.tally.unchanged, 0, "b.cpp read the overlay's h.h: {out:?}");
    assert_eq!(r.tally.failed, 0, "{out:?}");
    assert_eq!(r.tally.verified, 1, "a.cpp only: {out:?}");
    if link {
        // `check` gave it a job (by identity), rather than leaving the
        // report to refuse `unchanged` (`failed`): the job (WP-074's O2)
        // says `unverifiable`, the file being another name of the
        // overlay's.
        assert_eq!(r.tally.unverifiable, 1, "{out:?}");
        assert!(
            out.stderr.contains(&format!("{spelled} is another name of the overlay's ")),
            "{out:?}"
        );
    } else {
        // WP-147: a file is read under the spelling its directory gives
        // it, so `b.cpp`'s `H.h` is the overlay's `h.h` by path: the job
        // parses the post-image and finds `k()` changed.
        assert_eq!(r.tally.mismatch, 1, "{out:?}");
        assert_eq!(r.tally.unverifiable, 0, "{out:?}");
        assert!(!out.stderr.contains("is another name of the overlay's "), "{out:?}");
    }
    assert!(
        !matches!(r.state, State::Verified | State::VerifiedWithResidue),
        "{:?}: {out:?}",
        r.state
    );
    Some(r)
}

#[test]
fn s8_2_unchanged_by_identity_case_spelling() {
    if common::folding_volume("s8_2_unchanged_by_identity_case_spelling").is_none() {
        return;
    }
    assert!(unchanged_by_identity("identity-case", "H.h", false).is_some());
}

#[test]
fn s8_2_unchanged_by_identity_hard_link() {
    assert!(unchanged_by_identity("identity-link", "hl.h", true).is_some());
}

// ----- an interrupted map (WP-076 fix round, should-fix 4) ------------------

/// The map shards of round 1 of `plan` in the store.
fn map_shards(t: &Tree, plan: &str) -> usize {
    let dir = t.path("store").join("shards").join(plan).join("map.1");
    std::fs::read_dir(dir).map_or(0, |d| d.flatten().count())
}

/// WP-076's should-fix 4 (WP-157's fixture): `map -j 1` over four
/// observers (a job at a time, in plan order), interrupted once two have
/// their shards and the third's job blocks on a fifo: that job is
/// cancelled, the fourth never starts. Every unit calls `f` through
/// `call` (`f(x)`, or a macro that makes the edit set refused). The tree,
/// the plan's id and the main files in plan order.
fn interrupted_map(name: &str, call: &str) -> (Tree, String, Vec<String>) {
    use std::os::unix::process::ExitStatusExt as _;
    let t = Tree::new(name);
    let define = if call.starts_with("CALL") { "#define CALL(x) f(x)\n" } else { "" };
    t.write("src/h.h", &format!("#pragma once\nint f(int);\n{define}"));
    let units = ["src/a.cpp", "src/b.cpp", "src/c.cpp", "src/d.cpp"];
    for (k, u) in units.iter().enumerate() {
        let n = &u[4..5];
        t.write(&format!("src/p{n}.h"), "\n");
        t.write(
            u,
            &format!(
                "#include \"h.h\"\n#include \"p{n}.h\"\nint g{k}(int x) {{ return {call}; }}\n"
            ),
        );
    }
    t.db(&units);
    let plan = t.plan("store");
    let bytes = std::fs::read(t.path("store").join("plans").join(format!("{plan}.plan"))).unwrap();
    let order: Vec<String> = Plan::from_bytes(&bytes)
        .unwrap()
        .observers
        .iter()
        .map(|o| String::from_utf8_lossy(&o.main.path).into_owned())
        .collect();
    // The third observer's own header becomes a fifo.
    let third = &order[2];
    let n = &third[third.len() - 5..third.len() - 4];
    let fifo = format!("src/p{n}.h");
    std::fs::remove_file(t.path(&fifo)).unwrap();
    mkfifo(&t, &fifo);
    let mut cmd = Command::new(SLIP);
    cmd.args([
        "map", "--plan", &plan, "--from", "f", "--to", "renamed", "--owned", "src", "--store",
        "store", "-j", "1",
    ])
    .current_dir(&t.root)
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null());
    let mut child = cmd.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_mins(1);
    while map_shards(&t, &plan) < 2 {
        assert!(Instant::now() < deadline, "no two map shards");
        std::thread::sleep(Duration::from_millis(20));
    }
    std::thread::sleep(Duration::from_millis(300));
    let sent = Command::new("kill").args(["-INT", &child.id().to_string()]).status().unwrap();
    assert!(sent.success());
    let status = wait_bounded(&mut child, Duration::from_mins(1));
    assert_eq!(status.signal(), Some(2));
    assert_eq!(map_shards(&t, &plan), 2);
    std::fs::remove_file(t.path(&fifo)).unwrap();
    t.write(&fifo, "\n");
    (t, plan, order)
}

/// The edit sets in the store.
fn edit_sets(t: &Tree) -> usize {
    std::fs::read_dir(t.path("store").join("edits")).map_or(0, |d| d.flatten().count())
}

/// The round's map shards as `slip_reduce` reads them, an absent one
/// `None`: what `reduce` did before WP-157.
struct AsBefore {
    store: Store,
    plan: Digest,
    observers: Vec<Digest>,
}

impl slip_reduce::Shards for AsBefore {
    fn open(&self, index: usize) -> Result<Option<Vec<u8>>, String> {
        self.store
            .read_shard(&self.plan, Phase::Map, 1, &self.observers[index])
            .map_err(|e| e.to_string())
    }

    fn len(&self) -> usize {
        self.observers.len()
    }

    fn observer(&self, index: usize) -> Digest {
        self.observers[index]
    }
}

/// The edit set `reduce` made of round 1 before WP-157, from the shards
/// there are: the set a `check` of an unfinished round is given.
fn reduced_as_before(t: &Tree, plan: &str) -> EditSet {
    let store = t.store("store");
    let plan = Digest::from_hex(plan).unwrap();
    let bytes = std::fs::read(t.path("store").join("plans").join(format!("{plan}.plan"))).unwrap();
    let observers: Vec<Digest> =
        Plan::from_bytes(&bytes).unwrap().observers.iter().map(|o| o.id).collect();
    let request = observers
        .iter()
        .find_map(|o| store.read_shard(&plan, Phase::Map, 1, o).unwrap())
        .map(|b| Shard::from_bytes(&b).unwrap().edits.unwrap().transform)
        .unwrap();
    let trfm = t.path("store").join("transforms").join(format!("{request}.trfm"));
    let transform = slip_model::Transform::from_bytes(&std::fs::read(trfm).unwrap()).unwrap();
    let scratch = t.path("reduce-scratch");
    std::fs::create_dir_all(&scratch).unwrap();
    let shards = AsBefore { store, plan, observers };
    let options = slip_reduce::Options::new(scratch);
    slip_reduce::reduce(&transform, request, plan, 1, &shards, &options).unwrap().edits
}

// spec/edits.md §8.2, §10.2 (WP-157): `reduce` after an interrupted map
// refuses the round: status 3, the message names the main file of an
// observer with no shard, and the store has no edit set, whatever the
// threads. `--allow-unparsed` is not `reduce`'s option (exec.md §8): a
// usage error, and no edit set either.
#[test]
fn s10_2_reduce_refuses_an_interrupted_map() {
    let (t, plan, order) = interrupted_map("sigint-map-reduce", "f(x)");
    for jobs in ["1", "4"] {
        let reduce =
            t.slip(&["reduce", "--plan", &plan, "--round", "1", "--store", "store", "-j", jobs]);
        assert_eq!(reduce.code(), Some(3), "-j {jobs}: {reduce:?}");
        // The least selected position with no shard: the third observer.
        assert!(
            reduce.stderr.contains(&format!(
                "{}: no map shard of round 1: the round's map phase did not finish",
                order[2]
            )),
            "{reduce:?}"
        );
        assert!(!reduce.has_line("edits "), "{reduce:?}");
        assert_eq!(edit_sets(&t), 0, "-j {jobs}");
    }
    let reduce = t.slip(&[
        "reduce",
        "--plan",
        &plan,
        "--round",
        "1",
        "--store",
        "store",
        "--allow-unparsed",
    ]);
    assert_eq!(reduce.code(), Some(2), "{reduce:?}");
    assert!(reduce.stderr.contains("unknown option `--allow-unparsed`"), "{reduce:?}");
    assert_eq!(edit_sets(&t), 0);
    // No record names an edit set: `reduce` publishes none, and `map`'s
    // has none.
    let records = transform_runs(&t, "store", &plan);
    assert!(records.iter().all(|r| r.edits.is_none()), "{records:?}");
}

// §8.2, §8.5; exec.md §7: `check` of the same round, given the edit set
// `reduce` made of it before WP-157: an observer with no map shard of the
// round did not finish the phase; it is no unparsed observer, and `check`
// refuses the round whatever `--allow-unparsed` says.
#[test]
fn s8_5_an_interrupted_map_is_not_unparsed() {
    let (t, plan, order) = interrupted_map("sigint-map-check", "f(x)");
    let set = reduced_as_before(&t, &plan);
    assert_eq!(set.state, SetState::Accepted);
    std::fs::write(t.path("before.edts"), set.to_bytes()).unwrap();
    for extra in [&[][..], &["--allow-unparsed"][..]] {
        let mut args = vec![
            "check",
            "--plan",
            &plan,
            "--round",
            "1",
            "--edits",
            "before.edts",
            "--store",
            "store",
        ];
        args.extend_from_slice(extra);
        let check = t.slip(&args);
        assert_eq!(check.code(), Some(3), "{extra:?}: {check:?}");
        assert!(
            order[2..].iter().any(|m| check.stderr.contains(&format!(
                "{m}: no map shard of round 1: the round's map phase did not finish"
            ))),
            "{check:?}"
        );
        assert!(!check.has_line("result "), "{check:?}");
        assert!(!check.has_line("verify:"), "{check:?}");
    }
    let results = t.path("store").join("results");
    assert!(!results.exists() || std::fs::read_dir(&results).unwrap().next().is_none());
}

// §8.1, §8.2 (WP-157, item 4): a refused set is concluded with no apply
// step, so `check` reads no map shard for it; it refuses an unfinished
// round all the same, before it concludes, rather than take the missing
// observers for unparsed ones in a `refused` result.
#[test]
fn s8_1_check_of_a_refused_set_of_an_interrupted_map() {
    let (t, plan, order) = interrupted_map("sigint-map-refused", "CALL(x)");
    let set = reduced_as_before(&t, &plan);
    assert_eq!(set.state, SetState::Refused);
    assert_eq!(set.unparsed.len(), 2, "the old reduce took them for unparsed");
    std::fs::write(t.path("before.edts"), set.to_bytes()).unwrap();
    for extra in [&[][..], &["--allow-unparsed"][..]] {
        let mut args = vec![
            "check",
            "--plan",
            &plan,
            "--round",
            "1",
            "--edits",
            "before.edts",
            "--store",
            "store",
        ];
        args.extend_from_slice(extra);
        let check = t.slip(&args);
        assert_eq!(check.code(), Some(3), "{extra:?}: {check:?}");
        assert!(
            check.stderr.contains(&format!(
                "{}: no map shard of round 1: the round's map phase did not finish",
                order[2]
            )),
            "{check:?}"
        );
        assert!(!check.has_line("result "), "{check:?}");
    }
    let results = t.path("store").join("results");
    assert!(!results.exists() || std::fs::read_dir(&results).unwrap().next().is_none());
}

// §10.2 (WP-157): every selected observer has a map shard, one of them
// not `ok`: `reduce` accepts the round as before, with that observer
// unparsed.
#[test]
fn s10_2_reduce_of_a_round_with_a_shard_not_ok() {
    let t = Tree::three_files("reduce-not-ok");
    t.write("src/c.cpp", "int broken( {\n");
    t.db(&["src/a.cpp", "src/b.cpp", "src/c.cpp"]);
    let plan = t.plan("store");
    let map = t.slip(&[
        "map", "--plan", &plan, "--from", "f", "--to", "renamed", "--owned", "src", "--store",
        "store",
    ]);
    assert_eq!(map.code(), Some(1), "{map:?}");
    let reduce = t.slip(&["reduce", "--plan", &plan, "--round", "1", "--store", "store"]);
    assert_eq!(reduce.code(), Some(0), "{reduce:?}");
    assert_eq!(reduce.value("state"), Some("accepted"), "{reduce:?}");
    let set = edit_set(&t, "store", &reduce.id("edits"));
    assert_eq!(set.unparsed.len(), 1);
    assert_eq!(set.edits.len(), 3);
}

// §10.2 (WP-157): a map shard of the round overwritten with other bytes
// is not its shard: `reduce` refuses the round naming that observer's
// main file, and writes no edit set.
#[test]
fn s10_2_reduce_refuses_an_unusable_map_shard() {
    let t = Tree::three_files("reduce-unusable");
    let plan = t.plan("store");
    let map = t.slip(&[
        "map", "--plan", &plan, "--from", "f", "--to", "renamed", "--owned", "src", "--store",
        "store",
    ]);
    assert_eq!(map.code(), Some(0), "{map:?}");
    let bytes = std::fs::read(t.path("store").join("plans").join(format!("{plan}.plan"))).unwrap();
    // The first: the shard `reduce` would read the request from, so the
    // check is seen to come before anything else reads the round.
    let first = &Plan::from_bytes(&bytes).unwrap().observers[0];
    let main = String::from_utf8_lossy(&first.main.path).into_owned();
    let plan_id = Digest::from_hex(&plan).unwrap();
    let path = t.store("store").shard_path(&plan_id, Phase::Map, 1, &first.id);
    // Replaced, not written through: no other name of the file sees it.
    std::fs::remove_file(&path).unwrap();
    std::fs::write(&path, b"not a shard").unwrap();
    let reduce = t.slip(&["reduce", "--plan", &plan, "--round", "1", "--store", "store"]);
    assert_eq!(reduce.code(), Some(3), "{reduce:?}");
    assert!(
        reduce.stderr.contains(&format!(
            "{main}: an unusable map shard of round 1: the round's map phase did not finish"
        )),
        "{reduce:?}"
    );
    assert_eq!(edit_sets(&t), 0);
}

// §10.2 (WP-157): the check reads the observers the round's map phase
// selected, not the plan's. `map --only b.cpp` selects the plan's second
// observer: `reduce` accepts the round, though the first has no shard.
#[test]
fn s10_2_reduce_of_a_partial_round_reads_the_selected() {
    let t = Tree::three_files("reduce-partial");
    let plan = t.plan("store");
    let map = t.slip(&[
        "map", "--plan", &plan, "--from", "f", "--to", "renamed", "--owned", "src", "--store",
        "store", "--only", "b.cpp",
    ]);
    assert_eq!(map.code(), Some(0), "{map:?}");
    let bytes = std::fs::read(t.path("store").join("plans").join(format!("{plan}.plan"))).unwrap();
    let order: Vec<String> = Plan::from_bytes(&bytes)
        .unwrap()
        .observers
        .iter()
        .map(|o| String::from_utf8_lossy(&o.main.path).into_owned())
        .collect();
    assert!(order[0].ends_with("a.cpp") && order[1].ends_with("b.cpp"), "{order:?}");
    let reduce = t.slip(&["reduce", "--plan", &plan, "--round", "1", "--store", "store"]);
    assert_eq!(reduce.code(), Some(0), "{reduce:?}");
    assert_eq!(reduce.value("state"), Some("accepted"), "{reduce:?}");
    assert_eq!(edit_sets(&t), 1);
}

// ----- a given --declared (WP-076 fix round, should-fix 3; A5) --------------

// §10.2, WP-076 decision 7: the edit set derives the keys it exempts
// (an in-force `changed` entry: added by hand, the native rename makes
// none). Derived, the result exempts exactly them. Given, a declaration
// may exempt those (and give `kept` the set cannot say), but not a key
// the set does not derive, in any of its lists: an input error naming it.
#[test]
fn s10_2_a_given_declaration_exempts_only_what_the_set_derives() {
    use slip_model::edits::Place;
    use slip_model::editset::Exempt;
    let t = Tree::three_files("declared-given");
    let plan = t.plan("store");
    let out = rename(&t, &plan, &[]);
    let mut set = edit_set(&t, "store", &out.id("edits"));
    let e = &set.edits[0];
    let key = Digest::from_hex(&"ab".repeat(32)).unwrap();
    let other = Digest::from_hex(&"cd".repeat(32)).unwrap();
    set.changed.push(Exempt {
        key,
        place: Place { file: e.file, lo: e.lo, hi: e.hi },
        reason: b"by hand".to_vec(),
        in_force: true,
        observers: 1,
        first: vec![e.first[0]],
    });
    // Derived: the result exempts the set's key.
    let out = check_with(&t, &plan, &set, "x.edts");
    assert!(out.code().is_some(), "{out:?}");
    assert!(out.has_line("declared "), "{out:?}");
    let id = out.id("result");
    let bytes = std::fs::read(t.path("store").join("results").join(format!("{id}.rslt"))).unwrap();
    assert_eq!(TransformResult::from_bytes(&bytes).unwrap().exempt, [key]);
    let derived = t.path("store").join("declared").join(format!("{}.decl", out.id("declared")));
    let derived = Declared::from_bytes(&std::fs::read(derived).unwrap()).unwrap();
    let with = |name: &str, d: &Declared| {
        std::fs::write(t.path(name), d.to_bytes()).unwrap();
        t.slip(&[
            "check",
            "--plan",
            &plan,
            "--round",
            "1",
            "--edits",
            "x.edts",
            "--declared",
            name,
            "--store",
            "store",
        ])
    };
    let rules = derived.rules().rules().to_vec();
    let files = derived.files().cloned();
    let decl = |c: Vec<Digest>, cr: Vec<Digest>, r: Vec<Digest>| {
        Declared::new(rules.clone(), c, cr, r).unwrap().with_files(files.clone())
    };
    // Given: the set's own key, and a `kept` the set cannot say.
    let same = decl(vec![key], vec![], vec![]).with_kept(vec![other]).unwrap();
    let out = with("same.decl", &same);
    assert!(out.has_line("result "), "{out:?}");
    // Narrower: nothing exempt.
    let out = with("none.decl", &Declared::none());
    assert!(out.has_line("result "), "{out:?}");
    // Wider, in each list.
    for (name, d) in [
        ("changed", decl(vec![key, other], vec![], vec![])),
        ("created", decl(vec![], vec![other], vec![])),
        ("removed", decl(vec![], vec![], vec![other])),
    ] {
        let out = with(&format!("{name}.decl"), &d);
        assert_eq!(out.code(), Some(3), "{name}: {out:?}");
        assert!(
            out.stderr.contains(&format!(
                "--declared: `{name}` exempts key {other}, which the edit set does not derive"
            )),
            "{name}: {out:?}"
        );
        assert!(!out.has_line("result "), "{name}: {out:?}");
    }
}

// §8.4 (WP-076 fix round, should-fix 5): a file is scanned once, by
// identity. `src/hl.h` is a hard link to `src/h.h`, which the set edits:
// the walk finds both paths, and the file's spellings of `f` are the
// edits' (before, `hl.h`'s were `not_compiled` residue).
#[test]
fn s8_4_a_file_is_scanned_once_by_identity() {
    let t = Tree::three_files("residue-identity");
    // Identity; `rename` without `--write` replaces neither name (spec/harness.md §11.4).
    std::fs::hard_link(t.path("src/h.h"), t.path("src/hl.h")).unwrap();
    let plan = t.plan("store");
    let out = rename(&t, &plan, &[]);
    assert!(
        out.has_line("residue: dependent 0, not_compiled 0, not_planned 0; mentions 0"),
        "{out:?}"
    );
    assert_eq!(out.stdout.lines().last(), Some("state verified"), "{out:?}");
}

// ----- what the scan reads (§8.4; WP-076 fix round, should-fix 2) ----------

// §8.4: the main files of unparsed observers are scanned, as files no
// observer read, only with `--allow-unparsed`, and only the owned ones.
// `src/c.inl` (owned, an extension the walk does not take) and
// `lib/d.cpp` (not owned) do not parse; both spell `f`.
#[test]
fn s8_4_unparsed_mains_are_scanned_with_allow_unparsed() {
    let t = Tree::three_files("unparsed-mains");
    t.write("src/c.inl", "int f(int);\nint broken( {\n");
    t.write("lib/d.cpp", "int f(int);\nint broken( {\n");
    let dir = t.root.to_str().unwrap();
    let unit = |file: &str, extra: &str| {
        format!(
            "{{\"directory\": \"{dir}\", \"file\": \"{file}\", \"command\": \"clang++ {extra}-std=c++17 -c {file}\"}}"
        )
    };
    let db = [
        unit("src/a.cpp", ""),
        unit("src/b.cpp", ""),
        unit("src/c.inl", "-x c++ "),
        unit("lib/d.cpp", ""),
    ];
    t.write("cdb.json", &format!("[\n{}\n]\n", db.join(",\n")));
    let plan = t.plan("store");
    let out = rename(&t, &plan, &[]);
    assert_eq!(out.value("map:"), Some("selected 4, ok 2, not ok 2, failed runs 0"), "{out:?}");
    assert_eq!(
        out.value("residue:"),
        Some("dependent 0, not_compiled 0, not_planned 0; mentions 0"),
        "without --allow-unparsed no main is scanned: {out:?}"
    );
    let out = rename(&t, &plan, &["--allow-unparsed", "--relative-to", "."]);
    let residue: Vec<&str> = out.stdout.lines().filter(|l| l.starts_with("residue ")).collect();
    assert_eq!(residue.len(), 1, "{out:?}");
    assert!(residue[0].ends_with(": src/c.inl:1:5"), "{out:?}");
}

// §8.4: an included file that is not owned is not scanned. `b.cpp`
// includes `lib/l.h`, which spells `f` in a comment.
#[test]
fn s8_4_a_file_read_but_not_owned_is_not_scanned() {
    let t = Tree::three_files("read-not-owned");
    t.write("lib/l.h", "#pragma once\n// calls f\n");
    t.write("src/b.cpp", "#include \"h.h\"\n#include \"../lib/l.h\"\nint g() { return f(2); }\n");
    let plan = t.plan("store");
    let out = rename(&t, &plan, &[]);
    assert_eq!(
        out.value("residue:"),
        Some("dependent 0, not_compiled 0, not_planned 0; mentions 0"),
        "{out:?}"
    );
    assert_eq!(out.stdout.lines().last(), Some("state verified"), "{out:?}");
}

// §8.4: the walk follows no symbolic link (`src/link` leads to `lib`,
// whose `x.h` declares `f`), and takes no file of a `--readonly` entry
// (`src/ro/r.h` declares `f`).
#[test]
fn s8_4_the_walk_follows_no_link_and_skips_readonly() {
    let t = Tree::three_files("walk-scope");
    t.write("lib/x.h", "int f(int);\n");
    std::os::unix::fs::symlink(t.path("lib"), t.path("src/link")).unwrap();
    std::os::unix::fs::symlink(t.path("lib/x.h"), t.path("src/x.h")).unwrap();
    t.write("src/ro/r.h", "int f(int);\n");
    let plan = t.plan("store");
    let out = rename(&t, &plan, &["--readonly", "src/ro"]);
    assert_eq!(
        out.value("residue:"),
        Some("dependent 0, not_compiled 0, not_planned 0; mentions 0"),
        "{out:?}"
    );
    assert_eq!(out.stdout.lines().last(), Some("state verified"), "{out:?}");
    // Without `--readonly`, `r.h` is an owned file: the scan finds it.
    let out = rename(&t, &plan, &[]);
    assert_eq!(
        out.value("residue:"),
        Some("dependent 0, not_compiled 0, not_planned 1; mentions 0"),
        "{out:?}"
    );
}

// ----- the round's selection (§8.5, decision 9; should-fix 2) --------------

// §8.5: the stages over a round mapped with `--only`: `check`'s result is
// partial, and `apply` writes it only with `--allow-partial`. Decision 9:
// `check` with a selection other than the round's is refused.
#[test]
fn s8_5_check_of_a_partial_round() {
    let t = Tree::three_files("partial-stages");
    t.write("src/c.cpp", "int c() { return 3; }\n");
    t.db(&["src/a.cpp", "src/b.cpp", "src/c.cpp"]);
    let plan = t.plan("store");
    let only = ["--only", "a.cpp", "--only", "b.cpp"];
    let map = t.slip(
        &[
            &["map", "--plan", &plan, "--from", "f", "--to", "renamed", "--owned", "src"][..],
            &["--store", "store"],
            &only,
        ]
        .concat(),
    );
    assert_eq!(map.code(), Some(0), "{map:?}");
    let reduce = t.slip(&["reduce", "--plan", &plan, "--round", "1", "--store", "store"]);
    let edits = reduce.id("edits");
    let check = |extra: &[&str]| {
        t.slip(
            &[
                &["check", "--plan", &plan, "--round", "1", "--edits", &edits][..],
                &["--store", "store"],
                extra,
            ]
            .concat(),
        )
    };
    let other = check(&["--only", "a.cpp"]);
    assert_eq!(other.code(), Some(3), "{other:?}");
    assert!(!other.has_line("result "), "{other:?}");
    let out = check(&only);
    assert_eq!(out.stdout.lines().last(), Some("state verified"), "{out:?}");
    let id = out.id("result");
    let bytes = std::fs::read(t.path("store").join("results").join(format!("{id}.rslt"))).unwrap();
    assert!(TransformResult::from_bytes(&bytes).unwrap().partial);
    let before = t.snapshot();
    let apply = t.slip(&["apply", "--result", &id, "--write", "--store", "store"]);
    assert_eq!(apply.code(), Some(1), "{apply:?}");
    assert_eq!(t.snapshot(), before);
    let apply =
        t.slip(&["apply", "--result", &id, "--write", "--allow-partial", "--store", "store"]);
    assert_eq!(apply.code(), Some(0), "{apply:?}");
    assert_eq!(t.read("src/h.h"), POST_H);
}

// §8.4: the scan reads a file of the edit set only as the set names it: a
// file changed since the map phase is an error naming it. (Through the
// command, apply refuses such a set first, `stale file`; the scan's own
// check is tested on the function.)
#[test]
fn s8_4_the_scan_refuses_a_file_changed_since_the_map_phase() {
    use slip_cli::transform::residue::{extensions, residue};
    let t = Tree::three_files("residue-stale");
    let plan = t.plan("store");
    let out = rename(&t, &plan, &[]);
    let set = edit_set(&t, "store", &out.id("edits"));
    let trfm = t.path("store").join("transforms").join(format!("{}.trfm", out.id("transform")));
    let transform = slip_model::Transform::from_bytes(&std::fs::read(trfm).unwrap()).unwrap();
    let ext = extensions(None);
    assert!(residue(&transform, &set, &[], &[], &ext, 1).is_ok());
    t.write("src/h.h", "#pragma once\nint f(int); \n");
    let e = residue(&transform, &set, &[], &[], &ext, 1).unwrap_err().to_string();
    assert!(
        e.ends_with(
            "h.h: the file is not the one the edit set names (it changed after the map phase)"
        ),
        "{e}"
    );
}

// ----- one file under two spellings (WP-076 fix round, B3) ------------------

/// Whether the tree's volume makes `src/H.h` the file `src/h.h` (case-
/// insensitive), through `common::folding_volume` (which panics on the
/// gate's host when the folding root does not fold, and says when a test
/// skips); the tree must be under the folding root.
fn case_insensitive(t: &Tree, test: &str) -> bool {
    let folds = common::folding_volume(test).is_some();
    assert!(!folds || t.path("src/H.h").exists(), "{test}: the tree does not fold");
    folds
}

// §8.2, B3: the reviewer's probe. `b.cpp` reaches the overlay's `h.h` as
// `H.h`, and its post-image changes (`k()` returns 2), in a run without
// rules: not `unchanged`, the result not `verified`, and `apply --write`
// writes nothing.
#[test]
fn s8_2_case_spelling_is_not_unchanged() {
    let t = Tree::new_in(&common::folding_root(), "case-spelling");
    let h = "#pragma once\nint f(int);\n#ifdef B_SIDE\ninline int k() { return 1; }\n#endif\n";
    t.write("src/h.h", h);
    if !case_insensitive(&t, "s8_2_case_spelling_is_not_unchanged") {
        return;
    }
    t.write("src/a.cpp", "#include \"h.h\"\nint f(int x) { return x + 1; }\n");
    t.write("src/b.cpp", "#define B_SIDE\n#include \"H.h\"\nint g() { return k(); }\n");
    t.db(&["src/a.cpp", "src/b.cpp"]);
    let plan = t.plan("store");
    let out = rename(&t, &plan, &[]);
    let mut set = edit_set(&t, "store", &out.id("edits"));
    let hf = file_of(&set, &t, "src/h.h");
    for e in &mut set.edits {
        e.pieces = vec![Piece::Literal(b"f".to_vec())];
    }
    let mut extra = set.edits.iter().find(|e| e.file == hf).unwrap().clone();
    let at = h.find("return 1").unwrap() as u64 + 7;
    extra.lo = at;
    extra.hi = at + 1;
    extra.pieces = vec![Piece::Literal(b"2".to_vec())];
    set.edits.push(extra);
    set.edits.sort_by_key(|e| (e.file, e.lo));
    set.validate().unwrap();
    std::fs::write(t.path("none.decl"), Declared::none().to_bytes()).unwrap();
    std::fs::write(t.path("k.edts"), set.to_bytes()).unwrap();
    let before = t.snapshot();
    let out = t.slip(&[
        "check",
        "--plan",
        &plan,
        "--round",
        "1",
        "--edits",
        "k.edts",
        "--declared",
        "none.decl",
        "--store",
        "store",
    ]);
    let id = out.id("result");
    let bytes = std::fs::read(t.path("store").join("results").join(format!("{id}.rslt"))).unwrap();
    let r = TransformResult::from_bytes(&bytes).unwrap();
    assert_eq!(r.tally.unchanged, 0, "{out:?}");
    assert!(!matches!(r.state, State::Verified | State::VerifiedWithResidue), "{out:?}");
    let apply = t.slip(&["apply", "--result", &id, "--write", "--store", "store"]);
    assert_ne!(apply.code(), Some(0), "{apply:?}");
    assert_eq!(t.snapshot(), before);
}

// §8.1, B3 as WP-147 leaves it: a plain rename where `b.cpp` spells the
// header `H.h`. A file is read under the spelling its directory gives it
// (`spec/exec.md` §8.2), so the edit set names the one file once, as `h.h`,
// both observers verify, and `--patch` and `--write` do what they say;
// `b.cpp`'s own directive keeps its spelling. (Two names that are not
// spellings of one directory entry, a hard link: the tests above and
// WP-151.)
#[test]
fn s8_1_one_file_under_two_spellings_in_the_set() {
    let t = Tree::new_in(&common::folding_root(), "case-two-spellings");
    t.write("src/h.h", "#pragma once\nint f(int);\n");
    if !case_insensitive(&t, "s8_1_one_file_under_two_spellings_in_the_set") {
        return;
    }
    t.write("src/a.cpp", "#include \"h.h\"\nint f(int x) { return x + 1; }\n");
    t.write("src/b.cpp", "#include \"H.h\"\nint g() { return f(2); }\n");
    t.db(&["src/a.cpp", "src/b.cpp"]);
    let plan = t.plan("store");
    let before = t.snapshot();
    let path = |rel: &str| t.path(rel).to_str().unwrap().as_bytes().to_vec();
    let verified = |asked: &str, out: &Out| {
        let set = edit_set(&t, "store", &out.id("edits"));
        let files = set.files.iter().map(|f| f.path.clone()).collect::<Vec<_>>();
        assert_eq!(files.len(), 3, "{asked}: {out:?}");
        assert!(files.contains(&path("src/h.h")), "{asked}: {out:?}");
        assert!(!files.contains(&path("src/H.h")), "{asked}: {out:?}");
        assert_eq!(out.code(), Some(0), "{asked}: {out:?}");
        assert!(!out.stderr.contains("names one file twice"), "{asked}: {out:?}");
        assert_eq!(
            out.value("verify:"),
            Some(
                "verified 2, unchanged 0, mismatch 0, not_parsed 0, unverifiable 0, never_parsed 0, failed 0"
            ),
            "{asked}: {out:?}"
        );
        assert!(out.stdout.lines().any(|l| l == "state verified"), "{asked}: {out:?}");
    };
    // `--patch` first: it leaves the tree as it was.
    let out = rename(&t, &plan, &["--patch", "p.diff"]);
    verified("--patch", &out);
    assert_eq!(t.snapshot(), before, "{out:?}");
    let diff = std::fs::read_to_string(t.path("p.diff")).unwrap();
    assert!(diff.contains("/h.h") && !diff.contains("/H.h"), "{diff}");
    let out = rename(&t, &plan, &["--write"]);
    verified("--write", &out);
    let after = t.snapshot();
    assert_eq!(after.len(), 3, "{after:?}");
    assert_eq!(after["src/a.cpp"], POST_A);
    assert_eq!(after["src/h.h"], POST_H);
    assert_eq!(after["src/b.cpp"], "#include \"H.h\"\nint g() { return renamed(2); }\n");
}

/// The template of WP-139's `wp139_dependent_call_found_by_adl`
/// (`crates/slip_refactor/tests/rename.rs`): `h(t)` in `call` is
/// dependent, found by argument-dependent lookup at the instantiation
/// `call<m::A>`, which binds it to the subject `m::h`.
fn dependent_tree(name: &str) -> Tree {
    let t = Tree::new(name);
    t.write(
        "src/main.cpp",
        "namespace m { struct A {}; int h(A) { return 1; } }\ntemplate <class T> int call(T t) { \
         return h(t); }\nint u() { return call(m::A()); }\n",
    );
    t.db(&["src/main.cpp"]);
    t
}

/// The value of the last `state` line (the result's; the set's comes first).
fn last_state(out: &Out) -> Option<&str> {
    out.stdout.lines().filter_map(|l| l.strip_prefix("state ")).next_back()
}

fn rename_dependent(t: &Tree, extra: &[&str]) -> Out {
    let plan = t.plan("store");
    let mut args = vec![
        "rename",
        "--plan",
        &plan,
        "--from",
        "m::h",
        "--to",
        "k",
        "--owned",
        "src",
        "--store",
        "store",
        "--relative-to",
        ".",
    ];
    args.extend_from_slice(extra);
    t.slip(&args)
}

// `spec/edits.md` §10.1, §10.5, §9.4 (WP-139 item 4, `docs/wp/WP-076.md`
// second fix round): `rename --dependent offer` edits the dependent name
// bound by every instantiation to the subject. The report has the
// `instances:` heading, which says the run cannot verify the name, and
// one `instance edited` line; the holder differs until dependent names are
// compared mapped (WP-154), so the state is `not_verified`, status 1.
#[test]
fn s10_1_rename_dependent_offer_edits_an_instance() {
    let t = dependent_tree("dependent-offer");
    let before = t.snapshot();
    let out = rename_dependent(&t, &["--dependent", "offer"]);
    assert_eq!(out.code(), Some(1), "{out:?}");
    assert_eq!(last_state(&out), Some("not_verified"), "{out:?}");
    assert!(out.has_line("edit 2 in 1 file(s);"), "{out:?}");
    // The holder `call` differs (WP-154 turns it around).
    assert!(out.has_line("verify: verified 0, unchanged 0, mismatch 1,"), "{out:?}");
    assert!(out.has_line("finding changed template <class> int call("), "{out:?}");
    assert!(out.has_line("kept 0; unresolved 0"), "{out:?}");
    assert!(
        out.has_line(
            "instances: 1 edited, 0 refused; a specialisation made only outside the run is not \
             checked; the run cannot verify a name edited here until dependent names are \
             compared mapped"
        ),
        "{out:?}"
    );
    let instances: Vec<&str> = out.stdout.lines().filter(|l| l.starts_with("instance ")).collect();
    assert_eq!(instances.len(), 1, "{out:?}");
    assert!(
        instances[0].starts_with(
            "instance edited src/main.cpp:2:43: call<m::A>(m::A): m::h(m::A) (1 observer(s)"
        ),
        "{out:?}"
    );
    // §10.5: the instance lines come right after the finding (no exempt line here).
    let lines: Vec<&str> = out.stdout.lines().collect();
    let heading = lines.iter().position(|l| l.starts_with("instances:")).unwrap();
    assert!(lines[heading - 1].starts_with("finding changed "), "{out:?}");
    assert!(lines[heading + 1].starts_with("instance edited "), "{out:?}");
    assert!(out.has_line("residue: dependent 0,"), "{out:?}");
    assert_eq!(t.snapshot(), before, "nothing written without `--write`");
}

// The same with the default (`leave`, §3): the dependent name is not
// edited (`unresolved 1`, residue `dependent`), no `instances:` heading
// and no `instance` line; the instantiation's `h(t)` finds no `h` in the
// post-image, so the run is not verified either.
#[test]
fn s10_1_rename_dependent_default_leaves_the_name() {
    let t = dependent_tree("dependent-leave");
    let out = rename_dependent(&t, &[]);
    assert_eq!(out.code(), Some(1), "{out:?}");
    assert!(out.has_line("edit 1 in 1 file(s);"), "{out:?}");
    assert!(out.has_line("kept 0; unresolved 1"), "{out:?}");
    assert!(!out.has_line("instances:"), "{out:?}");
    assert!(!out.has_line("instance "), "{out:?}");
    assert!(out.has_line("residue: dependent 1,"), "{out:?}");
    assert!(out.has_line("residue dependent: src/main.cpp:2:43"), "{out:?}");
    assert!(out.has_line("verify: verified 0, unchanged 0, mismatch 0, not_parsed 1,"), "{out:?}");
    assert_eq!(last_state(&out), Some("not_verified"), "{out:?}");
    // `--dependent leave` is the default spelled out: the same request.
    let given = rename_dependent(&t, &["--dependent", "leave"]);
    assert_eq!(given.id("transform"), out.id("transform"), "{given:?}");
}

// §2.1 (WP-164): `slip rename --relative-to` resolves through
// `slip_spelling::real_path`, so on a folding volume a directory given in
// another case than on disk still strips the prefix from detail lines.
#[test]
fn s2_1_rename_relative_to_in_another_case() {
    let t = Tree::new_in(&common::folding_root(), "rename-rel-case");
    t.write(
        "src/main.cpp",
        "namespace m { struct A {}; int h(A) { return 1; } }\ntemplate <class T> int call(T t) { \
         return h(t); }\nint u() { return call(m::A()); }\n",
    );
    if common::folding_volume("s2_1_rename_relative_to_in_another_case").is_none() {
        return;
    }
    assert!(t.path("SRC/main.cpp").exists(), "the tree does not fold");
    t.db(&["src/main.cpp"]);
    let plan = t.plan("store");
    let out = t.slip(&[
        "rename",
        "--plan",
        &plan,
        "--from",
        "m::h",
        "--to",
        "k",
        "--owned",
        "src",
        "--store",
        "store",
        "--relative-to",
        "SRC",
    ]);
    assert_eq!(out.code(), Some(1), "{out:?}");
    assert!(out.has_line("residue dependent: main.cpp:2:43"), "{out:?}");
}

// §2.1 (WP-164): `slip rename --relative-to` through a symbolic link gives
// the target's real path, stripping it from detail lines and patches alike.
#[test]
fn s2_1_rename_relative_to_through_symlink() {
    let t = dependent_tree("rename-rel-symlink");
    std::os::unix::fs::symlink(t.path("src"), t.path("link")).unwrap();
    let plan = t.plan("store");
    let out = t.slip(&[
        "rename",
        "--plan",
        &plan,
        "--from",
        "m::h",
        "--to",
        "k",
        "--owned",
        "src",
        "--store",
        "store",
        "--relative-to",
        "link",
    ]);
    assert_eq!(out.code(), Some(1), "{out:?}");
    assert!(out.has_line("residue dependent: main.cpp:2:43"), "{out:?}");

    // And a verified rename with `--patch` through the same link.
    let p = Tree::new("rename-rel-symlink-patch");
    p.write("src/a.cpp", "int f() { return 1; }\nint bar() { return f(); }\n");
    p.db(&["src/a.cpp"]);
    std::os::unix::fs::symlink(p.path("src"), p.path("link")).unwrap();
    let plan = p.plan("store");
    let out = rename(&p, &plan, &["--patch", "out.patch", "--relative-to", "link"]);
    assert_eq!(out.code(), Some(0), "{out:?}");
    let patch = std::fs::read_to_string(p.path("out.patch")).unwrap();
    assert!(patch.starts_with("--- a.cpp\n+++ a.cpp\n"), "{patch}");
}
