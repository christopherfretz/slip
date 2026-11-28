//! `slip_kiln::session`, the transform session (`docs/wp/WP-080.md`
//! "Tests"; `spec/api.md` §2.1, §2.2, §2.3, §2.4 rule 5, §2.5, §2.8). Each
//! test is named after the item of the package it covers (`t1` … `t8`).
//!
//! The transforms are written to a directory of their own under Cargo's
//! temporary directory and named by absolute path. One test runs
//! `parse_observer_with`, which changes the process's current directory;
//! nothing here depends on it.

mod common;

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use slip_kiln::kiln::ast::Ast;
use slip_kiln::kiln::oven::{HostCalls, HostError, HostOut};
use slip_kiln::kiln::source::SourceManager;
use slip_kiln::session::{
    Budget, Cancel, CompileError, EntryError, Handler, Out, RunFailure, RunReport, Session,
    TransformCommand, with_transform,
};
use slip_kiln::snippet::{Snippet, parse};
use slip_kiln::{ParseOptions, Unit, parse_observer_with};
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
    let dir = common::work_root().join(format!("session-{}-{n}-{name}", std::process::id()));
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
    is_handler::<Holding<'_>>();
    is_send::<Session<'_>>();
    let _ = holding;
}

/// A handler holding every part of a subject unit that is `Send` at this
/// Kiln revision (WP-080 fix round, plan (b)): not the `Unit`, whose
/// `Oven` holds `RefCell`s, `Cell`s and a `dyn HostCalls`.
struct Holding<'s> {
    ast: &'s Ast,
    sm: &'s SourceManager,
    inputs: &'s [FileRef],
    filter: &'s (dyn Fn(&[u8]) -> bool + Sync),
}

/// The parts of `u`, for the unit's lifetime: `Unit` hands them out so.
fn holding<'r>(u: &Unit<'r>, filter: &'r (dyn Fn(&[u8]) -> bool + Sync)) -> Holding<'r> {
    Holding { ast: u.ast(), sm: u.sm(), inputs: u.inputs(), filter }
}

impl Handler for Holding<'_> {
    fn call(&mut self, _: u32, _: &[u64], bytes: &[u8], _: &mut dyn Out) -> Result<u64, String> {
        let files = self.sm.file_count() + self.inputs.len();
        Ok(top_level(self.ast) + u64::from((self.filter)(bytes)) + files as u64)
    }
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
        parse_observer_with(&o, Phase::Map, 1, ParseOptions::default(), |u| top_level(u.ast()))
            .unwrap();
    let alone = alone.unwrap();
    assert!(alone >= 4, "{alone}");
    let cmd = transform("t3_two", REPORT);
    let (alone_run, together_shard, together) = with(&cmd, |s| {
        let entry = s.entry("transform").unwrap();
        let mut fixed = Fixed { value: alone, reported: Vec::new() };
        let alone_run = summary(&s.run(entry, &mut fixed, BUDGET, &Cancel::new()));
        let here = std::thread::current().id();
        let (shard, value) = parse_observer_with(&o, Phase::Map, 1, ParseOptions::default(), |u| {
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
         int repair;
         namespace n { consteval void directed() {} consteval void declared() {} }
         using namespace n;
         using n::declared;
         inline namespace in { consteval void in_inline() {} }
         namespace { consteval void in_unnamed() {} }
         extern \"C++\" { consteval void in_linkage() {} }
         namespace outer { inline namespace v1 { consteval void nested() {} } }
         consteval void deleted() = delete;",
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
            "::fine",
            " fine ",
            "fine ",
            "",
            "directed",
            "declared",
            "in_inline",
            "in_unnamed",
            "in_linkage",
            "nested",
            "deleted",
        ]
        .iter()
        .map(|n| s.entry(n).map(|_| ()))
        .collect();
        let declares: Vec<_> = [
            "reduce",
            "prepare",
            "repair",
            "fine",
            "nothing",
            "directed",
            "declared",
            "in_inline",
            "in_unnamed",
            "nested",
            " fine",
        ]
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
            Err(EntryError::Missing),
            Err(EntryError::Missing),
            Err(EntryError::Missing),
            Err(EntryError::Missing),
            Err(EntryError::Missing),
            Err(EntryError::Missing),
            Ok(()),
            Ok(()),
            Ok(()),
            Err(EntryError::Missing),
            Err(EntryError::Undefined),
        ]
    );
    assert_eq!(declares, [true, true, false, true, false, false, false, true, true, false, false]);
}

