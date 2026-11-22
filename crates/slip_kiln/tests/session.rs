//! `slip_kiln::session`, the transform session (`docs/wp/WP-080.md`
//! "Tests"; `spec/api.md` §2.1, §2.2, §2.3, §2.4 rule 5, §2.5, §2.8). Each
//! test is named after the item of the package it covers (`t1` … `t8`).
//!
//! The transforms are written to a directory of their own under Cargo's
//! temporary directory and named by absolute path. One test runs
//! `parse_observer_with`, which changes the process's current directory;
//! nothing here depends on it.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use slip_kiln::kiln::ast::Ast;
use slip_kiln::kiln::oven::{HostCalls, HostError, HostOut};
use slip_kiln::parse_observer_with;
use slip_kiln::session::{
    Budget, Cancel, CompileError, EntryError, Handler, Out, RunFailure, RunReport, Session,
    TransformCommand, with_transform,
};
use slip_kiln::snippet::{Snippet, parse};
use slip_model::hash::content_digest;
use slip_model::{Command, Digest, FileRef, Language, Observer, Origin, Phase};

/// The defaults of `spec/api.md` §2.5.
const BUDGET: Budget = Budget { steps: 1 << 32, bytes: 256 << 20, depth: 4096 };

/// The stack of the thread a transform is compiled on.
const STACK: usize = 64 << 20;

/// `<resource>/include` of this checkout.
fn resource_include() -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../resource/include");
    std::fs::canonicalize(p).unwrap()
}

/// A new directory of this test process.
fn scratch(name: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("session-{}-{n}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::canonicalize(&dir).unwrap()
}

/// The command of a transform file `name` with `text`, in a new directory.
fn command(name: &str, text: &str, author: &[&str]) -> TransformCommand {
    let dir = scratch(name);
    let file = dir.join(format!("{name}.cpp"));
    std::fs::write(&file, text).unwrap();
    let author: Vec<String> = author.iter().map(|&a| a.to_owned()).collect();
    TransformCommand::new(resource_include(), &author, file).expect("a valid command")
}

/// `with_transform(cmd, ..., f)` on a thread with the stack it needs.
fn on_stack<R: Send>(
    cmd: &TransformCommand,
    f: impl FnOnce(Result<Session<'_>, CompileError>) -> R + Send,
) -> R {
    std::thread::scope(|s| {
        std::thread::Builder::new()
            .stack_size(STACK)
            .spawn_scoped(s, || with_transform(cmd, STACK - (1 << 20), f))
            .unwrap()
            .join()
            .unwrap()
    })
}

/// `f` with the session of `cmd`.
fn with<R: Send>(cmd: &TransformCommand, f: impl FnOnce(&mut Session<'_>) -> R + Send) -> R {
    on_stack(cmd, |opened| match opened {
        Ok(mut session) => f(&mut session),
        Err(e) => panic!("the transform does not compile: {e:?}"),
    })
}

/// Why `cmd` does not compile.
fn compile_error(cmd: &TransformCommand) -> CompileError {
    on_stack(cmd, |opened| match opened {
        Ok(_) => panic!("the transform compiles"),
        Err(e) => e,
    })
}

/// `host(op, a)`: one host call with one integer argument and one result.
const PRELUDE: &str = r"
using u64 = unsigned long long;
consteval u64 host(unsigned op, u64 a) {
  u64 in[1] = {a};
  const char bin[1] = {0};
  u64 out[1] = {0};
  char bout[1] = {0};
  return __builtin_kiln_host_call(op, in, 1, bin, 0, out, 1, bout, 1);
}
";

/// A transform of `PRELUDE` and `body`.
fn transform(name: &str, body: &str) -> TransformCommand {
    command(name, &format!("{PRELUDE}{body}"), &[])
}

/// Answers op 1 with twice its argument; counts its calls.
#[derive(Default)]
struct Counting {
    calls: u64,
}

impl Handler for Counting {
    fn call(&mut self, op: u32, ints: &[u64], _: &[u8], out: &mut dyn Out) -> Result<u64, String> {
        self.calls += 1;
        assert_eq!(op, 1);
        assert_eq!((out.int_capacity(), out.byte_capacity()), (1, 1));
        out.write_ints(&[ints[0]]);
        out.write_bytes(b"x");
        Ok(ints[0] * 2)
    }
}

/// Answers op 1 with the number of top-level declarations of a subject
/// unit; records what op 2 reports.
struct Subject<'s> {
    ast: &'s Ast,
    reported: Vec<u64>,
}

/// The top-level declarations of `ast`.
fn top_level(ast: &Ast) -> u64 {
    ast.members(ast.tu).count() as u64
}

impl Handler for Subject<'_> {
    fn call(&mut self, op: u32, ints: &[u64], _: &[u8], _: &mut dyn Out) -> Result<u64, String> {
        if op == 1 {
            Ok(top_level(self.ast))
        } else {
            self.reported.push(ints[0]);
            Ok(0)
        }
    }
}

/// Answers op 1 with a fixed number; records what op 2 reports.
struct Fixed {
    value: u64,
    reported: Vec<u64>,
}

impl Handler for Fixed {
    fn call(&mut self, op: u32, ints: &[u64], _: &[u8], _: &mut dyn Out) -> Result<u64, String> {
        if op == 1 {
            Ok(self.value)
        } else {
            self.reported.push(ints[0]);
            Ok(0)
        }
    }
}

/// Records the bytes of every call.
#[derive(Default)]
struct Recording {
    bytes: Vec<Vec<u8>>,
}

impl Handler for Recording {
    fn call(&mut self, _: u32, _: &[u64], bytes: &[u8], _: &mut dyn Out) -> Result<u64, String> {
        self.bytes.push(bytes.to_vec());
        Ok(0)
    }
}

/// Fails every call.
struct Failing;

impl Handler for Failing {
    fn call(&mut self, _: u32, _: &[u64], _: &[u8], _: &mut dyn Out) -> Result<u64, String> {
        Err("the handler refuses".into())
    }
}

/// The parts of a report a test compares.
fn summary(r: &RunReport) -> (Result<(), RunFailure>, u64, u64) {
    (r.result.clone(), r.steps, r.host_calls)
}

/// A Kiln handler that borrows the subject's AST
/// (`docs/design/notes/kiln-landed-2026-11.md` §3.4 item 1).
struct Borrowing<'s> {
    ast: &'s Ast,
}

