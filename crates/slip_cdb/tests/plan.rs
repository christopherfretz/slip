//! `spec/cdb.md` §7: building the plan. The accounting invariant, merging
//! equal commands within and across configurations, plan order,
//! `cost_hint`, `Usage` and `Database` errors, determinism across thread
//! counts, and golden JSON plans (`spec/model.md` §10).

mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use common::TempDir;
use slip_cdb::check::{ByExtension, FrontEndCheck, Rejection};
use slip_cdb::plan::{PlanError, READ_CHUNK, Source, Tool, build};
use slip_model::hash::content_digest;
use slip_model::json::{Value, parse, write_pretty};
use slip_model::{Command, Language, Plan, Reason};

fn tool() -> Tool {
    Tool { slip: "0.0.0".into(), kiln: "1.3.0".into() }
}

/// `ByExtension`, except that any command holding `-fbad` is refused with
/// `UnsupportedFlag("-fbad")`.
struct RejectsFbad;

impl FrontEndCheck for RejectsFbad {
    fn check(&self, command: &Command, main: &[u8]) -> Result<Language, Rejection> {
        if command.argv.iter().any(|a| a == b"-fbad") {
            return Err(Rejection::UnsupportedFlag("-fbad".into()));
        }
        ByExtension.check(command, main)
    }
}

/// A fixture tree whose root is its real path, so that the paths written
/// into databases are already real.
struct Tree {
    tmp: TempDir,
    root: String,
}

impl Tree {
    fn new(tag: &str) -> Tree {
        let tmp = TempDir::new(tag);
        let root = std::fs::canonicalize(tmp.path()).unwrap().to_str().unwrap().to_owned();
        Tree { tmp, root }
    }

    /// Writes a file; `$ROOT` in `contents` is replaced by the root.
    fn write(&self, relative: &str, contents: &str) -> PathBuf {
        self.tmp.write(relative, contents.replace("$ROOT", &self.root))
    }

    /// A file of `size` bytes.
    fn sized(&self, relative: &str, size: usize) -> PathBuf {
        let text: String = (0..size).map(|i| (b'a' + (i % 26) as u8) as char).collect();
        self.tmp.write(relative, text)
    }

    fn abs(&self, relative: &str) -> String {
        format!("{}/{relative}", self.root)
    }

    fn source(&self, name: &str, relative: &str) -> Source {
        Source { name: name.into(), path: PathBuf::from(self.abs(relative)) }
    }
}

/// A database from (directory, file, arguments) triples, `$ROOT` replaced.
fn database(entries: &[(&str, &str, &[&str])]) -> String {
    let render = |s: &str| Value::Str(s.into());
    let array = Value::Array(
        entries
            .iter()
            .map(|(directory, file, arguments)| {
                Value::Object(vec![
                    ("directory".into(), render(directory)),
                    ("file".into(), render(file)),
                    (
                        "arguments".into(),
                        Value::Array(arguments.iter().map(|a| render(a)).collect()),
                    ),
                ])
            })
            .collect(),
    );
    write_pretty(&array).unwrap()
}

fn plan_of(sources: &[Source], check: &dyn FrontEndCheck) -> Plan {
    let plan = build(sources, check, &tool(), 1).unwrap_or_else(|e| panic!("build failed: {e}"));
    plan.validate().expect("the plan validates");
    plan
}

/// The accounting invariant (`spec/model.md` §7): every (configuration,
/// entry) pair appears exactly once, in some origin or in `excluded`.
fn assert_accounted(plan: &Plan, entries_per_database: &[u64]) {
    let mut seen = BTreeMap::new();
    for o in &plan.observers {
        for origin in &o.origins {
            *seen.entry((origin.configuration, origin.entry)).or_insert(0) += 1;
        }
    }
    for x in &plan.excluded {
        *seen.entry((x.configuration, x.entry)).or_insert(0) += 1;
    }
    let mut expected = BTreeMap::new();
    for (c, &n) in entries_per_database.iter().enumerate() {
        for e in 0..n {
            expected.insert((c as u64, e), 1);
        }
    }
    assert_eq!(seen, expected, "every pair exactly once");
}