/// Whether the unit of `text` has an entry `transform`, and whether it
/// declares `reduce`.
fn entry_and_reduce(name: &str, text: &str) -> (Result<(), EntryError>, bool) {
    with(&command(name, text, &[]), |s| (s.entry("transform").map(|_| ()), s.declares("reduce")))
}

// WP-080 fix round, S3 (spec/api.md §6.1 rules 3 and 5): a name reached
// through a using-directive or a using-declaration is not a member of the
// global namespace, nor is one of `std` after `using namespace std;`.
#[test]
fn t4_using_is_not_followed() {
    let directive = "namespace n { consteval void transform() {} }\nusing namespace n;\n";
    let declaration = "namespace n { consteval void transform() {} }\nusing n::transform;\n";
    let numeric = "#include <numeric>\nusing namespace std;\nconsteval void transform() {}\n";
    assert_eq!(entry_and_reduce("directive", directive), (Err(EntryError::Missing), false));
    assert_eq!(entry_and_reduce("declaration", declaration), (Err(EntryError::Missing), false));
    assert_eq!(entry_and_reduce("numeric", numeric), (Ok(()), false));
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

// WP-080 test 7 (spec/api.md §2.1 rule 4): `__DATE__`, `__TIME__` and
// `__TIMESTAMP__` are those of the Unix epoch.
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
           say(__TIMESTAMP__, sizeof(__TIMESTAMP__) - 1);
         }",
    );
    let (result, said) = with(&cmd, |s| {
        let entry = s.entry("transform").unwrap();
        let mut h = Recording::default();
        let r = s.run(entry, &mut h, BUDGET, &Cancel::new());
        ((r.result, r.rendered), h.bytes)
    });
    assert_eq!(result.0, Ok(()), "{}", result.1);
    assert_eq!(
        said,
        [b"Jan  1 1970".to_vec(), b"00:00:00".to_vec(), b"Thu Jan  1 00:00:00 1970".to_vec()]
    );
}

// WP-080 test 8 (spec/api.md §2.1 rules 2 and 3): an author option outside
// the rule is refused when the command is built, so it never reaches Kiln,
// naming the word and why; so is a value or path beginning with `@` (a
// response file), holding a NUL byte, or (a path) not absolute. The
// options of the rule reach Kiln in order, each as two words.
#[test]
fn t8_author_options() {
    let new = |author: &[&str]| {
        let author: Vec<String> = author.iter().map(|&a| a.to_owned()).collect();
        TransformCommand::new(resource_include(), &author, PathBuf::from("/t.cpp"))
    };
    let only = "only -I, -isystem, -D, -U and -include may be given (spec/api.md §2.1)";
    let missing = "the option's value is missing";
    let dash = "the value begins with `-`, which Kiln would read as an option";
    let at = "the value begins with `@`, which Kiln would read as a response file";
    let nul = "the value holds a NUL byte";
    let relative = "the path is not absolute";
    let joined = "-isystem and -include take their value as the next word";
    for (bad, word, reason) in [
        (&["-O2"][..], "-O2", only),
        (&["-fplugin=x.so"], "-fplugin=x.so", only),
        (&["-std=c++20"], "-std=c++20", only),
        (&["-iquote", "/d"], "-iquote", only),
        (&["-idirafter", "/d"], "-idirafter", only),
        (&["--include-directory=/d"], "--include-directory=/d", only),
        (&["-include-pch", "/x.pch"], "-include-pch", joined),
        (&["-isystem/d"], "-isystem/d", joined),
        (&["-include/x.h"], "-include/x.h", joined),
        (&["-I"], "-I", missing),
        (&["-DX", "-I"], "-I", missing),
        (&["-D", "-fplugin=x.so"], "-fplugin=x.so", dash),
        (&["-I", ""], "", "the value is empty"),
        (&["t2.cpp"], "t2.cpp", only),
        (&["@opts.rsp"], "@opts.rsp", only),
        (&["-DX", "-Xclang", "-load"], "-Xclang", only),
        (&["-D", "@opts.rsp"], "@opts.rsp", at),
        (&["-D@opts.rsp"], "-D@opts.rsp", at),
        (&["-U", "@opts.rsp"], "@opts.rsp", at),
        (&["-I", "@opts.rsp"], "@opts.rsp", at),
        (&["-I@opts.rsp"], "-I@opts.rsp", at),
        (&["-isystem", "@opts.rsp"], "@opts.rsp", at),
        (&["-include", "@opts.rsp"], "@opts.rsp", at),
        (&["-DX\0Y"], "-DX\0Y", nul),
        (&["-I", "/a\0b"], "/a\0b", nul),
        (&["-Ia"], "-Ia", relative),
        (&["-I", "b"], "b", relative),
        (&["-I", "./b"], "./b", relative),
        (&["-isystem", "c"], "c", relative),
        (&["-include", "h.h"], "h.h", relative),
    ] {
        let e = new(bad).expect_err(&format!("{bad:?} is refused"));
        assert_eq!((e.word.as_str(), e.reason), (word, reason), "{bad:?}");
    }
    let path_refused = |resource: &str, file: &str| {
        let e = TransformCommand::new(PathBuf::from(resource), &[], PathBuf::from(file))
            .expect_err(&format!("{resource} {file} is refused"));
        (e.word, e.reason)
    };
    let include = resource_include().to_str().unwrap().to_owned();
    for (file, reason) in [
        ("-t.cpp", dash),
        ("@t.cpp", at),
        ("t.cpp", relative),
        ("./t.cpp", relative),
        ("/t\0.cpp", nul),
        ("", "the value is empty"),
    ] {
        assert_eq!(path_refused(&include, file), (file.to_owned(), reason), "file {file:?}");
    }
    for (resource, reason) in
        [("@include", at), ("include", relative), ("-include", dash), ("/in\0clude", nul)]
    {
        assert_eq!(path_refused(resource, "/t.cpp"), (resource.to_owned(), reason), "{resource:?}");
    }
    let ok = new(&[
        "-I/a", "-I", "/b", "-isystem", "/c", "-DX=1", "-D", "Y", "-UZ", "-include", "/h.h",
        "-DW=a@b",
    ])
    .unwrap();
    assert_eq!(
        ok.command_line(),
        [
            "-std=c++26",
            "-fsyntax-only",
            "-fkiln-reproducible-time",
            "-ftemplate-depth=512",
            "-I",
            &include,
            "-I",
            "/a",
            "-I",
            "/b",
            "-isystem",
            "/c",
            "-D",
            "X=1",
            "-D",
            "Y",
            "-U",
            "Z",
            "-include",
            "/h.h",
            "-D",
            "W=a@b",
            "/t.cpp",
        ]
    );
}

