//! The verify phase of `slip check` and `slip rename` over an overlay,
//! through the executable (`spec/edits.md` §8.2, §8.3; `spec/verify.md`
//! §5.5, §9.3): who gets a verify job, `unchanged`, and the binding of the
//! shards. WP-076, "Tests". The verify job itself over an overlay (a correct
//! rename, an included overlay file, O1 and O2, blobs that fail the job)
//! is `tests/tasks.rs`'s (WP-074); a map shard replaced between a job and
//! the report is `slip_verify`'s `tests/run.rs` (WP-114).

#![cfg(target_os = "linux")]

mod common;

use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use slip_exec::store::{Access, Store};
use slip_model::declared::Declared;
use slip_model::formats::content_id;
use slip_model::transform_run::{StepName, TransformRun};
use slip_model::{Digest, Phase, RunRecord, Shard};

const SLIP: &str = env!("CARGO_BIN_EXE_slip");

struct Tree {
    root: PathBuf,
}

impl Tree {
    /// `f` declared in `h.h`, defined in `a.cpp`, called in `b.cpp`; `c.cpp`
    /// includes nothing of it.
    fn new(name: &str) -> Tree {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let root =
            common::work_root().join(format!("wp076-overlay-{}-{n}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).unwrap();
        let t = Tree { root: std::fs::canonicalize(&root).unwrap() };
        t.write("src/h.h", "#pragma once\nint f(int);\n");
        t.write("src/a.cpp", "#include \"h.h\"\nint f(int x) { return x + 1; }\n");
        t.write("src/b.cpp", "#include \"h.h\"\nint g() { return f(2); }\n");
        t.write("src/c.cpp", "int c() { return 3; }\n");
        let dir = t.root.to_str().unwrap();
        let items: Vec<String> = ["src/a.cpp", "src/b.cpp", "src/c.cpp"]
            .iter()
            .map(|u| {
                format!(
                    "{{\"directory\": \"{dir}\", \"file\": \"{u}\", \"command\": \"clang++ \
                     -std=c++17 -c {u}\"}}"
                )
            })
            .collect();
        t.write("cdb.json", &format!("[\n{}\n]\n", items.join(",\n")));
        t
    }

    fn write(&self, rel: &str, text: &str) {
        std::fs::write(self.root.join(rel), text).unwrap();
    }

    fn slip(&self, args: &[&str]) -> (Option<i32>, String, String) {
        let mut child = Command::new(SLIP)
            .args(args)
            .current_dir(&self.root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let read = |mut r: Box<dyn Read + Send>| {
            std::thread::spawn(move || {
                let mut s = String::new();
                let _ = r.read_to_string(&mut s);
                s
            })
        };
        let out = read(Box::new(child.stdout.take().unwrap()));
        let err = read(Box::new(child.stderr.take().unwrap()));
        let deadline = Instant::now() + Duration::from_mins(3);
        let status = loop {
            if let Some(s) = child.try_wait().unwrap() {
                break s;
            }
            if Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("slip did not finish");
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        (status.code(), out.join().unwrap(), err.join().unwrap())
    }

    fn store(&self) -> Store {
        Store::open(&self.root.join("store"), Access::Shared).unwrap()
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn value<'a>(stdout: &'a str, key: &str) -> &'a str {
    stdout
        .lines()
        .find_map(|l| l.strip_prefix(key)?.strip_prefix(' '))
        .unwrap_or_else(|| panic!("no `{key}`: {stdout}"))
}

fn id(stdout: &str, key: &str) -> Digest {
    Digest::from_hex(value(stdout, key).split(':').next().unwrap()).unwrap()
}

/// The plan, its map round 1 and its reduced edit set.
fn mapped(t: &Tree) -> (String, String) {
    let (code, out, err) = t.slip(&["plan", "--db", "cdb.json", "--store", "store"]);
    assert_eq!(code, Some(0), "{out}{err}");
    let plan = value(&out, "plan").to_owned();
    let (code, out, err) = t.slip(&[
        "map", "--plan", &plan, "--from", "f", "--to", "renamed", "--owned", "src", "--store",
        "store",
    ]);
    assert_eq!(code, Some(0), "{out}{err}");
    let (code, out, err) = t.slip(&["reduce", "--plan", &plan, "--round", "1", "--store", "store"]);
    assert_eq!(code, Some(0), "{out}{err}");
    (plan, value(&out, "edits").split(':').next().unwrap().to_owned())
}

/// The jobs of the verify phase of the last transform run record.
fn verify_jobs(t: &Tree, plan: &str) -> Vec<Digest> {
    let dir = t.root.join("store").join("runs").join(plan);
    let mut runs: Vec<(String, Vec<u8>)> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| (e.file_name().to_str().unwrap().to_owned(), std::fs::read(e.path()).unwrap()))
        .collect();
    runs.sort();
    let (_, last) = runs.iter().rev().find(|(n, _)| n.starts_with("transform.")).unwrap();
    let r = TransformRun::from_bytes(last).unwrap();
    let step = r.steps.iter().find(|s| s.name == StepName::Verify).unwrap();
    let want = step.run.unwrap();
    let (_, bytes) = runs.iter().find(|(_, b)| content_id(b).ok() == Some(want)).unwrap();
    let mut jobs: Vec<Digest> =
        RunRecord::from_bytes(bytes).unwrap().jobs.iter().map(|j| j.observer).collect();
    jobs.sort();
    jobs.dedup();
    jobs
}

// §8.2: with rules, every observer with an `ok` map shard gets a verify job,
// `c.cpp` too, though it reads no file of the overlay; every shard is bound
// to the run (verify.md §9.3).
#[test]
fn s8_2_with_rules_every_ok_observer_gets_a_job() {
    let t = Tree::new("rules");
    let (plan, edits) = mapped(&t);
    let (code, out, err) =
        t.slip(&["check", "--plan", &plan, "--round", "1", "--edits", &edits, "--store", "store"]);
    assert_eq!(code, Some(0), "{out}{err}");
    assert_eq!(
        value(&out, "verify:"),
        "verified 3, unchanged 0, mismatch 0, not_parsed 0, unverifiable 0, never_parsed 0, \
         failed 0"
    );
    assert_eq!(verify_jobs(&t, &plan).len(), 3);
    let (overlay, declared) = (id(&out, "overlay"), id(&out, "declared"));
    let store = t.store();
    let plan_id = Digest::from_hex(&plan).unwrap();
    for o in verify_jobs(&t, &plan) {
        let bytes = store.read_shard(&plan_id, Phase::Verify, 1, &o).unwrap().unwrap();
        let b = Shard::from_bytes(&bytes).unwrap().binding.expect("a binding");
        assert_eq!((b.overlay, b.declared, b.pre), (Some(overlay), declared, 1));
    }
}

// §8.2, verify.md §5.5: without rules, only the observers that read a file
// of the overlay get a job; `c.cpp` is `unchanged`, decided by the report
// from its map shard.
#[test]
fn s8_2_without_rules_an_observer_reading_no_overlay_file_is_unchanged() {
    let t = Tree::new("no-rules");
    let (plan, edits) = mapped(&t);
    std::fs::write(t.root.join("none.decl"), Declared::none().to_bytes()).unwrap();
    let (code, out, err) = t.slip(&[
        "check",
        "--plan",
        &plan,
        "--round",
        "1",
        "--edits",
        &edits,
        "--declared",
        "none.decl",
        "--store",
        "store",
    ]);
    // `f`'s key changes and nothing declares it: a and b mismatch.
    assert_eq!(code, Some(1), "{out}{err}");
    assert_eq!(value(&out, "declared").split(": ").nth(1), Some("rules 0, exempt 0"));
    let verify = value(&out, "verify:");
    assert!(verify.contains(", unchanged 1, "), "{verify}");
    assert!(verify.ends_with(", failed 0"), "{verify}");
    let jobs = verify_jobs(&t, &plan);
    assert_eq!(jobs.len(), 2);
    let plan_id = Digest::from_hex(&plan).unwrap();
    let store = t.store();
    // c.cpp's observer is the one with a map shard and no verify shard.
    let c = t.root.join("src/c.cpp");
    let p = slip_model::Plan::from_bytes(&std::fs::read(store.plan_path(&plan_id)).unwrap());
    let c_id = p
        .unwrap()
        .observers
        .iter()
        .find(|o| o.main.path == c.as_os_str().as_encoded_bytes())
        .unwrap()
        .id;
    assert!(!jobs.contains(&c_id));
    assert!(store.read_shard(&plan_id, Phase::Verify, 1, &c_id).unwrap().is_none());
}