fn reasons(plan: &Plan) -> Vec<(u64, u64, Reason)> {
    plan.excluded.iter().map(|x| (x.configuration, x.entry, x.reason)).collect()
}

// ---------------------------------------------------------------------------
// The mixed fixture, also used for the golden plan.
// ---------------------------------------------------------------------------

/// Two configurations whose entries cover an observer of each language
/// kind, every exclusion reason, and merging within and across databases.
/// Every observer's main file has a distinct size, so the plan order does
/// not depend on ids (which depend on where the tree is).
fn mixed() -> (Tree, Vec<Source>) {
    let t = Tree::new("plan-mixed");
    t.sized("src/big.cpp", 300);
    t.sized("src/mid.c", 200);
    t.sized("src/small.cc", 100);
    t.sized("src/f.f90", 10);
    t.sized("src/dup.cpp", 10);
    t.sized("src/a.cpp", 10);
    std::fs::create_dir_all(t.abs("build/debug")).unwrap();
    std::fs::create_dir_all(t.abs("build/release")).unwrap();
    let debug = "$ROOT/build/debug";
    t.write(
        "build/debug/compile_commands.json",
        &database(&[
            (debug, "../../src/big.cpp", &["clang++", "-c", "../../src/big.cpp", "-o", "big.o"]),
            (debug, "$ROOT/src/mid.c", &["gcc", "-std=c11", "-c", "$ROOT/src/mid.c"]),
            (debug, "app", &["clang++", "big.o", "-o", "app"]),
            (debug, "../../src/f.f90", &["gfortran", "-c", "../../src/f.f90"]),
            (debug, "$ROOT/src/small.cc", &["clang++", "-fbad", "-c", "$ROOT/src/small.cc"]),
            (debug, "../../src/gone.cpp", &["clang++", "-c", "../../src/gone.cpp"]),
            (debug, "x.cpp", &["cl.exe", "/c", "x.cpp"]),
            (
                debug,
                "../../src/big.cpp",
                &["ccache", "clang++", "-c", "../../src/big.cpp", "-o", "other.o", "-MD"],
            ),
        ])
        .replacen("[\n", "[\n  42,\n", 1),
    );
    t.write(
        "build/release/compile_commands.json",
        &format!(
            r#"[
  {{ "directory": "{debug}", "file": "../../src/big.cpp", "command": "clang++ -c ../../src/big.cpp" }},
  {{ "directory": "$ROOT/build/release", "file": "$ROOT/src/small.cc",
     "command": "clang++ -O2 -c $ROOT/src/small.cc -o small.o" }},
  {{ "directory": "$ROOT/build/release", "file": "../../src/dup.cpp",
     "arguments": ["clang++", "-c", "../../src/dup.cpp", "../../src/a.cpp"] }}
]
"#
        ),
    );
    let sources = vec![
        t.source("debug", "build/debug/compile_commands.json"),
        t.source("release", "build/release/compile_commands.json"),
    ];
    (t, sources)
}

#[test]
fn s7_accounting_invariant_with_every_reason() {
    let (_t, sources) = mixed();
    let plan = plan_of(&sources, &RejectsFbad);
    assert_accounted(&plan, &[9, 3]);
    // Debug entry 0 is the inserted `42`; the rest shift by one.
    assert_eq!(
        reasons(&plan),
        vec![
            (0, 0, Reason::MalformedEntry),
            (0, 3, Reason::NotACompile),
            (0, 4, Reason::UnsupportedLanguage),
            (0, 5, Reason::UnsupportedFlag),
            (0, 6, Reason::MissingFile),
            (0, 7, Reason::UnsupportedCompiler),
            (1, 2, Reason::MalformedEntry),
        ]
    );
    let flag = plan.excluded.iter().find(|x| x.reason == Reason::UnsupportedFlag).unwrap();
    assert_eq!(flag.detail, "-fbad", "the rejection's text is the detail");
    let language = plan.excluded.iter().find(|x| x.reason == Reason::UnsupportedLanguage).unwrap();
    assert_eq!(language.detail, ".f90");
    assert_eq!(language.file, b"../../src/f.f90", "file as spelled");
    assert_eq!(plan.excluded[0].file, b"", "a defect with no file");
    assert!(plan.excluded[6].detail.contains("several inputs"));
    assert_eq!(plan.observers.len(), 3);
}