impl HostCalls for Borrowing<'_> {
    fn call(
        &mut self,
        _op: u32,
        _ints: &[u64],
        _bytes: &[u8],
        _out: &mut HostOut<'_>,
    ) -> Result<u64, HostError> {
        Ok(top_level(self.ast))
    }
}

// WP-080 first task: a handler holding `&Ast` satisfies `HostCalls: Send`,
// so does a `Handler` holding one, and a `Session` is `Send`.
#[test]
fn t1_handler_borrowing_ast_is_send() {
    fn is_host_calls<H: HostCalls>() {}
    fn is_handler<H: Handler>() {}
    fn is_send<T: Send>() {}
    is_host_calls::<Borrowing<'_>>();
    is_handler::<Subject<'_>>();
    is_send::<Session<'_>>();
}

/// The subject unit of t1 and t3.
const SUBJECT: &str = "int a; int b(); struct C {}; namespace d { int e; }";

/// Reports the subject's count once.
const REPORT: &str = "consteval void transform() { host(2, host(1, 0)); }";

// WP-080 test 1 and test 3 (one thread): a handler holding the `&Ast` of a
// snippet runs, while the snippet's front end is alive on the thread of
// the session; the run and the subject give what they give alone.
#[test]
fn t1_t3_handler_over_a_snippet_on_one_thread() {
    let snippet =
        Snippet { files: &[("s.cpp", SUBJECT)], main: "s.cpp", include: &[], std: "c++20" };
    let alone = parse(&snippet, |p| top_level(p.ast)).unwrap();
    assert!(alone >= 4, "the subject alone: {alone}");
    let cmd = transform("t3_one", REPORT);
    let (alone_run, together, inside) = with(&cmd, |s| {
        let entry = s.entry("transform").unwrap();
        let mut fixed = Fixed { value: alone, reported: Vec::new() };
        let alone_run = summary(&s.run(entry, &mut fixed, BUDGET, &Cancel::new()));
        assert_eq!(fixed.reported, [alone]);
        let (together, inside) = parse(&snippet, |p| {
            let mut h = Subject { ast: p.ast, reported: Vec::new() };
            let r = summary(&s.run(entry, &mut h, BUDGET, &Cancel::new()));
            (r, (h.reported, top_level(p.ast)))
        })
        .unwrap();
        (alone_run, together, inside)
    });
    assert_eq!(alone_run.0, Ok(()));
    assert_eq!(alone_run.2, 2);
    assert_eq!(together, alone_run, "the run beside a subject front end");
    assert_eq!(inside, (vec![alone], alone), "the subject beside a session");
}

/// An observer of `argv` in `dir`.
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