// WP-080 fix round, B1: Kiln's driver expanded a response file named by a
// word beginning with `@`, so a word `@opts.rsp` would bring in any
// option. Since Kiln WP-2411 `with_eval_session` reads no `@file` (the
// word is an ordinary word: `kiln_driver/src/session.rs:167-170`): given
// to Kiln directly, the file's `-DSMUGGLED=1` does not arrive. Slip still
// refuses every way of naming it through `TransformCommand`; the unit
// built from the options that remain does not see it.
#[test]
fn b1_a_response_file_never_reaches_kiln() {
    let dir = scratch("b1");
    let rsp = dir.join("opts.rsp");
    std::fs::write(&rsp, "-DSMUGGLED=1 -std=c++20\n").unwrap();
    let file = dir.join("b1.cpp");
    let text = "#ifdef SMUGGLED\n#error smuggled\n#endif\n\
                static_assert(__cplusplus > 202302L);\nconsteval void transform() {}\n";
    std::fs::write(&file, text).unwrap();
    let rsp_word = format!("@{}", rsp.to_str().unwrap());
    // The hole, closed by Kiln: given the word, Kiln does not read the file.
    let mut line =
        TransformCommand::new(resource_include(), &[], file.clone()).unwrap().command_line();
    line.insert(line.len() - 1, rsp_word.clone());
    let raw = std::thread::scope(|s| {
        std::thread::Builder::new()
            .stack_size(STACK)
            .spawn_scoped(s, || {
                kiln_sema::stack_guard::StackGuard::install(STACK - (1 << 20));
                let buffer = kiln_diag::SharedBuffer::new();
                let io = kiln_driver::Io {
                    stdout: Box::new(std::io::sink()),
                    stderr: kiln_driver::SharedWriter::new(buffer.clone()),
                    stdin: Box::new(std::io::empty()),
                };
                let ok = kiln_driver::session::with_eval_session("kiln", &line, io, |r| r.is_ok());
                (ok, buffer.contents())
            })
            .unwrap()
            .join()
            .unwrap()
    });
    assert!(!raw.1.contains("smuggled"), "Kiln expands the file: {}", raw.1);
    let author =
        |words: &[&str]| words.iter().map(|&w| w.replace("RSP", &rsp_word)).collect::<Vec<_>>();
    for words in [&["-D", "RSP"][..], &["-DRSP"], &["-I", "RSP"], &["-IRSP"], &["RSP"]] {
        let words = author(words);
        let e = TransformCommand::new(resource_include(), &words, file.clone())
            .expect_err(&format!("{words:?}"));
        assert!(e.word.contains(&rsp_word), "{e:?}");
    }
    let e = TransformCommand::new(resource_include(), &[], PathBuf::from(&rsp_word)).unwrap_err();
    assert_eq!(e.word, rsp_word);
    let cmd = TransformCommand::new(resource_include(), &author(&["-DNOT=RSP"]), file).unwrap();
    assert!(with(&cmd, |s| s.entry("transform").is_ok()), "SMUGGLED did not arrive");
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

// WP-080 fix round, M12: what a handler writes (`Out::write_ints`,
// `Out::write_bytes`) is what the transform reads back.
#[test]
fn m12_the_transform_reads_what_the_handler_wrote() {
    let cmd = command(
        "m12",
        "using u64 = unsigned long long;
         consteval void transform() {
           u64 in[1] = {21};
           const char bin[1] = {0};
           u64 out[1] = {0};
           char bout[1] = {0};
           u64 r = __builtin_kiln_host_call(1, in, 1, bin, 0, out, 1, bout, 1);
           if (r != 42) throw 1;
           if (out[0] != 21) throw 2;
           if (bout[0] != 'x') throw 3;
         }",
        &[],
    );
    let r = with(&cmd, |s| {
        let entry = s.entry("transform").unwrap();
        s.run(entry, &mut Counting::default(), BUDGET, &Cancel::new())
    });
    assert_eq!(r.result, Ok(()), "{}", r.rendered);
}

/// Panics on every call.
struct Panicking;

impl Handler for Panicking {
    fn call(&mut self, _: u32, _: &[u64], _: &[u8], _: &mut dyn Out) -> Result<u64, String> {
        panic!("the handler panics")
    }
}

// WP-080 fix round, S2: a handler's panic goes on out of `run`; the
// session is then poisoned, and a later run does not reach Kiln.
#[test]
fn s2_a_handler_panic_poisons_the_session() {
    let cmd =
        transform("s2", "consteval void transform() { host(1, 1); }\nconsteval void fine() {}\n");
    let (panicked, after, fine) = with(&cmd, |s| {
        let entry = s.entry("transform").unwrap();
        let fine = s.entry("fine").unwrap();
        // Deliberate catch: tests that a handler panic poisons the session.
        let panicked = catch_unwind(AssertUnwindSafe(|| {
            s.run(entry, &mut Panicking, BUDGET, &Cancel::new());
        }))
        .map_err(|p| p.downcast_ref::<&str>().map(|m| (*m).to_owned()));
        let after = s.run(entry, &mut Counting::default(), BUDGET, &Cancel::new());
        let fine = s.run(fine, &mut Counting::default(), BUDGET, &Cancel::new());
        (panicked, after, fine)
    });
    assert_eq!(panicked, Err(Some("the handler panics".to_owned())));
    for r in [after, fine] {
        assert_eq!(
            r,
            RunReport {
                result: Err(RunFailure::Poisoned),
                steps: 0,
                host_calls: 0,
                peak_bytes: 0,
                rendered: String::new(),
                diagnostics: Vec::new(),
            }
        );
    }
}

// WP-080 fix round, F7: an entry is refused by any session but the one
// that gave it, before Kiln.
#[test]
fn f7_an_entry_of_another_session_is_refused() {
    let a = transform("f7_a", "consteval void transform() {}\n");
    let b = transform("f7_b", "consteval void transform() {}\n");
    let (own, foreign) = with(&a, |sa| {
        let ea = sa.entry("transform").unwrap();
        let foreign = with(&b, |sb| {
            let eb = sb.entry("transform").unwrap();
            assert_eq!(sb.run(eb, &mut Failing, BUDGET, &Cancel::new()).result, Ok(()));
            sb.run(ea, &mut Failing, BUDGET, &Cancel::new())
        });
        (sa.run(ea, &mut Failing, BUDGET, &Cancel::new()).result, foreign)
    });
    assert_eq!(own, Ok(()));
    assert_eq!((foreign.result, foreign.steps), (Err(RunFailure::ForeignEntry), 0));
}

// WP-080 fix round, S5 (spec/api.md §2.5 rule 4): another thread cancels a
// run under way; a run given the same token is cancelled too; after
// `Cancel::reset` a run completes.
#[test]
fn s5_cancel_from_another_thread() {
    let cmd = transform(
        "s5",
        "consteval void forever() { for (u64 i = 0; i < (1ull << 62); ++i) {} }
         consteval void fine() {}",
    );
    let (during, again, reset) = with(&cmd, |s| {
        let forever = s.entry("forever").unwrap();
        let fine = s.entry("fine").unwrap();
        let cancel = Cancel::new();
        let budget = Budget { steps: u64::MAX, ..BUDGET };
        let during = std::thread::scope(|t| {
            let other = cancel.clone();
            t.spawn(move || {
                std::thread::sleep(Duration::from_millis(200));
                other.cancel();
            });
            s.run(forever, &mut Failing, budget, &cancel)
        });
        let again = s.run(fine, &mut Failing, BUDGET, &cancel);
        cancel.reset();
        let reset = s.run(fine, &mut Failing, BUDGET, &cancel);
        (during, again, reset)
    });
    assert_eq!(during.result, Err(RunFailure::Cancelled), "{}", during.rendered);
    assert!(during.steps > 1000, "cancelled under way: {} steps", during.steps);
    assert_eq!(again.result, Err(RunFailure::Cancelled));
    assert_eq!(reset.result, Ok(()), "{}", reset.rendered);
}

// WP-080 fix round, F1: a transform that may use `#embed` or `__has_embed`,
// in its file or a header it includes, is refused, naming the file and the
// line.
#[test]
fn f1_embed_is_refused() {
    let dir = scratch("f1");
    std::fs::write(dir.join("d.bin"), b"ab").unwrap();
    std::fs::write(dir.join("h.h"), "int h;\n#if __has_embed(\"d.bin\")\n#endif\n").unwrap();
    let cases = [
        (
            "embed",
            "const char d[] = {\n#embed \"d.bin\"\n};\nconsteval void transform() {}\n",
            "embed.cpp",
            2,
        ),
        (
            "dead",
            "#if 0\n#  embed \"d.bin\"\n#endif\nconsteval void transform() {}\n",
            "dead.cpp",
            2,
        ),
        (
            "has",
            "\n#if __has_embed(\"d.bin\")\n#endif\nconsteval void transform() {}\n",
            "has.cpp",
            2,
        ),
        ("header", "#include \"h.h\"\nconsteval void transform() {}\n", "h.h", 2),
        (
            "string",
            "const char* s = \"__has_embed\";\nconsteval void transform() {}\n",
            "string.cpp",
            1,
        ),
    ];
    for (name, text, file, line) in cases {
        let path = dir.join(format!("{name}.cpp"));
        std::fs::write(&path, text).unwrap();
        let cmd = TransformCommand::new(resource_include(), &[], path).unwrap();
        match compile_error(&cmd) {
            CompileError::Embed { path, line: at } => {
                let path = String::from_utf8(path).unwrap();
                assert!(path.ends_with(&format!("/{file}")), "{name}: {path}");
                assert_eq!(at, line, "{name}");
            }
            other => panic!("{name}: {other:?}"),
        }
    }
    let fine = dir.join("fine.cpp");
    std::fs::write(&fine, "int embed = 1; // embed\nconsteval void transform() {}\n").unwrap();
    let cmd = TransformCommand::new(resource_include(), &[], fine).unwrap();
    assert!(with(&cmd, |s| s.entry("transform").is_ok()));
}

// §2.1 rule 6: a macro of the command line is in no file the scan reads,
// so a `-D` whose replacement holds `embed` is refused where the options
// are checked. The macro's name is not looked at, and neither is `-U`.
#[test]
fn f1_embed_in_a_macro_option_is_refused() {
    let new = |author: &[&str]| {
        let author: Vec<String> = author.iter().map(|&a| a.to_owned()).collect();
        TransformCommand::new(resource_include(), &author, PathBuf::from("/t.cpp"))
    };
    let reason = "the macro's replacement holds `embed`: `#embed` and `__has_embed` are not supported in a transform (spec/api.md §2.1)";
    for (bad, word) in [
        (&["-DE=__has_embed"][..], "-DE=__has_embed"),
        (&["-D", "E=__has_embed"], "E=__has_embed"),
        (&["-DE(x)=__has_embed(x)"], "-DE(x)=__has_embed(x)"),
        (&["-DP(a)=a##embed"], "-DP(a)=a##embed"),
        (&["-DX=1", "-D", "Y=__has_##embed"], "Y=__has_##embed"),
        (&["-DE=a=embed"], "-DE=a=embed"),
    ] {
        let e = new(bad).unwrap_err();
        assert_eq!((e.word.as_str(), e.reason), (word, reason), "{bad:?}");
    }
    for fine in [
        &["-Dembed"][..],
        &["-Dembedded=1"],
        &["-D", "has_embed"],
        &["-DE=EMBED"],
        &["-DE=embe d"],
        &["-Uembed"],
        &["-U", "__has_embed"],
    ] {
        assert!(new(fine).is_ok(), "{fine:?}");
    }
}

// WP-080 fix round, S1 (K2): a run is checked against the stack guard of
// the thread that compiled the unit (`kiln_sema/src/templ/mod.rs:617`), so
// a run on another thread that reaches a stack check (an instantiation, a
// nested evaluation) would fail with "stack exhausted", and every later
// run too. The attempt to reach one from an entry: each template the
// entry's body uses below (a member of a class template, one with a
// `static_assert`, a virtual override, a variable template) is
// instantiated when the unit is compiled, so the runs, away and then at
// home, reach no check and complete, with a guard (4 MiB) that a thread 64
// MiB away is far outside. Not reproduced at this Kiln revision; the hazard
// stands (docs/wp/WP-080.md, K2).
#[test]
fn s1_runs_away_from_the_compiling_thread_reach_no_stack_check() {
    let variants = [
        (
            "member",
            "template <class T> struct W { static consteval int run() { return sizeof(T); } };
             consteval void transform() { if (W<long>::run() != 8) throw 0; }",
        ),
        (
            "assert",
            "template <class T> struct W {
               static consteval int run() { static_assert(sizeof(T) == 8); return 1; }
             };
             consteval void transform() { if (W<long>::run() != 1) throw 0; }",
        ),
        (
            "virtual",
            "struct B { virtual consteval int f() const = 0; };
             template <class T> struct D : B {
               consteval int f() const override { return sizeof(T); }
             };
             consteval int call(const B& b) { return b.f(); }
             consteval void transform() { D<long> d; if (call(d) != 8) throw 0; }",
        ),
        (
            "variable",
            "template <class T> constexpr int size = sizeof(T);
             consteval void transform() { if (size<long> != 8) throw 0; }",
        ),
    ];
    let thread = |f: &(dyn Fn() -> (RunReport, RunReport) + Sync)| {
        std::thread::scope(|t| {
            std::thread::Builder::new()
                .stack_size(STACK)
                .spawn_scoped(t, f)
                .unwrap()
                .join()
                .unwrap()
        })
    };
    for (name, body) in variants {
        let cmd = transform(&format!("s1_{name}"), body);
        let (away, home) = thread(&|| {
            with_transform(&cmd, 4 << 20, |opened| {
                let s = &mut opened.unwrap();
                let entry = s.entry("transform").unwrap();
                let away = std::thread::scope(|t| {
                    let s = &mut *s;
                    std::thread::Builder::new()
                        .stack_size(STACK)
                        .spawn_scoped(t, move || s.run(entry, &mut Failing, BUDGET, &Cancel::new()))
                        .unwrap()
                        .join()
                        .unwrap()
                });
                (away, s.run(entry, &mut Failing, BUDGET, &Cancel::new()))
            })
        });
        for (at, r) in [("away", away), ("home", home)] {
            assert_eq!((r.result, r.rendered), (Ok(()), String::new()), "{name}, {at}");
        }
    }
}