#[test]
fn s7_equal_commands_merge_within_and_across_configurations() {
    let (t, sources) = mixed();
    let plan = plan_of(&sources, &RejectsFbad);
    let big = &plan.observers[0];
    assert_eq!(big.main.path, t.abs("src/big.cpp").into_bytes());
    let origins: Vec<(u64, u64)> = big.origins.iter().map(|o| (o.configuration, o.entry)).collect();
    // Debug entries 1 and 8 (launcher, -o, -MD removed), release entry 0.
    assert_eq!(origins, vec![(0, 1), (0, 8), (1, 0)]);
    assert_eq!(big.command.argv, vec![b"clang++".to_vec(), b"../../src/big.cpp".to_vec()]);
}

#[test]
fn s7_configurations_and_tool_recorded() {
    let (t, sources) = mixed();
    let plan = plan_of(&sources, &RejectsFbad);
    assert_eq!((plan.tool_slip.as_str(), plan.tool_kiln.as_str()), ("0.0.0", "1.3.0"));
    assert_eq!(plan.configurations.len(), 2);
    for (configuration, source) in plan.configurations.iter().zip(&sources) {
        let bytes = std::fs::read(&source.path).unwrap();
        assert_eq!(configuration.name, source.name);
        assert_eq!(configuration.database.path, source.path.to_str().unwrap().as_bytes());
        assert_eq!(configuration.database.size, bytes.len() as u64);
        assert_eq!(configuration.database.content, content_digest(&bytes));
    }
    drop(t);
}

#[test]
fn s7_plan_order_and_cost_hint() {
    let (_t, sources) = mixed();
    let plan = plan_of(&sources, &RejectsFbad);
    let sizes: Vec<u64> = plan.observers.iter().map(|o| o.cost_hint).collect();
    assert_eq!(sizes, vec![300, 200, 100], "largest first");
    for o in &plan.observers {
        assert_eq!(o.cost_hint, o.main.size, "cost_hint is the main file's size");
        let bytes = std::fs::read(String::from_utf8(o.main.path.clone()).unwrap()).unwrap();
        assert_eq!(o.main.content, content_digest(&bytes));
        assert_eq!(o.id, o.command.observer_id());
    }
    assert_eq!(plan.observers[1].language, Language::C);
    assert_eq!(plan.observers[0].language, Language::Cxx);
}

#[test]
fn s7_ties_in_cost_are_ordered_by_id() {
    let t = Tree::new("plan-ties");
    for name in ["a", "b", "c", "d", "e", "f"] {
        t.sized(&format!("{name}.cc"), 64);
    }
    t.write(
        "cdb.json",
        &database(&[
            ("$ROOT", "a.cc", &["c++", "a.cc"]),
            ("$ROOT", "b.cc", &["c++", "b.cc"]),
            ("$ROOT", "c.cc", &["c++", "c.cc"]),
            ("$ROOT", "d.cc", &["c++", "d.cc"]),
            ("$ROOT", "e.cc", &["c++", "e.cc"]),
            ("$ROOT", "f.cc", &["c++", "f.cc"]),
        ]),
    );
    let plan = plan_of(&[t.source("x", "cdb.json")], &ByExtension);
    let ids: Vec<_> = plan.observers.iter().map(|o| o.id).collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted, "equal cost_hint: id ascending");
}