// WP-080 test 3 (two threads): the session runs on the thread of a subject
// unit's front end (`parse_observer_with`'s closure) while the thread of
// `with_transform` waits; the shard and the run are what they are alone.
#[test]
fn t3_session_on_a_subject_front_end_thread() {
    let dir = scratch("t3_two");
    std::fs::write(dir.join("s.cpp"), SUBJECT).unwrap();
    let o = observer(&dir, &["clang++", "-std=c++20", "-c", "s.cpp"]);
    let (alone_shard, alone) =
        parse_observer_with(&o, Phase::Map, 1, |u| top_level(u.ast())).unwrap();
    let alone = alone.unwrap();
    assert!(alone >= 4, "{alone}");
    let cmd = transform("t3_two", REPORT);
    let (alone_run, together_shard, together) = with(&cmd, |s| {
        let entry = s.entry("transform").unwrap();
        let mut fixed = Fixed { value: alone, reported: Vec::new() };
        let alone_run = summary(&s.run(entry, &mut fixed, BUDGET, &Cancel::new()));
        let here = std::thread::current().id();
        let (shard, value) = parse_observer_with(&o, Phase::Map, 1, |u| {
            assert_ne!(std::thread::current().id(), here, "the front end has its own thread");
            let mut h = Subject { ast: u.ast(), reported: Vec::new() };
            let r = summary(&s.run(entry, &mut h, BUDGET, &Cancel::new()));
            (r, h.reported, top_level(u.ast()))
        })
        .unwrap();
        (alone_run, shard, value.unwrap())
    });
    assert_eq!(alone_run.0, Ok(()));
    assert_eq!(together_shard, alone_shard, "the subject beside a session");
    assert_eq!(together, (alone_run, vec![alone], alone), "the run beside a subject");
}

// WP-080 test 2 (spec/api.md §2.4 rule 5): compiled once, run 1000 times
// with a counting handler; every run gives the same result, steps and
// host-call count.
#[test]
fn t2_compile_once_run_many() {
    let cmd = transform(
        "t2",
        "consteval void transform() {
           u64 s = 0;
           for (u64 i = 0; i < 10; ++i) s += host(1, i);
           if (s != 90) throw s;
         }",
    );
    let (all, calls) = with(&cmd, |s| {
        let entry = s.entry("transform").unwrap();
        let mut h = Counting::default();
        let cancel = Cancel::new();
        let runs: Vec<_> =
            (0..1000).map(|_| summary(&s.run(entry, &mut h, BUDGET, &cancel))).collect();
        (runs, h.calls)
    });
    let first = all[0].clone();
    assert_eq!(first.0, Ok(()));
    assert_eq!(first.2, 10);
    assert!(first.1 > 0);
    assert!(all.iter().all(|r| *r == first), "every run is the first");
    assert_eq!(calls, 10_000);
}

// WP-080 test 4 (spec/api.md §2.3 rules 2 and 3): each `EntryError`, in the
// order of the table where a function fails two; `declares` for the
// reserved names.
#[test]
fn t4_entry_errors_and_reserved_names() {
    let cmd = transform(
        "t4",
        "consteval void over() {}
         consteval void over(int) {}
         int variable;
         template <class T> consteval void templ() {}
         constexpr void plain() {}
         consteval void params(int) {}
         consteval int returns() { return 0; }
         consteval int returns_undefined();
         consteval void undefined();
         consteval void fine() {}
         namespace ns { consteval void fine() {} }
         void reduce();
         template <class T> void prepare(T);
         int repair;",
    );
    let (entries, declares) = with(&cmd, |s| {
        let entries: Vec<_> = [
            "missing",
            "over",
            "variable",
            "templ",
            "plain",
            "params",
            "returns",
            "returns_undefined",
            "undefined",
            "fine",
            "ns::fine",
        ]
        .iter()
        .map(|n| s.entry(n).map(|_| ()))
        .collect();
        let declares: Vec<_> = ["reduce", "prepare", "repair", "fine", "nothing"]
            .iter()
            .map(|n| s.declares(n))
            .collect();
        (entries, declares)
    });
    assert_eq!(
        entries,
        [
            Err(EntryError::Missing),
            Err(EntryError::Overloaded),
            Err(EntryError::NotFunction),
            Err(EntryError::NotFunction),
            Err(EntryError::NotConsteval),
            Err(EntryError::Parameters),
            Err(EntryError::Type),
            Err(EntryError::Type),
            Err(EntryError::Undefined),
            Ok(()),
            Err(EntryError::Missing),
        ]
    );
    assert_eq!(declares, [true, true, false, true, false]);
}