/// A Kiln handler that answers every call with 0.
struct NoCalls;

impl HostCalls for NoCalls {
    fn call(&mut self, _: u32, _: &[u64], _: &[u8], _: &mut HostOut<'_>) -> Result<u64, HostError> {
        Ok(0)
    }
}

// WP-080 fix round, K2, at the Kiln level (Slip's entries cannot reach
// it, see `s1_…`): Kiln's own pattern (`kiln_parse/tests/eval_session.rs`
// `instantiation_diagnostics_are_reported_by_every_run`), an entry that is
// an uninstantiated member of a class template, so the first run
// instantiates it. Compiled under a 4 MiB guard, run first on another
// thread: the instantiation was checked against the compiling thread's
// guard and failed with "stack exhausted", sticky for the next run at
// home. Since Kiln WP-2410 (the guard of the running thread) both runs
// complete.
#[test]
fn k2_a_kiln_run_instantiating_away_from_the_compiling_thread_completes() {
    let dir = scratch("k2");
    let file = dir.join("k2.cpp");
    std::fs::write(
        &file,
        "template <class T> struct W { static consteval int run() { return sizeof(T); } };\n\
         static_assert(sizeof(W<int>) > 0);\n\
         using X = W<int>;\n",
    )
    .unwrap();
    let line: Vec<String> =
        ["-std=c++26", "-fsyntax-only", file.to_str().unwrap()].map(str::to_owned).into();
    let budget = kiln_sema::session::RunBudget {
        steps: 1 << 32,
        bytes: 256 << 20,
        depth: 4096,
        ..kiln_sema::session::RunBudget::PROGRAM
    };
    let shown =
        |r: &kiln_sema::session::RunReport| (r.result.is_ok(), format!("{:?}", r.diagnostics));
    let (away, home) = std::thread::scope(|t| {
        std::thread::Builder::new()
            .stack_size(STACK)
            .spawn_scoped(t, || {
                kiln_sema::stack_guard::StackGuard::install(4 << 20);
                let io = kiln_driver::Io {
                    stdout: Box::new(std::io::sink()),
                    stderr: kiln_driver::SharedWriter::new(kiln_diag::SharedBuffer::new()),
                    stdin: Box::new(std::io::empty()),
                };
                kiln_driver::session::with_eval_session("kiln", &line, io, |opened| {
                    let mut fe = opened.unwrap_or_else(|e| panic!("{e:?}"));
                    let entry = fe.session.entry_named("X::run").unwrap();
                    let cancel = kiln_sema::session::CancelToken::new();
                    let session = &mut fe.session;
                    let away = std::thread::scope(|t| {
                        std::thread::Builder::new()
                            .stack_size(STACK)
                            .spawn_scoped(t, || {
                                shown(&session.run(entry, &mut NoCalls, budget, &cancel))
                            })
                            .unwrap()
                            .join()
                            .unwrap()
                    });
                    (away, shown(&fe.session.run(entry, &mut NoCalls, budget, &cancel)))
                })
            })
            .unwrap()
            .join()
            .unwrap()
    });
    for (at, (ok, diagnostics)) in [("away", away), ("home", home)] {
        assert!(ok && !diagnostics.contains("stack exhausted"), "{at}: {diagnostics}");
    }
}