#[test]
fn s7_same_file_different_flags_is_two_observers() {
    let t = Tree::new("plan-flags");
    t.sized("m.cc", 77);
    // `alias.cc` is a second spelling of the same real file.
    std::os::unix::fs::symlink(t.abs("m.cc"), t.abs("alias.cc")).unwrap();
    t.write(
        "cdb.json",
        &database(&[
            ("$ROOT", "m.cc", &["c++", "-DA", "-c", "m.cc"]),
            ("$ROOT", "m.cc", &["c++", "-DB", "-c", "m.cc"]),
            ("$ROOT", "alias.cc", &["c++", "-DA", "-c", "alias.cc"]),
        ]),
    );
    let plan = plan_of(&[t.source("x", "cdb.json")], &ByExtension);
    assert_eq!(plan.observers.len(), 3, "different commands are different observers");
    let real = t.abs("m.cc").into_bytes();
    for o in &plan.observers {
        assert_eq!(o.main.path, real, "identified by real path");
        assert_eq!(o.main.size, 77);
        assert_eq!(o.origins.len(), 1);
    }
    assert_eq!(plan.observers[0].main, plan.observers[1].main);
}

#[test]
fn s7_main_file_that_cannot_be_read_is_missing_file_for_every_origin() {
    let t = Tree::new("plan-unreadable");
    // A directory: it exists and has a real path, but reading it fails.
    std::fs::create_dir_all(t.abs("dir.cc")).unwrap();
    t.sized("ok.cc", 5);
    t.write(
        "a.json",
        &database(&[
            ("$ROOT", "dir.cc", &["c++", "-c", "dir.cc"]),
            ("$ROOT", "ok.cc", &["c++", "ok.cc"]),
        ]),
    );
    t.write("b.json", &database(&[("$ROOT", "./dir.cc", &["c++", "dir.cc", "-o", "x.o"])]));
    let plan = plan_of(&[t.source("a", "a.json"), t.source("b", "b.json")], &ByExtension);
    assert_accounted(&plan, &[2, 1]);
    assert_eq!(reasons(&plan), vec![(0, 0, Reason::MissingFile), (1, 0, Reason::MissingFile)]);
    assert_eq!(plan.excluded[1].file, b"./dir.cc", "each origin keeps its own file");
    assert!(plan.excluded[0].detail.contains("cannot be read"), "{}", plan.excluded[0].detail);
    assert_eq!(plan.observers.len(), 1);
}

#[test]
fn s7_main_file_hashed_in_pieces() {
    let t = Tree::new("plan-large");
    let size = 3 * READ_CHUNK + 17;
    let path = t.sized("large.cc", size);
    t.write("cdb.json", &database(&[("$ROOT", "large.cc", &["c++", "large.cc"])]));
    let plan = plan_of(&[t.source("x", "cdb.json")], &ByExtension);
    let main = &plan.observers[0].main;
    assert_eq!(main.size, size as u64);
    assert_eq!(main.content, content_digest(&std::fs::read(path).unwrap()));
}

#[test]
fn s7_empty_database_is_an_empty_plan() {
    let t = Tree::new("plan-empty");
    t.write("cdb.json", "[]");
    let plan = plan_of(&[t.source("x", "cdb.json")], &ByExtension);
    assert!(plan.observers.is_empty() && plan.excluded.is_empty());
    assert_eq!(plan.configurations.len(), 1);
}

// ---------------------------------------------------------------------------
// Errors.
// ---------------------------------------------------------------------------

fn usage(sources: &[Source]) -> String {
    match build(sources, &ByExtension, &tool(), 1) {
        Err(PlanError::Usage(detail)) => detail,
        other => panic!("expected Usage, got {other:?}"),
    }
}

#[test]
fn s7_duplicate_configuration_name_is_usage() {
    let t = Tree::new("plan-usage");
    t.write("a.json", "[]");
    t.write("b.json", "[]");
    let detail =
        usage(&[t.source("x", "a.json"), t.source("y", "b.json"), t.source("x", "b.json")]);
    assert!(detail.contains("`x`"), "{detail}");
}

