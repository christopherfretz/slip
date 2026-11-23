//! `spec/model.md` §8 and §8.4: `parse_observer` and `parse_observer_with`,
//! one observer through Kiln's front-end library entry to its shard. Each
//! test names the paragraph of §8.4 it covers ("the run", "outcome",
//! "inputs", "counters"); the parity test compares every fixture with the
//! parse task of WP-010 (`support::oracle`).
//!
//! The parse changes the process's current directory, so every test here
//! holds [`LOCK`] and uses absolute paths only.

mod support;

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use slip_cdb::check::FrontEndCheck;
use slip_kiln::kiln::ast::decl::DeclKind;
use slip_kiln::kiln::source::FileOrigin;
use slip_kiln::{KilnCheck, Unit, parse_observer, parse_observer_with};
use slip_model::hash::content_digest;
use slip_model::{Command, Digest, FileRef, Language, Observer, Origin, Phase, Shard, Status};

static LOCK: Mutex<()> = Mutex::new(());

/// A fixture directory, removed on drop, a directory beside it for the
/// oracle's dependency file, and the lock held while it lives.
struct Fixture {
    root: PathBuf,
    oracle: PathBuf,
    _lock: MutexGuard<'static, ()>,
}

impl Fixture {
    fn new(name: &str) -> Fixture {
        let lock = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
        let base =
            std::env::temp_dir().join(format!("slip-kiln-parse-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("work")).unwrap();
        std::fs::create_dir_all(base.join("oracle")).unwrap();
        // The fixture's own real path: the system temp dir may be a link.
        let base = std::fs::canonicalize(&base).unwrap();
        Fixture { root: base.join("work"), oracle: base.join("oracle"), _lock: lock }
    }

    fn write(&self, rel: &str, text: &str) -> PathBuf {
        self.write_bytes(rel, text.as_bytes())
    }

    fn write_bytes(&self, rel: &str, bytes: &[u8]) -> PathBuf {
        let p = self.root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, bytes).unwrap();
        p
    }

    fn observer(&self, dir: &str, argv: &[&str]) -> Observer {
        observer(&self.root.join(dir), argv)
    }

    /// Parses `argv` in `dir` (relative to the root); checks the shard's
    /// invariants and that `KilnCheck` agrees with the parse.
    fn parse(&self, dir: &str, argv: &[&str]) -> Shard {
        let o = self.observer(dir, argv);
        let shard = parse_observer(&o, Phase::Map, 3).expect("parse_observer");
        // Every shard satisfies the format's invariants (spec/model.md §8).
        shard.validate().expect("a valid shard");
        assert_agrees(&o.command, &shard);
        shard
    }

    /// What the parse task of WP-010 gave the same observer.
    fn old(&self, dir: &str, argv: &[&str]) -> Shard {
        let o = self.observer(dir, argv);
        support::oracle::old_shard(&o, Phase::Map, 3, &self.oracle).expect("the oracle")
    }

    /// The root's entries, recursively, sorted.
    fn entries(&self) -> Vec<PathBuf> {
        fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
            for e in std::fs::read_dir(dir).unwrap() {
                let p = e.unwrap().path();
                out.push(p.clone());
                if p.is_dir() && !p.is_symlink() {
                    walk(&p, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.root, &mut out);
        out.sort();
        out
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.root.parent().unwrap());
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

/// Whether the parse refused the command for a reason of `spec/cdb.md`
/// §6.2 rejection 3: what `run_front_end` refuses without running, or with
/// `drv_front_end_unsupported`.
fn refused_by_entry(shard: &Shard) -> bool {
    shard.outcome.status == Status::Rejected
        && (shard.outcome.message == "the command line is not -fsyntax-only"
            || shard.outcome.message == "the command line has not one source input"
            || shard.outcome.message.contains("is not supported by the front-end library entry"))
}

/// `spec/cdb.md` §6.2: a command `KilnCheck` accepts is never refused by
/// the parse for a reason of rejection 3.
fn assert_agrees(command: &Command, shard: &Shard) {
    if KilnCheck.check(command, b"").is_ok() {
        assert!(!refused_by_entry(shard), "KilnCheck accepts {command:?}: {:?}", shard.outcome);
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

fn refs(paths: &[&Path]) -> Vec<FileRef> {
    let mut v: Vec<FileRef> = paths.iter().map(|p| file_ref(p)).collect();
    v.sort();
    v
}

/// The closure of `parse_observer_with` that returns the unit's errors.
fn errors_of(unit: &Unit<'_>) -> u32 {
    unit.errors()
}

fn counter(shard: &Shard, name: &str) -> Option<u64> {
    shard.counters.iter().find(|c| c.name == name).map(|c| c.value)
}

// §8.4 outcome and counters: a clean translation unit is `ok`, zero
// errors, header as given; `inputs.bytes`, `inputs.files`, `pp.tokens`.
#[test]
fn s8_4_outcome_clean_unit_is_ok() {
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
    assert_eq!(names, ["inputs.bytes", "inputs.files", "pp.tokens"], "sorted by name");
    assert_eq!(counter(&s, "inputs.files"), Some(1));
    assert_eq!(counter(&s, "inputs.bytes"), Some(std::fs::metadata(&main).unwrap().len()));
}

// §8.4 outcome: errors are `diagnosed`, counted, the first one's text kept.
#[test]
fn s8_4_outcome_errors_are_diagnosed() {
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
    // A parse happened: the inputs are known.
    assert_eq!(s.inputs, Some(vec![file_ref(&main)]));
}

// §8.4 outcome: warnings are counted, and alone leave the status `ok`.
#[test]
fn s8_4_outcome_warnings_counted() {
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

// §8.4 inputs: a relative `-I` is found from the command's directory; every
// header once, by real path, sorted, with size and digest.
#[test]
fn s8_4_inputs_relative_include_once_each_sorted() {
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
    assert_eq!(s.inputs, Some(refs(&[&a, &b, &main])));
    assert_eq!(counter(&s, "inputs.files"), Some(3));
}

// §8.4 inputs: a header reached through a symbolic link is listed by its
// real path.
#[cfg(unix)]
#[test]
fn s8_4_inputs_symlinked_header_by_real_path() {
    let f = Fixture::new("symlink");
    let real = f.write("real/r.h", "int r();\n");
    std::os::unix::fs::symlink(&real, f.root.join("link.h")).unwrap();
    std::os::unix::fs::symlink(f.root.join("real"), f.root.join("linkdir")).unwrap();
    let main = f.write("main.cpp", "#include \"link.h\"\n#include \"linkdir/r.h\"\nint m();\n");
    let s = f.parse(".", &["c++", "main.cpp"]);
    assert_eq!(s.outcome.status, Status::Ok, "{:?}", s.outcome);
    assert_eq!(s.inputs, Some(refs(&[&real, &main])));
}

// §8 rule 2: deterministic, the same observer and files give the same shard.
#[test]
fn s8_4_deterministic() {
    let f = Fixture::new("determinism");
    f.write("h.h", "#warning w\nstruct S { int x; };\n");
    f.write("main.cpp", "#include \"h.h\"\nint f(S s) { return s.y; }\n");
    let one = f.parse(".", &["c++", "main.cpp"]);
    let two = f.parse(".", &["c++", "main.cpp"]);
    assert_eq!(one.outcome.status, Status::Diagnosed);
    assert_eq!(one, two);
    assert_eq!(one.to_bytes(), two.to_bytes(), "byte for byte once encoded");
}

// §8.4 the run: a C translation unit is parsed as C (`new` is an
// identifier).
#[test]
fn s8_4_run_c_unit() {
    let f = Fixture::new("c");
    let main = f.write("main.c", "int new = 1;\nint use(void) { return new; }\n");
    let s = f.parse(".", &["cc", "-std=c11", "main.c"]);
    assert_eq!(s.outcome.status, Status::Ok, "{:?}", s.outcome);
    assert_eq!(s.inputs, Some(vec![file_ref(&main)]));
    // The same file as C++ does not parse.
    let s = f.parse(".", &["cc", "-x", "c++", "main.c"]);
    assert_eq!(s.outcome.status, Status::Diagnosed);
}

// §8.4 outcome: a command line the driver refuses is `rejected`, with no
// inputs and no counters; so is one the entry refuses, with its message.
#[test]
fn s8_4_outcome_rejected_command() {
    let f = Fixture::new("rejected");
    f.write("main.cpp", "int x;\n");
    let s = f.parse(".", &["clang++", "-fslip-no-such-flag", "main.cpp"]);
    assert_eq!(s.outcome.status, Status::Rejected, "{:?}", s.outcome);
    assert_eq!(s.outcome.errors, 1);
    assert!(s.outcome.message.contains("-fslip-no-such-flag"), "{}", s.outcome.message);
    assert_eq!(s.inputs, None);
    assert!(s.counters.is_empty());

    // `NotSyntaxOnly` and `NotOneInput`: one error, the error's text.
    for (argv, message) in [
        (&["clang++", "-E", "main.cpp"][..], "the command line is not -fsyntax-only"),
        (&["clang++", "-M", "main.cpp"][..], "the command line is not -fsyntax-only"),
        (&["clang++", "main.cpp", "main.cpp"][..], "the command line has not one source input"),
        (&["clang++", "-x", "c++", "-"][..], "the command line has not one source input"),
    ] {
        let s = f.parse(".", argv);
        assert_eq!(s.outcome.status, Status::Rejected, "{argv:?}");
        assert_eq!((s.outcome.errors, s.outcome.warnings), (1, 0), "{argv:?}");
        assert_eq!(s.outcome.message, message, "{argv:?}");
        assert_eq!(s.inputs, None);
        assert!(s.counters.is_empty());
    }
    // `drv_front_end_unsupported`: a driver error.
    let s = f.parse(".", &["clang++", "-verify", "main.cpp"]);
    assert_eq!(s.outcome.status, Status::Rejected, "{:?}", s.outcome);
    assert!(s.outcome.message.contains("'-verify' is not supported"), "{}", s.outcome.message);
    assert_eq!(s.inputs, None);
}

// §8.4 outcome: a command that is not UTF-8 is `rejected` without running
// the front end.
#[test]
fn s8_4_outcome_not_utf8() {
    let f = Fixture::new("utf8");
    f.write("main.cpp", "int x;\n");
    let mut o = f.observer(".", &["clang++", "main.cpp"]);
    o.command.argv.insert(1, b"-DX=\xff".to_vec());
    let s = parse_observer(&o, Phase::Map, 0).unwrap();
    assert_eq!(s.outcome.status, Status::Rejected);
    assert_eq!((s.outcome.errors, s.outcome.warnings), (1, 0));
    assert_eq!(s.outcome.message, "argument `-DX=\u{fffd}` is not UTF-8");
    assert_eq!(s.inputs, None);
}

// §8.4 inputs (new in WP-025): a missing header is `diagnosed`, and the
// inputs are known, since a parse happened.
#[test]
fn s8_4_inputs_missing_header_inputs_known() {
    let f = Fixture::new("missing");
    let main = f.write("main.cpp", "#include \"nowhere.h\"\nint x;\n");
    let s = f.parse(".", &["c++", "main.cpp"]);
    assert_eq!(s.outcome.status, Status::Diagnosed, "{:?}", s.outcome);
    assert!(s.outcome.message.contains("nowhere.h"), "{}", s.outcome.message);
    assert_eq!(s.inputs, Some(vec![file_ref(&main)]));
    assert_eq!(counter(&s, "inputs.files"), Some(1));
}

// §8.4: `Err` only for a failure of the worker itself.
#[test]
fn s8_4_run_worker_failures_are_err() {
    let f = Fixture::new("failures");
    f.write("main.cpp", "int x;\n");
    let o = observer(&f.root.join("no-such-dir"), &["c++", "main.cpp"]);
    let e = parse_observer(&o, Phase::Map, 0).unwrap_err();
    assert!(e.contains("cannot enter"), "{e}");
}

// §8.4 inputs, with the standard library: system headers are inputs too.
// Skipped, saying why, when Kiln finds no C++ standard library.
#[test]
fn s8_4_inputs_standard_library_headers() {
    let f = Fixture::new("cstddef");
    let main = f.write("main.cpp", "#include <cstddef>\nstd::size_t n = sizeof(int);\n");
    let argv = ["c++", "-std=c++17", "main.cpp"];
    let s = f.parse(".", &argv);
    if s.outcome.status != Status::Ok && s.outcome.message.contains("cstddef") {
        eprintln!("skipped: no C++ standard library found: {}", s.outcome.message);
        return;
    }
    assert_eq!(s.outcome.status, Status::Ok, "{:?}", s.outcome);
    let inputs = s.inputs.clone().unwrap();
    assert!(inputs.len() > 2, "{inputs:?}");
    assert!(inputs.contains(&file_ref(&main)));
    assert!(inputs.iter().any(|i| i.path.ends_with(b"/cstddef")), "{inputs:?}");
    assert!(inputs.windows(2).all(|w| w[0].path < w[1].path), "sorted, no duplicates");
    for i in &inputs {
        let p = std::str::from_utf8(&i.path).unwrap();
        assert_eq!(std::fs::canonicalize(p).unwrap(), PathBuf::from(p), "real path");
    }
    // And the list is the one `-MD` gave (the parity of §8.4 "Inputs").
    assert_eq!(without_tokens(&s), f.old(".", &argv));
}

// §8.4 the run: Kiln's stack guard is installed on the front end's
// thread, so nesting deeper than its stack ends in Kiln's fatal diagnostic
// (`diagnosed`), not a stack overflow that would abort this test process.
// A small `-fkiln-stack-size=` makes it quick.
#[test]
fn s8_4_run_deep_nesting_is_diagnosed_not_a_crash() {
    let f = Fixture::new("deep");
    let depth = 20_000;
    let text = format!("int x = {}1{};\n", "(".repeat(depth), ")".repeat(depth));
    f.write("main.cpp", &text);
    let s = f.parse(".", &["c++", "-std=c++17", "-fkiln-stack-size=4194304", "main.cpp"]);
    assert_eq!(s.outcome.status, Status::Diagnosed, "{:?}", s.outcome);
    assert!(s.outcome.message.contains("stack"), "{:?}", s.outcome);
}

// §8.4 the run: the entry writes nothing, whatever the command asks for:
// the fixture directory holds the same entries after the parse as before.
#[test]
fn s8_4_run_no_file_created() {
    let f = Fixture::new("nofile");
    f.write("main.cpp", "#include \"h.h\"\nint x;\n");
    f.write("h.h", "int h;\n");
    let before = f.entries();
    for argv in [
        &["clang++", "-MD", "-MF", "x.d", "-o", "x.o", "main.cpp"][..],
        &["clang++", "-Wp,-MMD,elsewhere.d", "-MJ", "frag.json", "main.cpp"][..],
    ] {
        let s = f.parse(".", argv);
        assert_eq!(s.outcome.status, Status::Ok, "{argv:?}: {:?}", s.outcome);
        assert_eq!(f.entries(), before, "{argv:?}");
    }
}

/// `shard` without its counter `pp.tokens`, which the oracle cannot
/// count.
fn without_tokens(shard: &Shard) -> Shard {
    let mut s = shard.clone();
    s.counters.retain(|c| c.name != "pp.tokens");
    s
}

/// A difference of the parity test that §8.4 documents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Documented {
    /// None: the shards agree but for `pp.tokens`.
    None,
    /// A `diagnosed` unit whose `#include` was not found: the old flow
    /// had no inputs (Kiln wrote no dependency rule), the new one has.
    InputsOfMissingInclude,
    /// The first error's message carries the ` [option]` suffix that the
    /// text sink prints and the JSON's `message` does not.
    OptionSuffix,
}

/// §8.4 parity: the shard of `parse_observer` equals the oracle's, field
/// for field, except `pp.tokens` and the documented difference `expect`.
fn assert_parity(f: &Fixture, dir: &str, argv: &[&str], expect: Documented) -> Shard {
    let new = f.parse(dir, argv);
    let old = f.old(dir, argv);
    let mut got = without_tokens(&new);
    match expect {
        Documented::None => {}
        Documented::InputsOfMissingInclude => {
            assert_eq!(new.outcome.status, Status::Diagnosed, "{argv:?}");
            assert_eq!(old.inputs, None, "{argv:?}: the old flow had no inputs");
            assert!(got.inputs.is_some(), "{argv:?}: the new flow has");
            got.inputs = None;
            got.counters.clear();
        }
        Documented::OptionSuffix => {
            let suffix = got.outcome.message.strip_prefix(&old.outcome.message);
            assert!(
                suffix.is_some_and(|s| s.starts_with(" [") && s.ends_with(']')),
                "{argv:?}: {:?} against {:?}",
                got.outcome.message,
                old.outcome.message
            );
            got.outcome.message.clone_from(&old.outcome.message);
        }
    }
    assert_eq!(got, old, "{argv:?}: the new shard against the old");
    new
}

// §8.4 parity (WP-025 "Tests"): each fixture of the WP against the oracle.
#[test]
fn s8_4_parity_with_the_old_flow() {
    use Documented::{InputsOfMissingInclude, OptionSuffix};
    let f = Fixture::new("parity");
    let none = Documented::None;

    // Two headers, one guarded, one `#pragma once`, each included twice.
    f.write("guarded.h", "#ifndef G\n#define G\nint g;\n#endif\n");
    f.write("once.h", "#pragma once\nint o;\n");
    f.write(
        "two.cpp",
        "#include \"guarded.h\"\n#include \"once.h\"\n#include \"guarded.h\"\n#include \"once.h\"\n",
    );
    let s = assert_parity(&f, ".", &["c++", "two.cpp"], none);
    assert_eq!(counter(&s, "inputs.files"), Some(3));

    // A header through a symbolic link and through `..`.
    f.write("inc/real.h", "#pragma once\nint r;\n");
    #[cfg(unix)]
    std::os::unix::fs::symlink(f.root.join("inc/real.h"), f.root.join("link.h")).unwrap();
    f.write("sub/links.cpp", "#include \"../link.h\"\n#include \"../sub/../inc/real.h\"\n");
    let s = assert_parity(&f, "sub", &["c++", "links.cpp"], none);
    assert_eq!(counter(&s, "inputs.files"), Some(2));

    // `__has_include` of a file not included and of a missing one.
    f.write("probed.h", "int never;\n");
    f.write(
        "probe.cpp",
        "#if __has_include(\"probed.h\")\n#endif\n#if __has_include(\"absent.h\")\n#endif\nint p;\n",
    );
    let s = assert_parity(&f, ".", &["c++", "probe.cpp"], none);
    assert_eq!(counter(&s, "inputs.files"), Some(2), "probed.h is an input");

    // `#embed` and `__has_embed`.
    f.write_bytes("data.bin", b"\x01\x02\x03");
    f.write_bytes("other.bin", b"\x04");
    f.write(
        "embed.cpp",
        "const unsigned char d[] = {\n#embed \"data.bin\"\n};\nstatic_assert(sizeof d == 3);\n\
         #if __has_embed(\"other.bin\")\n#endif\n#if __has_embed(\"absent.bin\")\n#endif\n",
    );
    let s = assert_parity(&f, ".", &["c++", "-std=c++26", "embed.cpp"], none);
    assert_eq!(s.outcome.status, Status::Ok, "{:?}", s.outcome);
    assert_eq!(counter(&s, "inputs.files"), Some(3));

    // `-include` of a file.
    f.write("pre.h", "int pre;\n");
    f.write("inc.cpp", "int use = pre;\n");
    let s = assert_parity(&f, ".", &["c++", "-include", "pre.h", "inc.cpp"], none);
    assert_eq!(counter(&s, "inputs.files"), Some(2));

    // A header holding invalid UTF-8 and one with a byte-order mark.
    f.write_bytes("bad.h", b"// bad \xFF\xFE bytes\nint bad;\n");
    f.write_bytes("bom.h", b"\xEF\xBB\xBFint bom;\n");
    f.write("bytes.cpp", "#include \"bad.h\"\n#include \"bom.h\"\n");
    let s = assert_parity(&f, ".", &["c++", "bytes.cpp"], none);
    assert_eq!(counter(&s, "inputs.files"), Some(3));

    // Warnings (`-Wall` on an unused variable), and `-Werror`.
    f.write("warn.cpp", "void f() { int unused; }\n");
    let s = assert_parity(&f, ".", &["c++", "-Wall", "warn.cpp"], none);
    assert_eq!((s.outcome.status, s.outcome.warnings), (Status::Ok, 1));
    let s = assert_parity(&f, ".", &["c++", "-Wall", "-Werror", "warn.cpp"], OptionSuffix);
    assert_eq!((s.outcome.status, s.outcome.errors), (Status::Diagnosed, 1));

    // An error in a header: its position in the message.
    f.write("err.h", "int e = undeclared;\n");
    f.write("err.cpp", "#include \"err.h\"\n");
    let s = assert_parity(&f, ".", &["c++", "err.cpp"], none);
    assert!(s.outcome.message.contains("err.h:1:9: "), "{}", s.outcome.message);

    // A fatal missing include.
    f.write("fatal.cpp", "int before;\n#include \"nowhere.h\"\nint after;\n");
    let s = assert_parity(&f, ".", &["c++", "fatal.cpp"], InputsOfMissingInclude);
    assert_eq!(s.outcome.errors, 1);

    // `-ferror-limit=1` with three errors.
    f.write("limit.cpp", "int a = x1;\nint b = x2;\nint c = x3;\n");
    let s = assert_parity(&f, ".", &["c++", "-ferror-limit=1", "limit.cpp"], none);
    assert_eq!(s.outcome.status, Status::Diagnosed);

    // An unknown `-std=`: rejected.
    let s = assert_parity(&f, ".", &["c++", "-std=c++99x", "two.cpp"], none);
    assert_eq!(s.outcome.status, Status::Rejected);

    // A missing main file: rejected, two driver errors as `kiln` prints.
    let s = assert_parity(&f, ".", &["c++", "missing.cpp"], none);
    assert_eq!((s.outcome.status, s.outcome.errors), (Status::Rejected, 2));
}

// §8.4 counters: `pp.tokens` is the tokens the parser received, the end of
// file not counted, and the same on a second run.
#[test]
fn s8_4_counters_pp_tokens_by_hand() {
    let f = Fixture::new("tokens");
    f.write("h.h", "#define TWO 2\nint h_decl;\n");
    f.write("main.cpp", "#include \"h.h\"\nint main_v = TWO + 1;\n");
    let s = f.parse(".", &["c++", "main.cpp"]);
    assert_eq!(s.outcome.status, Status::Ok, "{:?}", s.outcome);
    // By hand: `int h_decl ;` (3), `int main_v = 2 + 1 ;` (7).
    assert_eq!(counter(&s, "pp.tokens"), Some(10));
    let again = f.parse(".", &["c++", "main.cpp"]);
    assert_eq!(counter(&again, "pp.tokens"), Some(10));
    // No parse: no counter at all.
    let s = f.parse(".", &["c++", "-std=c++99x", "main.cpp"]);
    assert!(s.counters.is_empty());
}

// §8.4 `parse_observer_with`: the closure sees the finished unit.
#[test]
fn s8_4_with_closure_sees_the_unit() {
    let f = Fixture::new("with");
    let header = f.write("h.h", "int helper(int);\n");
    let main = f.write("main.cpp", "#include \"h.h\"\nint wanted(int x) { return helper(x); }\n");
    let o = f.observer(".", &["c++", "main.cpp"]);
    let (shard, seen) = parse_observer_with(&o, Phase::Map, 3, |unit| {
        let ast = unit.ast();
        let found = ast.decls.ids().any(|d| {
            let node = ast.decls.get(d);
            node.kind() == DeclKind::Function
                && node.name().as_ident().is_some_and(|s| s.as_str(ast.symbols()) == "wanted")
        });
        let sm = unit.sm();
        let main_path = unit.main_file().and_then(|m| unit.real_path(m)).map(<[u8]>::to_vec);
        let header_path = sm
            .file_ids()
            .find(|&b| sm.file_name(b).ends_with("h.h"))
            .and_then(|b| unit.real_path(b))
            .map(<[u8]>::to_vec);
        let virtual_buffers: Vec<_> =
            sm.file_ids().filter(|&b| sm.file_origin(b) == FileOrigin::Virtual).collect();
        let virtual_paths =
            virtual_buffers.iter().filter(|&&b| unit.real_path(b).is_some()).count();
        (found, main_path, header_path, virtual_buffers.len(), virtual_paths, unit.errors())
    })
    .unwrap();
    let (found, main_path, header_path, virtuals, virtual_paths, errors) = seen.unwrap();
    assert!(found, "the closure finds `wanted` in the AST");
    let inputs = shard.inputs.unwrap();
    let entry = |p: &Path| inputs.iter().find(|i| i.path == file_ref(p).path).unwrap().path.clone();
    assert_eq!(main_path, Some(entry(&main)));
    assert_eq!(header_path, Some(entry(&header)));
    assert!(virtuals >= 1, "the predefines are a buffer");
    assert_eq!(virtual_paths, 0, "the predefines have no real path");
    assert_eq!(errors, 0);
}

// §8.4 `parse_observer_with`: the closure runs for a `diagnosed` unit and
// not for a `rejected` one.
#[test]
fn s8_4_with_closure_runs_when_a_parse_happened() {
    let f = Fixture::new("with-status");
    f.write("bad.cpp", "int x = nope;\n");
    let o = f.observer(".", &["c++", "bad.cpp"]);
    let (shard, seen) = parse_observer_with(&o, Phase::Map, 3, errors_of).unwrap();
    assert_eq!(shard.outcome.status, Status::Diagnosed);
    assert_eq!(seen, Some(1));
    for argv in [&["c++", "-std=c++99x", "bad.cpp"][..], &["c++", "-E", "bad.cpp"][..]] {
        let o = f.observer(".", argv);
        let (shard, seen) =
            parse_observer_with(&o, Phase::Map, 3, |_| -> u8 { panic!("not called") }).unwrap();
        assert_eq!(shard.outcome.status, Status::Rejected, "{argv:?}");
        assert_eq!(seen, None, "{argv:?}");
    }
}

// §8.4 the run: a closure that panics is `Err`, and the next call works.
#[test]
fn s8_4_with_panicking_closure_is_err() {
    let f = Fixture::new("with-panic");
    f.write("main.cpp", "int x;\n");
    let o = f.observer(".", &["c++", "main.cpp"]);
    let e = parse_observer_with(&o, Phase::Map, 3, |_| -> u8 { panic!("boom {}", 7) }).unwrap_err();
    assert_eq!(e, "the front end panicked: boom 7");
    let (shard, seen) = parse_observer_with(&o, Phase::Map, 3, errors_of).unwrap();
    assert_eq!((shard.outcome.status, seen), (Status::Ok, Some(0)));
}

// §8.4 inputs: an `#embed` resource that cannot be read here is `Err`
// (`read_consulted`'s unit test in `src/parse.rs`). Kiln's header search
// reads a resource whole when it finds it, so a directory or an unreadable
// file is never found: not consulted, not an input, no `Err`
// (`spec/landed/model/WP-025.md`).
#[test]
fn s8_4_inputs_directory_embed_resource_is_not_found() {
    let f = Fixture::new("embed-dir");
    std::fs::create_dir_all(f.root.join("res.bin")).unwrap();
    let main = f.write("main.cpp", "#if __has_embed(\"res.bin\")\n#error found\n#endif\nint x;\n");
    let s = f.parse(".", &["c++", "-std=c++26", "main.cpp"]);
    assert_eq!(s.outcome.status, Status::Ok, "{:?}", s.outcome);
    assert_eq!(s.inputs, Some(vec![file_ref(&main)]));
}

// §8.4 outcome, at the Kiln pin `83566cc47` (WP-044): the smallest
// reproducers of the Oven panics Slip reported (docs/requests/KD1, KD2),
// which failed at the old pin `62d190007`, parse `ok`.
#[test]
fn s8_4_outcome_kd1_kd2_reproducers_are_ok() {
    let f = Fixture::new("kd");
    let cases = [
        // KD1 §1: an assignment of an array new with a bound that is not
        // a constant, of a class with a default member initialiser.
        (
            "kd1_new.cpp",
            "struct S { unsigned x = 0; };\nS *p;\n\
             void f(unsigned long n) { p = new S[n]; }\n",
        ),
        // KD1 §2: a bit-field of an anonymous union through `->`, in an
        // instantiated body.
        (
            "kd1_union.cpp",
            "struct T { union { unsigned a : 2; int other; }; };\n\
             template <class P> void ensure(T *t, T *ct) { t->a = ct->a; }\n\
             void g(T *t) { ensure<int>(t, t); }\n",
        ),
        // KD2: a parenthesised `std::min` call as a shift count.
        (
            "kd2_minmax.cpp",
            "namespace std {\n\
             template <class T> constexpr const T &min(const T &a, const T &b) \
             { return b < a ? b : a; }\n}\n\
             unsigned long long f(unsigned long n) \
             { return 1ull << (std::min((unsigned long)31, n)); }\n",
        ),
    ];
    for (name, text) in cases {
        f.write(name, text);
        let s = f.parse(".", &["clang++", "-std=c++17", name]);
        assert_eq!(s.outcome.status, Status::Ok, "{name}: {:?}", s.outcome);
        assert_eq!((s.outcome.errors, s.outcome.warnings), (0, 0), "{name}");
    }
}

// §8.4 the run (WP-044): the entry leaves Kiln's written-reference and
// declaration-extent tables off (`spec/ast.md` §18, §19 at Kiln
// `83566cc47`), so a unit full of uses has both empty.
#[test]
fn s8_4_run_written_refs_and_decl_extents_are_off() {
    let f = Fixture::new("written-refs-off");
    f.write(
        "main.cpp",
        "namespace n { struct A { int v; }; int g(A); }\n\
         int use(n::A a) { return n::g(a) + a.v; }\n",
    );
    let o = f.observer(".", &["c++", "main.cpp"]);
    let (shard, seen) = parse_observer_with(&o, Phase::Map, 3, |unit| {
        let ast = unit.ast();
        (
            ast.written_refs.is_on(),
            ast.written_refs.len(),
            ast.decl_extents.is_on(),
            ast.decl_extents.len(),
        )
    })
    .unwrap();
    assert_eq!(shard.outcome.status, Status::Ok, "{:?}", shard.outcome);
    assert_eq!(seen, Some((false, 0, false, 0)));
}