// WP-089 (spec/api.md §2.1 rule 3, §2.2 the target term): the session
// hands out the target's triple and the predefines buffer, which holds
// the target's predefined macros and then the command line's `-D` and
// `-U`; it is not among the files read.
#[test]
fn wp089_the_target_of_the_unit() {
    let cmd = command(
        "target",
        "#include <slip/meta.h>\nconsteval void transform() {}\n",
        &["-DWP089=7", "-UWP089_OFF"],
    );
    let (triple, predefines, files) =
        with(&cmd, |s| (s.triple().to_owned(), s.predefines().to_vec(), s.files().to_vec()));
    assert_eq!(triple, format!("{}-unknown-linux-gnu", std::env::consts::ARCH));
    let text = String::from_utf8(predefines).unwrap();
    let arch_macro = match std::env::consts::ARCH {
        "x86_64" => "#define __x86_64__ 1",
        "aarch64" => "#define __aarch64__ 1",
        other => panic!("no Kiln target for {other}"),
    };
    assert!(text.contains(arch_macro), "{text}");
    assert!(text.contains("#define __cplusplus "), "{text}");
    let define = text.find("#define WP089 7\n").expect("the -D line");
    let undef = text.find("#undef WP089_OFF\n").expect("the -U line");
    assert!(text.find(arch_macro).unwrap() < define && define < undef, "{text}");
    assert!(files.iter().all(|f| f.path != b"<built-in>"), "a virtual buffer is not a file read");
}