#[test]
fn s7_empty_configuration_name_is_usage() {
    let t = Tree::new("plan-usage");
    t.write("a.json", "[]");
    usage(&[t.source("", "a.json")]);
}

#[test]
fn s7_no_sources_is_usage() {
    usage(&[]);
}

#[test]
fn s7_usage_is_found_before_any_database_is_read() {
    // Neither database exists; the name is still the error.
    let t = Tree::new("plan-usage");
    usage(&[t.source("x", "none.json"), t.source("x", "none.json")]);
}

fn database_error(t: &Tree, contents: Option<&str>) -> (PathBuf, String) {
    t.write("good.json", "[]");
    if let Some(contents) = contents {
        t.write("bad.json", contents);
    }
    let sources = [t.source("good", "good.json"), t.source("bad", "bad.json")];
    match build(&sources, &ByExtension, &tool(), 1) {
        Err(PlanError::Database { path, detail }) => (path, detail),
        other => panic!("expected Database, got {other:?}"),
    }
}

#[test]
fn s7_database_that_cannot_be_read_is_database_error() {
    let t = Tree::new("plan-db");
    let (path, detail) = database_error(&t, None);
    assert_eq!(path, Path::new(&t.abs("bad.json")));
    assert!(detail.contains("cannot be read"), "{detail}");
}

#[test]
fn s7_database_not_json_is_database_error() {
    let t = Tree::new("plan-db");
    let (path, detail) = database_error(&t, Some("[1,]"));
    assert_eq!(path, Path::new(&t.abs("bad.json")));
    assert!(detail.starts_with("not JSON"), "{detail}");
}

#[test]
fn s7_database_not_an_array_is_database_error() {
    let t = Tree::new("plan-db");
    let (_, detail) = database_error(&t, Some("{}"));
    assert_eq!(detail, "the top level is an object, not an array");
}

// ---------------------------------------------------------------------------
// Determinism.
// ---------------------------------------------------------------------------

#[test]
fn s7_same_plan_bytes_for_every_thread_count() {
    let t = Tree::new("plan-threads");
    let mut entries = Vec::new();
    let mut names = Vec::new();
    for i in 0..40 {
        // Sizes repeat, so the id tie-break is exercised too.
        t.sized(&format!("f{i}.cc"), 10 + (i % 7) * 1000);
        names.push(format!("f{i}.cc"));
    }
    for (i, name) in names.iter().enumerate() {
        entries.push(("$ROOT", name.as_str(), vec!["c++", "-c", name.as_str()]));
        if i % 3 == 0 {
            // A second command for the same file: read once, two observers.
            entries.push(("$ROOT", name.as_str(), vec!["c++", "-O2", name.as_str()]));
        }
    }
    entries.push(("$ROOT", "gone.cc", vec!["c++", "gone.cc"]));
    let entries: Vec<(&str, &str, &[&str])> =
        entries.iter().map(|(d, f, a)| (*d, *f, a.as_slice())).collect();
    t.write("a.json", &database(&entries));
    t.write("b.json", &database(&entries[5..20]));
    let sources = [t.source("a", "a.json"), t.source("b", "b.json")];
    let reference = build(&sources, &ByExtension, &tool(), 1).unwrap().to_bytes();
    for threads in [0, 2, 8, 64] {
        let bytes = build(&sources, &ByExtension, &tool(), threads).unwrap().to_bytes();
        assert_eq!(bytes, reference, "threads = {threads}");
    }
    let plan = Plan::from_bytes(&reference).expect("the plan reads back");
    assert_eq!(plan.observers.len(), 40 + 14);
    assert_accounted(&plan, &[entries.len() as u64, 15]);
}

// ---------------------------------------------------------------------------
// Golden JSON plans (spec/model.md §10).
// ---------------------------------------------------------------------------