// WP-080 test 5 (spec/api.md §2.5, §2.8): each `RunFailure`, its rendered
// diagnostics holding the call stack; after each, a normal run completes.
#[test]
fn t5_each_run_failure_then_a_normal_run() {
    let cmd = transform(
        "t5",
        "constexpr int deref(int* p) { return *p; }
         consteval void not_constant() { deref(nullptr); }
         constexpr void spin(u64 n) { for (u64 i = 0; i < n; ++i) {} }
         consteval void steps() { spin(1000000); }
         constexpr void hold() { char* p = new char[1 << 20]; delete[] p; }
         consteval void memory() { hold(); }
         constexpr int down(int n) { return n == 0 ? 0 : down(n - 1) + 1; }
         consteval void depth() { down(100); }
         consteval void cancelled() { spin(1000000); }
         consteval u64 ask() { return host(1, 0); }
         consteval void host_failure() { ask(); }
         struct Thrown { int code; };
         constexpr void raise() { throw Thrown{3}; }
         consteval void uncaught() { raise(); }
         consteval void fine() { spin(10); }",
    );
    let results = with(&cmd, |s| {
        let fine = s.entry("fine").unwrap();
        let mut out = Vec::new();
        for (name, budget, cancelled) in [
            ("not_constant", BUDGET, false),
            ("steps", Budget { steps: 10_000, ..BUDGET }, false),
            ("memory", Budget { bytes: 1 << 16, ..BUDGET }, false),
            ("depth", Budget { depth: 16, ..BUDGET }, false),
            ("cancelled", BUDGET, true),
            ("host_failure", BUDGET, false),
            ("uncaught", BUDGET, false),
        ] {
            let entry = s.entry(name).unwrap();
            let cancel = Cancel::new();
            if cancelled {
                cancel.cancel();
            }
            let r = s.run(entry, &mut Failing, budget, &cancel);
            let after = s.run(fine, &mut Failing, BUDGET, &Cancel::new());
            out.push((name, r.result, r.rendered, after.result, after.rendered));
        }
        out
    });
    let expected = [
        RunFailure::NotConstant,
        RunFailure::StepBudget,
        RunFailure::MemoryBudget,
        RunFailure::DepthLimit,
        RunFailure::Cancelled,
        RunFailure::Host("the handler refuses".into()),
        RunFailure::Uncaught { type_name: "Thrown".into() },
    ];
    // The note that places the failure: the call to the function the entry
    // called (to the entry itself for a run cancelled before its first
    // step); for an exception, the throw (Kiln gives no call stack there).
    let callee = [
        "in call to 'deref(",
        "in call to 'spin(",
        "in call to 'hold(",
        "in call to 'down(",
        "in call to 'cancelled(",
        "in call to 'ask(",
        "t5.cpp:22:41: note: unhandled exception of type 'Thrown'",
    ];
    for (((name, result, rendered, after, after_rendered), want), callee) in
        results.into_iter().zip(expected).zip(callee)
    {
        assert_eq!(result, Err(want), "{name}: {rendered}");
        assert!(rendered.contains("t5.cpp:"), "{name}: {rendered}");
        assert!(rendered.contains("error:"), "{name}: {rendered}");
        assert!(rendered.contains("note:"), "{name}: {rendered}");
        assert!(rendered.contains(callee), "{name}: {rendered}");
        assert_eq!(after, Ok(()), "a normal run after {name}: {after_rendered}");
        assert_eq!(after_rendered, "", "a normal run after {name}");
    }
}

// WP-080 test 6 (spec/api.md §2.2): `files()` lists the transform file,
// `<slip/meta.h>`, `<slip/__intrinsics.h>` and the standard headers read,
// each once, in the order first read, with the digest of the bytes on
// disk.
#[test]
fn t6_files_read() {
    let cmd =
        command("t6", "#include <slip/meta.h>\nconsteval void transform() {}\n", &["-DUNUSED=1"]);
    let files = with(&cmd, |s| s.files().to_vec());
    let paths: Vec<String> =
        files.iter().map(|f| String::from_utf8(f.path.clone()).unwrap()).collect();
    for (f, path) in files.iter().zip(&paths) {
        let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e}"));
        assert_eq!(f.digest, content_digest(&bytes), "{path}");
    }
    for (i, f) in files.iter().enumerate() {
        assert!(!files[..i].contains(f), "each file once: {paths:#?}");
    }
    let at = |suffix: &str| {
        paths.iter().position(|p| p.ends_with(suffix)).unwrap_or_else(|| {
            panic!("{suffix} in {paths:#?}");
        })
    };
    let main = at("/t6.cpp");
    let meta = at("include/slip/meta.h");
    let intrinsics = at("include/slip/__intrinsics.h");
    let string = at("/string");
    assert!(main < meta && meta < intrinsics && intrinsics < string, "{paths:#?}");
    at("/string_view");
    at("/vector");
}