// WP-089 (`Session::stack_exhausted`): the flag reads false after runs at
// home and away that reach no stack check. That a run sets it (Kiln's
// `RunError::StackExhausted`, kept set by the host since Kiln WP-2410
// puts Kiln's flag back after each run): `slip_script` `driver::tests::
// a_tripped_guard_is_seen_before_a_run_and_retires`.
#[test]
fn wp089_stack_exhausted_reads_false_after_ordinary_runs() {
    let cmd = transform("wp089_stack", "consteval void transform() { host(1, 1); }");
    let flags = std::thread::scope(|t| {
        std::thread::Builder::new()
            .stack_size(STACK)
            .spawn_scoped(t, || {
                with_transform(&cmd, STACK - (1 << 20), |opened| {
                    let s = &mut opened.unwrap();
                    let before = s.stack_exhausted();
                    let entry = s.entry("transform").unwrap();
                    let home = s.run(entry, &mut Counting::default(), BUDGET, &Cancel::new());
                    let away = std::thread::scope(|t| {
                        let s = &mut *s;
                        std::thread::Builder::new()
                            .stack_size(STACK)
                            .spawn_scoped(t, move || {
                                s.run(entry, &mut Counting::default(), BUDGET, &Cancel::new())
                            })
                            .unwrap()
                            .join()
                            .unwrap()
                    });
                    (before, home.result, away.result, s.stack_exhausted())
                })
            })
            .unwrap()
            .join()
            .unwrap()
    });
    assert_eq!(flags, (false, Ok(()), Ok(()), false));
}