/// Renders a plan for comparison with a golden file. What depends on where
/// the fixture tree is gets replaced:
///
/// - every observer `id` by the string `"<observer id>"`, after checking it
///   equals `Command::observer_id` of the observer's command (the command's
///   `directory` holds the root);
/// - every configuration's database `size` and `content` by `"<size>"` and
///   `"<digest>"` (the database text holds the root);
/// - then, in the text, every occurrence of the tree's real path by `$ROOT`.
///
/// Main files' sizes and digests do not depend on the root and are kept.
fn golden_text(plan: &Plan, root: &str) -> String {
    for o in &plan.observers {
        assert_eq!(o.id, o.command.observer_id(), "observer id");
    }
    let mut value = plan.to_json();
    let sections = member(&mut value, "sections");
    if let Value::Array(observers) = member(sections, "observers") {
        for o in observers {
            *member(o, "id") = Value::Str("<observer id>".into());
        }
    }
    if let Value::Array(configurations) = member(sections, "configurations") {
        for c in configurations {
            let database = member(c, "database");
            *member(database, "size") = Value::Str("<size>".into());
            *member(database, "content") = Value::Str("<digest>".into());
        }
    }
    write_pretty(&value).unwrap().replace(root, "$ROOT")
}

fn member<'v>(value: &'v mut Value, key: &str) -> &'v mut Value {
    match value {
        Value::Object(members) => {
            &mut members
                .iter_mut()
                .find(|(k, _)| k == key)
                .unwrap_or_else(|| panic!("no `{key}`"))
                .1
        }
        _ => panic!("not an object looking for `{key}`"),
    }
}

fn golden(name: &str, actual: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden").join(name);
    if std::env::var_os("SLIP_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, actual).unwrap();
        eprintln!("blessed {}", path.display());
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}; run with SLIP_BLESS=1 to create", path.display()));
    assert_eq!(actual, expected, "{name} differs from its golden; SLIP_BLESS=1 rewrites it");
}

#[test]
fn s7_golden_mixed_plan() {
    let (t, sources) = mixed();
    let plan = plan_of(&sources, &RejectsFbad);
    // The JSON form reads back to the same plan.
    assert_eq!(
        Plan::from_json(&parse(write_pretty(&plan.to_json()).unwrap().as_bytes()).unwrap())
            .unwrap(),
        plan
    );
    golden("plan_mixed.json", &golden_text(&plan, &t.root));
}

#[test]
fn s7_golden_merged_plan() {
    // One observer from three entries in two configurations, and one from a
    // `command` string.
    let t = Tree::new("plan-golden-merged");
    t.write("src/lib.cpp", "int f() { return 1; }\n");
    t.write("src/util.c", "int g(void) { return 2; }\n");
    t.write(
        "one.json",
        &database(&[
            (
                "$ROOT/out",
                "../src/lib.cpp",
                &["sccache", "clang++", "-std=c++17", "-c", "../src/lib.cpp", "-o", "lib.o"],
            ),
            (
                "$ROOT/out",
                "$ROOT/src/lib.cpp",
                &["clang++", "-std=c++17", "-MMD", "-MF", "lib.d", "../src/lib.cpp"],
            ),
        ]),
    );
    t.write(
        "two.json",
        r#"[
  { "directory": "$ROOT/out", "file": "../src/lib.cpp", "command": "clang++ -std=c++17 -c ../src/lib.cpp" },
  { "directory": "$ROOT/out", "file": "../src/util.c", "command": "cc -I'../inc dir' -c ../src/util.c", "output": "util.o" }
]
"#,
    );
    std::fs::create_dir_all(t.abs("out")).unwrap();
    let plan = plan_of(&[t.source("one", "one.json"), t.source("two", "two.json")], &ByExtension);
    assert_eq!(plan.observers.len(), 2);
    assert!(plan.excluded.is_empty());
    golden("plan_merged.json", &golden_text(&plan, &t.root));
}