// WP-080 test 7 (spec/api.md §2.1 rule 4): `__DATE__` and `__TIME__` are
// those of the Unix epoch.
#[test]
fn t7_date_and_time_are_the_epoch() {
    let cmd = transform(
        "t7",
        "consteval void say(const char* s, unsigned n) {
           u64 in[1] = {0};
           u64 out[1] = {0};
           char bout[1] = {0};
           __builtin_kiln_host_call(3, in, 0, s, n, out, 0, bout, 0);
         }
         consteval void transform() {
           say(__DATE__, sizeof(__DATE__) - 1);
           say(__TIME__, sizeof(__TIME__) - 1);
         }",
    );
    let (result, said) = with(&cmd, |s| {
        let entry = s.entry("transform").unwrap();
        let mut h = Recording::default();
        let r = s.run(entry, &mut h, BUDGET, &Cancel::new());
        ((r.result, r.rendered), h.bytes)
    });
    assert_eq!(result.0, Ok(()), "{}", result.1);
    assert_eq!(said, [b"Jan  1 1970".to_vec(), b"00:00:00".to_vec()]);
}

// WP-080 test 8 (spec/api.md §2.1 rule 2): an author option outside the
// rule is refused when the command is built, so it never reaches Kiln; the
// options of the rule reach Kiln in order, each as two words.
#[test]
fn t8_author_options() {
    let new = |author: &[&str]| {
        let author: Vec<String> = author.iter().map(|&a| a.to_owned()).collect();
        TransformCommand::new(resource_include(), &author, PathBuf::from("/t.cpp"))
    };
    for bad in [
        &["-O2"][..],
        &["-fplugin=x.so"],
        &["-std=c++20"],
        &["-iquote", "d"],
        &["-idirafter", "d"],
        &["--include-directory=d"],
        &["-include-pch", "x.pch"],
        &["-isystemd"],
        &["-includex.h"],
        &["-I"],
        &["-D", "-fplugin=x.so"],
        &["-I", ""],
        &["t2.cpp"],
        &["-DX", "-Xclang", "-load"],
    ] {
        let e = new(bad).expect_err(&format!("{bad:?} is refused"));
        assert!(bad.contains(&e.word.as_str()), "{bad:?}: {e:?}");
    }
    assert!(TransformCommand::new(resource_include(), &[], PathBuf::from("-t.cpp")).is_err());
    let ok =
        new(&["-Ia", "-I", "b", "-isystem", "c", "-DX=1", "-D", "Y", "-UZ", "-include", "h.h"])
            .unwrap();
    let include = resource_include().to_str().unwrap().to_owned();
    assert_eq!(
        ok.command_line(),
        [
            "-std=c++26",
            "-fsyntax-only",
            "-fkiln-reproducible-time",
            "-I",
            &include,
            "-I",
            "a",
            "-I",
            "b",
            "-isystem",
            "c",
            "-D",
            "X=1",
            "-D",
            "Y",
            "-U",
            "Z",
            "-include",
            "h.h",
            "/t.cpp",
        ]
    );
}

// spec/api.md §2.1 rule 5 (`transform-compile`): a transform unit with an
// error gives its diagnostics as Kiln renders them, and no session; the
// author's `-D` reaches the unit.
#[test]
fn s2_1_rule5_a_unit_with_errors_is_diagnosed() {
    let text = "#ifndef GIVEN\n#error not given\n#endif\n\
                consteval void transform() { undeclared(); }\n";
    match compile_error(&command("bad", text, &[])) {
        CompileError::Diagnosed { rendered, errors } => {
            assert_eq!(errors, 2, "{rendered}");
            assert!(rendered.contains("bad.cpp:2:2: error: not given"), "{rendered}");
            assert!(rendered.contains("undeclared"), "{rendered}");
        }
        other => panic!("{other:?}"),
    }
    match compile_error(&command("given", text, &["-DGIVEN"])) {
        CompileError::Diagnosed { rendered, errors } => {
            assert_eq!(errors, 1, "{rendered}");
            assert!(!rendered.contains("not given"), "{rendered}");
        }
        other => panic!("{other:?}"),
    }
}