/// KN1 of WP-089: the smallest transform that instantiates at run time,
/// through reflection (`std::meta::substitute`, then `size_of`).
const KN1: &str = "#include \"kiln_meta.h\"\n\
                   template <class T> struct box { T v; };\n\
                   consteval void transform() {\n\
                     (void)std::meta::size_of(std::meta::substitute(^^box, {^^long}));\n\
                   }\n";

fn kiln_support() -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../kiln/tests/support");
    std::fs::canonicalize(p).unwrap().to_str().unwrap().to_owned()
}

/// KN1's runs on a thread of `STACK` bytes whose guard is `guard`: first
/// on another thread (or at home, if `away` is false), then at home; each
/// run's result, whether its diagnostics say "stack exhausted", and the
/// flag after both.
fn kn1(away: bool, guard: usize) -> ((bool, bool), (bool, bool), bool) {
    let support = kiln_support();
    let cmd = command(&format!("kn1_{away}_{guard}"), KN1, &["-I", &support]);
    let shown = |r: &RunReport| (r.result.is_ok(), r.rendered.contains("stack exhausted"));
    std::thread::scope(|t| {
        std::thread::Builder::new()
            .stack_size(STACK)
            .spawn_scoped(t, || {
                with_transform(&cmd, guard, |opened| {
                    let s = &mut opened.unwrap_or_else(|e| panic!("{e:?}"));
                    let entry = s.entry("transform").unwrap();
                    let first = if away {
                        std::thread::scope(|t| {
                            let s = &mut *s;
                            std::thread::Builder::new()
                                .stack_size(STACK)
                                .spawn_scoped(t, move || {
                                    shown(&s.run(entry, &mut NoCalls2, BUDGET, &Cancel::new()))
                                })
                                .unwrap()
                                .join()
                                .unwrap()
                        })
                    } else {
                        shown(&s.run(entry, &mut NoCalls2, BUDGET, &Cancel::new()))
                    };
                    let home = shown(&s.run(entry, &mut NoCalls2, BUDGET, &Cancel::new()));
                    (first, home, s.stack_exhausted())
                })
            })
            .unwrap()
            .join()
            .unwrap()
    })
}

