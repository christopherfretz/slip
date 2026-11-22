//! `spec/model.md` §8 and `spec/landed/model/WP-010.md`: `parse_observer`,
//! one observer through Kiln to its shard.
//!
//! `parse_observer` changes the process's current directory, so every test
//! here holds [`LOCK`] and uses absolute paths only.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use slip_kiln::{ParseOptions, parse_observer};
use slip_model::hash::content_digest;
use slip_model::{Command, Digest, FileRef, Language, Observer, Origin, Phase, Shard, Status};

static LOCK: Mutex<()> = Mutex::new(());

/// A fixture directory, removed on drop, and the lock held while it lives.
struct Fixture {
    root: PathBuf,
    _lock: MutexGuard<'static, ()>,
}

impl Fixture {
    fn new(name: &str) -> Fixture {
        let lock = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
        let root =
            std::env::temp_dir().join(format!("slip-kiln-parse-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("scratch")).unwrap();
        // The fixture's own real path: the system temp dir may be a link.
        let root = std::fs::canonicalize(&root).unwrap();
        Fixture { root, _lock: lock }
    }

    fn write(&self, rel: &str, text: &str) -> PathBuf {
        let p = self.root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, text).unwrap();
        p
    }

    fn options(&self) -> ParseOptions {
        ParseOptions { scratch: self.root.join("scratch") }
    }

    /// Parses `argv` in `dir` (relative to the root).
    fn parse(&self, dir: &str, argv: &[&str]) -> Shard {
        parse_observer(&observer(&self.root.join(dir), argv), Phase::Map, 3, &self.options())
            .expect("parse_observer")
    }

    fn scratch_is_empty(&self) -> bool {
        std::fs::read_dir(self.root.join("scratch")).unwrap().next().is_none()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn observer(dir: &Path, argv: &[&str]) -> Observer {
    Observer {
        id: Digest([7; 32]),
        command: Command {
            directory: dir.to_str().unwrap().as_bytes().to_vec(),
            argv: argv.iter().map(|a| a.as_bytes().to_vec()).collect(),
            env: Vec::new(),
        },
        language: Language::Cxx,
        main: FileRef { path: Vec::new(), size: 0, content: Digest([0; 32]) },
        cost_hint: 1,
        origins: vec![Origin { configuration: 0, entry: 0 }],
    }
}

fn file_ref(path: &Path) -> FileRef {
    let bytes = std::fs::read(path).unwrap();
    FileRef {
        path: path.to_str().unwrap().as_bytes().to_vec(),
        size: bytes.len() as u64,
        content: content_digest(&bytes),
    }
}

fn counter(shard: &Shard, name: &str) -> Option<u64> {
    shard.counters.iter().find(|c| c.name == name).map(|c| c.value)
}

// §8: a clean translation unit is `ok`, zero errors, header as given.
#[test]
fn s8_clean_unit_is_ok() {
    let f = Fixture::new("clean");
    let main = f.write("main.cpp", "int add(int a, int b) { return a + b; }\n");
    let s = f.parse(".", &["clang++", "-std=c++17", "main.cpp"]);
    assert_eq!(s.observer, Digest([7; 32]));
    assert_eq!(s.phase, Phase::Map);
    assert_eq!(s.round, 3);
    assert_eq!(s.outcome.status, Status::Ok, "{:?}", s.outcome);
    assert_eq!((s.outcome.errors, s.outcome.warnings), (0, 0));
    assert_eq!(s.outcome.message, "");
    assert_eq!(s.inputs, Some(vec![file_ref(&main)]));
    let names: Vec<&str> = s.counters.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["inputs.bytes", "inputs.files"], "sorted by name; no pp.tokens");
    assert_eq!(counter(&s, "inputs.files"), Some(1));
    assert_eq!(counter(&s, "inputs.bytes"), Some(std::fs::metadata(&main).unwrap().len()));
    assert!(f.scratch_is_empty(), "the dependency file is removed");
}

// §8: errors are `diagnosed`, counted, the first one's text kept.
#[test]
fn s8_errors_are_diagnosed() {
    let f = Fixture::new("error");
    let main = f.write(
        "main.cpp",
        "int f() { return undeclared_one; }\nint g() { return undeclared_two; }\n",
    );
    let s = f.parse(".", &["c++", "main.cpp"]);
    assert_eq!(s.outcome.status, Status::Diagnosed, "{:?}", s.outcome);
    assert_eq!(s.outcome.errors, 2);
    assert!(s.outcome.message.starts_with("main.cpp:1:"), "{}", s.outcome.message);
    assert!(s.outcome.message.contains("undeclared_one"), "{}", s.outcome.message);
    // The preprocessor reached the end: the inputs are known.
    assert_eq!(s.inputs, Some(vec![file_ref(&main)]));
    assert!(f.scratch_is_empty());
}

// §8: warnings are counted, and alone leave the status `ok`.
#[test]
fn s8_warnings_counted() {
    let f = Fixture::new("warnings");
    f.write("main.cpp", "#warning one\n#warning two\nint x;\n");
    let s = f.parse(".", &["c++", "main.cpp"]);
    assert_eq!(s.outcome.status, Status::Ok, "{:?}", s.outcome);
    assert_eq!((s.outcome.errors, s.outcome.warnings), (0, 2));
    // `-Werror` makes them errors.
    let s = f.parse(".", &["c++", "-Werror", "main.cpp"]);
    assert_eq!(s.outcome.status, Status::Diagnosed, "{:?}", s.outcome);
    assert_eq!((s.outcome.errors, s.outcome.warnings), (2, 0));
    assert!(s.outcome.message.contains("one"), "{}", s.outcome.message);
}

// §8 inputs: a relative `-I` is found from the command's directory; every
// header once, by real path, sorted, with size and digest.
#[test]
fn s8_inputs_relative_include_once_each_sorted() {
    let f = Fixture::new("inputs");
    let a = f.write("proj/include/a.h", "#pragma once\nint a();\n");
    let b =
        f.write("proj/include/sub/b.h", "#ifndef B\n#define B\n#include <a.h>\nint b();\n#endif\n");
    let main = f.write(
        "proj/src/main.cpp",
        "#include <sub/b.h>\n#include <a.h>\n#include <sub/b.h>\n#include \"../include/sub/../a.h\"\nint m() { return a() + b(); }\n",
    );
    let s = f.parse("proj", &["g++", "-Iinclude", "-c", "src/main.cpp"]);
    assert_eq!(s.outcome.status, Status::Ok, "{:?}", s.outcome);
    let mut want = vec![file_ref(&a), file_ref(&b), file_ref(&main)];
    want.sort();
    assert_eq!(s.inputs, Some(want));
    assert_eq!(counter(&s, "inputs.files"), Some(3));
}

// §6/§8: a header reached through a symbolic link is listed by its real path.
#[cfg(unix)]
#[test]
fn s8_symlinked_header_by_real_path() {
    let f = Fixture::new("symlink");
    let real = f.write("real/r.h", "int r();\n");
    std::os::unix::fs::symlink(&real, f.root.join("link.h")).unwrap();
    std::os::unix::fs::symlink(f.root.join("real"), f.root.join("linkdir")).unwrap();
    let main = f.write("main.cpp", "#include \"link.h\"\n#include \"linkdir/r.h\"\nint m();\n");
    let s = f.parse(".", &["c++", "main.cpp"]);
    assert_eq!(s.outcome.status, Status::Ok, "{:?}", s.outcome);
    let mut want = vec![file_ref(&real), file_ref(&main)];
    want.sort();
    assert_eq!(s.inputs, Some(want));
}

// §8: deterministic, the same observer and files give the same shard.
#[test]
fn s8_deterministic() {
    let f = Fixture::new("determinism");
    f.write("h.h", "#warning w\nstruct S { int x; };\n");
    f.write("main.cpp", "#include \"h.h\"\nint f(S s) { return s.y; }\n");
    let one = f.parse(".", &["c++", "main.cpp"]);
    let two = f.parse(".", &["c++", "main.cpp"]);
    assert_eq!(one.outcome.status, Status::Diagnosed);
    assert_eq!(one, two);
}

// §8: a C translation unit is parsed as C (`new` is an identifier).
#[test]
fn s8_c_unit() {
    let f = Fixture::new("c");
    let main = f.write("main.c", "int new = 1;\nint use(void) { return new; }\n");
    let s = f.parse(".", &["cc", "-std=c11", "main.c"]);
    assert_eq!(s.outcome.status, Status::Ok, "{:?}", s.outcome);
    assert_eq!(s.inputs, Some(vec![file_ref(&main)]));
    // The same file as C++ does not parse.
    let s = f.parse(".", &["cc", "-x", "c++", "main.c"]);
    assert_eq!(s.outcome.status, Status::Diagnosed);
}

// §8: a command line the driver refuses is `rejected`, with no inputs.
#[test]
fn s8_rejected_command() {
    let f = Fixture::new("rejected");
    f.write("main.cpp", "int x;\n");
    let s = f.parse(".", &["clang++", "-fslip-no-such-flag", "main.cpp"]);
    assert_eq!(s.outcome.status, Status::Rejected, "{:?}", s.outcome);
    assert_eq!(s.outcome.errors, 1);
    assert!(s.outcome.message.contains("-fslip-no-such-flag"), "{}", s.outcome.message);
    assert_eq!(s.inputs, None);
    assert!(s.counters.is_empty());
    assert!(f.scratch_is_empty());
    // A command that overrides the dependency output the parse relies on.
    let s = f.parse(".", &["clang++", "-Wp,-MMD,elsewhere.d", "main.cpp"]);
    assert_eq!(s.outcome.status, Status::Rejected, "{:?}", s.outcome);
    assert!(s.outcome.message.contains("-Wp,-MMD"), "{}", s.outcome.message);
    assert!(!f.root.join("elsewhere.d").exists());
}

// Landed: a missing header is `diagnosed`; Kiln writes no dependency rule,
// so the inputs are unknown.
#[test]
fn landed_missing_header_inputs_unknown() {
    let f = Fixture::new("missing");
    f.write("main.cpp", "#include \"nowhere.h\"\nint x;\n");
    let s = f.parse(".", &["c++", "main.cpp"]);
    assert_eq!(s.outcome.status, Status::Diagnosed, "{:?}", s.outcome);
    assert!(s.outcome.message.contains("nowhere.h"), "{}", s.outcome.message);
    assert_eq!(s.inputs, None);
    assert!(s.counters.is_empty());
    assert!(f.scratch_is_empty());
}

// §8: `Err` only for a failure of the worker itself.
#[test]
fn s8_worker_failures_are_err() {
    let f = Fixture::new("failures");
    f.write("main.cpp", "int x;\n");
    let o = observer(&f.root.join("no-such-dir"), &["c++", "main.cpp"]);
    let e = parse_observer(&o, Phase::Map, 0, &f.options()).unwrap_err();
    assert!(e.contains("cannot enter"), "{e}");
    let o = observer(&f.root, &["c++", "main.cpp"]);
    let bad = ParseOptions { scratch: f.root.join("no-such-scratch") };
    let e = parse_observer(&o, Phase::Map, 0, &bad).unwrap_err();
    assert!(e.contains("scratch"), "{e}");
}

// §8 with the standard library: system headers are inputs too (`-MD`).
// Skipped, saying why, when Kiln finds no C++ standard library.
#[test]
fn s8_standard_library_headers_are_inputs() {
    let f = Fixture::new("cstddef");
    let main = f.write("main.cpp", "#include <cstddef>\nstd::size_t n = sizeof(int);\n");
    let s = f.parse(".", &["c++", "-std=c++17", "main.cpp"]);
    if s.outcome.status != Status::Ok && s.outcome.message.contains("cstddef") {
        eprintln!("skipped: no C++ standard library found: {}", s.outcome.message);
        return;
    }
    assert_eq!(s.outcome.status, Status::Ok, "{:?}", s.outcome);
    let inputs = s.inputs.unwrap();
    assert!(inputs.len() > 2, "{inputs:?}");
    assert!(inputs.contains(&file_ref(&main)));
    assert!(inputs.iter().any(|i| i.path.ends_with(b"/cstddef")), "{inputs:?}");
    assert!(inputs.windows(2).all(|w| w[0].path < w[1].path), "sorted, no duplicates");
    for i in &inputs {
        let p = std::str::from_utf8(&i.path).unwrap();
        assert_eq!(std::fs::canonicalize(p).unwrap(), PathBuf::from(p), "real path");
    }
}