/// A handler that answers every call with 0.
struct NoCalls2;

impl Handler for NoCalls2 {
    fn call(&mut self, _: u32, _: &[u64], _: &[u8], _: &mut dyn Out) -> Result<u64, String> {
        Ok(0)
    }
}

// KN1 of WP-089 (H1 a of its review): an instantiation made at run time
// through reflection, on another thread than the compiling one, was
// checked against the compiling thread's guard and tripped it, sticky for
// the next run at home. Since Kiln WP-2410 (the guard of the running
// thread) both runs complete, away then at home, and the flag stays
// clear. `slip_script::driver`'s rule that a transform is run only on the
// thread that loaded it (spec/api.md §2.5 rule 5) is kept: relaxing it is
// "For WP-138" in `spec/landed/edits/WP-066.md`.
#[test]
fn kn1_an_instantiation_at_run_time_away_completes() {
    let guard = STACK - (1 << 20);
    assert_eq!(kn1(false, guard), ((true, false), (true, false), false), "at home");
    assert_eq!(kn1(true, guard), ((true, false), (true, false), false), "away, then home");
}

// WP-163 fix round 1 (F3): a transform's compile is refused while the
// process environment turns on a Kiln check Slip does not model
// (`slip_kiln::check::kiln_switch_in_environment`), as a parse is
// (`tests/parse.rs` `wp163_kiln_switch_in_environment_is_refused`). In a
// child: this binary again with the variable set and only this test
// selected.
#[test]
fn wp163_kiln_switch_in_environment_refuses_the_compile() {
    const CHILD: &str = "SLIP_KILN_SESSION_SWITCH_CHILD";
    if let Some(var) = std::env::var_os(CHILD) {
        let var = var.to_str().unwrap().to_owned();
        let cmd = transform("wp163_switch", "consteval void transform() {}");
        let got = on_stack(&cmd, |opened| match opened {
            Err(CompileError::CommandLine(m)) => Some(m),
            _ => None,
        });
        assert!(got.as_ref().is_some_and(|m| m.contains(&var)), "{got:?}");
        return;
    }
    for var in ["KILN_VERIFY_SUBST_FAILED", "KILN_FN_NAMES_VERIFY"] {
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["wp163_kiln_switch_in_environment_refuses_the_compile", "--exact"])
            .args(["--test-threads=1", "--nocapture"])
            .env(CHILD, var)
            .env(var, "1")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            out.status.success() && stdout.contains("1 passed"),
            "{var}: the child failed:\n{stdout}\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
