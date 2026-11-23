//! The rules of the script test runner (`spec/api.md` §11,
//! `docs/wp/WP-090.md`, `spec/landed/api/WP-090.md`): the header lines,
//! the rendering, the judgement of a rendering against `expect.txt`,
//! blessing, discovery, grouping and retirement; then a suite over a
//! fixture tree that compiles real transforms: an `xfail` that passes
//! fails it, a directory that cannot be loaded and a missing `expect.txt`
//! are errors, a transform that reads a file of its directory is an
//! error, two directories with one transform are one group.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use slip_model::{Counter, Digest, FileRef};
use slip_script::driver::{self, Budgets, Failure, FailureKind, LoadError};
use slip_script::output::{Changed, Piece, Place, Report, RunOutput, Severity, Tiers};
use slip_script::script_test::{
    self, Case, GroupRun, Ran, Script, SuiteOptions, Verdict, judge, load_script, subject_header,
    transform_header,
};

// ---------------------------------------------------------------------------
// Header lines (§11 rules 1 and 2, as spec/harness.md §6.1).

fn header_err(text: &str) -> String {
    transform_header(text).expect_err(text)
}

#[test]
fn s11_rule2_each_header_line() {
    let h = transform_header(
        "// spec: §5.1 rule 2\n// args: -DA=1  -I/x\n// steps: 12\n// bytes: 34\n// depth: 5\n\
         // owned: no\n// xfail: a reason: with a colon\n// counters: api.edits, api.op.lookup\n\
         #include <slip/meta.h>\n// spec: is a comment here: not a header line? it is\n",
    );
    // The last line is an exact header line after the block: an error.
    assert_eq!(h, Err("line 10: a header line outside the header block".to_owned()));
    let h = transform_header(
        "// spec: §5.1 rule 2\n// args: -DA=1  -I/x\n// steps: 12\n// bytes: 34\n// depth: 5\n\
         // owned: no\n// xfail: a reason: with a colon\n// counters: api.edits, api.op.lookup\n\
         #include <slip/meta.h>\n",
    )
    .unwrap();
    assert_eq!(h.spec, "§5.1 rule 2");
    assert_eq!(h.args, ["-DA=1", "-I/x"]);
    assert_eq!(h.budgets, Budgets { steps: 12, bytes: 34, depth: 5 });
    assert!(!h.owned);
    assert_eq!(h.xfail.as_deref(), Some("a reason: with a colon"));
    assert_eq!(h.counters, ["api.edits", "api.op.lookup"]);
    let d = transform_header("// spec: §11\nint x;\n").unwrap();
    assert_eq!(d.budgets, Budgets::default());
    assert!(d.owned && d.xfail.is_none() && d.args.is_empty() && d.counters.is_empty());
}

#[test]
fn s11_rule2_spec_is_required_and_has_a_form() {
    assert!(header_err("int x;\n").starts_with("no `// spec: §N.M` line"));
    assert!(header_err("// args: -DX\n").starts_with("no `// spec: §N.M` line"));
    for bad in ["11", "§", "§1.", "§.1", "§01", "§1.x", "§ 1"] {
        let e = header_err(&format!("// spec: {bad}\n"));
        assert!(e.contains("does not name a section"), "{bad}: {e}");
    }
    for good in ["§1", "§11", "§2.10", "§5.1 rule 2", "§10.1.3"] {
        assert!(transform_header(&format!("// spec: {good}\n")).is_ok(), "{good}");
    }
    // Blanks at the end of a line are not part of its value.
    assert_eq!(transform_header("// spec: §1 \t\n").unwrap().spec, "§1");
}

#[test]
fn s11_rule2_a_line_that_looks_like_a_header_line_and_is_not() {
    for (text, line) in [
        ("// spec: §11\n// arg: -DX\n", 2),
        ("//spec: §11\n", 1),
        ("// spec:§11\n", 1),
        ("// spec:  §11\n", 1),
        ("// spec: §11\n// owned: yes\n", 2),
        ("// spec: §11\n// owned:\n", 2),
        ("// Note: a comment\n// spec: §11\n", 1),
        ("// spec: §11\n// std: c++20\n", 2),
        ("// spec: §11\n//  depth: 3\n", 2),
        ("// spec: §11\n// args:  -DX\n", 2),
        ("// spec: §11\n// xfail:  why\n", 2),
    ] {
        let e = header_err(text);
        assert!(e.starts_with(&format!("line {line}: ")), "{text:?}: {e}");
    }
    assert_eq!(header_err("// spec: §11\n// spec: §12\n"), "line 2: a second `// spec:` line");
    for (text, what) in [
        ("// spec: §11\n// steps: 1e3\n", "not a decimal number"),
        ("// spec: §11\n// steps: -1\n", "not a decimal number"),
        ("// spec: §11\n// steps: +5\n", "not a decimal number"),
        ("// spec: §11\n// depth: 4294967296\n", "not a decimal number"),
        ("// spec: §11\n// bytes: 18446744073709551616\n", "not a decimal number"),
        ("// spec: §11\n// counters: api.edits,\n", "is not a counter's name"),
        ("// spec: §11\n// counters: api edits\n", "is not a counter's name"),
        ("// spec: §11\n// counters: Api\n", "is not a counter's name"),
        ("// spec: §11\n// counters: a, a\n", "listed twice"),
    ] {
        let e = header_err(text);
        assert!(e.contains(what), "{text:?}: {e}");
    }
}

#[test]
fn s11_rule2_a_header_line_after_the_block_is_an_error() {
    for text in [
        "// spec: §11\n\n// xfail: why\n",
        "// spec: §11\n// a comment\n// steps: 3\n",
        "// spec: §11\n/* */ int x;\n// depth: 3\n",
        "\u{feff}// spec: §11\n",
        "int x;\n// spec: §11\n",
    ] {
        let e = header_err(text);
        assert!(e.contains("a header line outside the header block"), "{text:?}: {e}");
    }
    // Not a header line: a comment that looks like none after the block.
    assert!(transform_header("// spec: §11\n#include <x>\n// see: below\n").is_ok());
}

#[test]
fn s11_rule1_the_subjects_header_lines() {
    let h = subject_header("// std: c++26\n// target: aarch64-unknown-linux-gnu\n// args: -fms-extensions  -fchar8_t\nint x;\n")
        .unwrap();
    assert_eq!(h.std, "c++26");
    assert_eq!(h.target.as_deref(), Some("aarch64-unknown-linux-gnu"));
    assert_eq!(h.args, ["-fms-extensions", "-fchar8_t"]);
    let d = subject_header("int x;\n").unwrap();
    assert_eq!((d.std.as_str(), d.target, d.args.len()), ("c++20", None, 0));
    for text in [
        "// errors: yes\n",
        "// spec: §11\n",
        "// target: a b\n",
        "// std: c++20\n// std: c++23\n",
        "int x;\n// std: c++23\n",
        "// stdd: c++23\n",
    ] {
        assert!(subject_header(text).is_err(), "{text:?}");
    }
}

// ---------------------------------------------------------------------------
// The rendering (§11 rule 3; the grammar of spec/landed/api/WP-090.md).

fn place(path: &str, lo: u32, hi: u32) -> Place {
    Place {
        file: Arc::new(FileRef {
            path: path.as_bytes().to_vec(),
            size: 0,
            content: Digest([0; 32]),
        }),
        lo,
        hi,
    }
}

#[test]
fn s11_rule3_places_are_lines_and_columns() {
    assert_eq!(script_test::line_col(b"ab\ncd", 0), (1, 1));
    assert_eq!(script_test::line_col(b"ab\ncd", 2), (1, 3));
    assert_eq!(script_test::line_col(b"ab\ncd", 3), (2, 1));
    assert_eq!(script_test::line_col(b"ab\ncd", 5), (2, 3));
    assert_eq!(script_test::line_col(b"ab\ncd", 9), (2, 3));
    assert_eq!(script_test::line_col("é\nx".as_bytes(), 2), (1, 3), "columns are bytes");
}

#[test]
fn s11_rule3_a_completed_run() {
    let key = Digest([0x5a; 32]);
    let other = Digest([0x6b; 32]);
    let mut out = RunOutput::empty(Digest([0xab; 32]));
    out.edits = vec![slip_script::output::Edit {
        place: place("t.cpp", 4, 13),
        pieces: vec![
            Piece::Literal(b"g(".to_vec()),
            Piece::Copy(place("inc/a.h", 0, 2)),
            Piece::Spelled { key, text: b"lib::g".to_vec() },
            Piece::Spelled { key: other, text: b"h".to_vec() },
        ],
        tiers: Tiers::ZERO,
        notes: vec![],
    }];
    out.reports = vec![
        Report { place: None, severity: Severity::Note, message: b"m".to_vec(), attached: false },
        Report {
            place: Some(place("unknown.h", 1, 2)),
            severity: Severity::Review,
            message: b"r".to_vec(),
            attached: true,
        },
    ];
    out.changed = vec![Changed { key, place: place("t.cpp", 0, 3), reason: b"why".to_vec() }];
    out.counters = vec![
        Counter { name: "api.edits".to_owned(), value: 1 },
        Counter { name: "api.steps".to_owned(), value: 999 },
    ];
    let files = vec![
        ("inc/a.h".to_owned(), "x\ny\n".to_owned()),
        ("t.cpp".to_owned(), "int a;\nint b;\n".to_owned()),
    ];
    let names: BTreeMap<Digest, String> = [(key, "lib::g(int)".to_owned())].into();
    let counters = ["api.edits".to_owned(), "api.nothing".to_owned()];
    let text = script_test::render_completed(&out, &files, &names, &counters);
    assert_eq!(
        text,
        format!(
            "transform <id>\n\
             edit 0 t.cpp:1:5-2:7 tiers 0 = \"g(\" copy(inc/a.h:1:1-2:1) spell(\"lib::g(int)\" \"lib::g\") spell({other} \"h\")\n\
             report - note \"m\"\n\
             report unknown.h:1-2 review \"r\" attached\n\
             changed \"lib::g(int)\" t.cpp:1:1-1:4 \"why\"\n\
             counter api.edits 1\n\
             counter api.nothing absent\n"
        )
    );
}

#[test]
fn s11_rule3_a_failed_run() {
    assert_eq!(
        script_test::render_failed(FailureKind::StepBudget, "first \"line\"\tx\u{a7}\nsecond"),
        "failed step-budget \"first \\\"line\\\"\\tx\\xc2\\xa7\"\n"
    );
    assert_eq!(script_test::render_failed(FailureKind::Uncaught, ""), "failed uncaught \"\"\n");
}

// ---------------------------------------------------------------------------
// Judgement and blessing.

fn rendered(s: &str) -> Ran {
    Ran::Rendered(s.to_owned())
}

#[test]
fn wp090_a_rendering_is_compared_byte_for_byte() {
    let ok = "transform <id>\nreport - note \"x\"\n";
    assert_eq!(judge(None, Some(ok.as_bytes()), rendered(ok), false), (Verdict::Pass, None));
    for differs in [
        "transform <id>\nreport - note \"x\"",
        "transform <id>\nreport - note \"x\"\n\n",
        "transform <id>\r\nreport - note \"x\"\r\n",
        "transform <id>\nreport - note \"x\" \n",
        "transform <id>\nreport - note \"X\"\n",
        "",
    ] {
        let (v, w) = judge(None, Some(differs.as_bytes()), rendered(ok), false);
        assert!(matches!(v, Verdict::Fail(_)), "{differs:?}: {v:?}");
        assert!(v.fails() && w.is_none());
    }
    let (v, _) = judge(None, Some(b"transform <id>\nreport - note \"y\"\n"), rendered(ok), false);
    let Verdict::Fail(why) = v else { panic!() };
    assert!(why.starts_with("expect.txt differs at line 2:\n"), "{why}");
}

#[test]
fn wp090_a_missing_expect_txt_is_an_error_outside_bless() {
    let (v, w) = judge(None, None, rendered("transform <id>\n"), false);
    assert!(matches!(&v, Verdict::Error(e) if e.starts_with("expect.txt is missing")), "{v:?}");
    assert!(v.fails() && w.is_none());
    let (v, _) = judge(Some("why"), None, rendered("transform <id>\n"), false);
    assert!(matches!(v, Verdict::Error(_)), "{v:?}");
}

#[test]
fn wp090_xfail() {
    let (v, _) = judge(Some("why"), Some(b"a\n"), rendered("b\n"), false);
    assert_eq!(v, Verdict::Xfail("why".to_owned()));
    assert!(!v.fails());
    let (v, _) = judge(Some("why"), Some(b"a\n"), rendered("a\n"), false);
    assert_eq!(v, Verdict::Xpass);
    assert!(v.fails(), "an xfail that passes fails the suite");
    // Neither an error of the test nor a missing result is masked.
    let (v, _) = judge(Some("why"), Some(b"a\n"), Ran::Error("bad".to_owned()), false);
    assert_eq!(v, Verdict::Error("bad".to_owned()));
    let (v, _) = judge(Some("why"), Some(b"a\n"), Ran::NoResult("retired".to_owned()), false);
    assert_eq!(v, Verdict::Fail("retired".to_owned()));
    assert!(v.fails());
}

#[test]
fn wp090_bless() {
    assert_eq!(
        judge(None, None, rendered("a\n"), true),
        (Verdict::Blessed, Some("a\n".to_owned()))
    );
    assert_eq!(
        judge(None, Some(b"b\n"), rendered("a\n"), true),
        (Verdict::Blessed, Some("a\n".to_owned()))
    );
    assert_eq!(judge(None, Some(b"a\n"), rendered("a\n"), true), (Verdict::Pass, None));
    // An xfail test is not blessed: its expect.txt holds what the host
    // does not give yet.
    assert_eq!(judge(Some("w"), Some(b"b\n"), rendered("a\n"), true).1, None);
    assert_eq!(judge(Some("w"), Some(b"b\n"), rendered("a\n"), true).0, Verdict::Xfail("w".into()));
    // Nothing is blessed from an error or from no result.
    assert_eq!(judge(None, None, Ran::Error("e".into()), true).1, None);
    assert_eq!(judge(None, None, Ran::NoResult("e".into()), true).1, None);
    assert_eq!(script_test::bless_from(None), Ok(false));
    assert_eq!(script_test::bless_from(Some("1".as_ref())), Ok(true));
    for v in ["", "0", "yes", "true", " 1"] {
        assert!(script_test::bless_from(Some(v.as_ref())).is_err(), "{v:?}");
    }
}

#[test]
fn wp090_summary() {
    let case = |v: Verdict| Case { name: "x".into(), verdict: v };
    let (line, ok) = script_test::summary(&[case(Verdict::Pass), case(Verdict::Xfail("w".into()))]);
    assert_eq!((line.as_str(), ok), ("api: 1 passed, 1 xfailed", true));
    for v in [Verdict::Fail("f".into()), Verdict::Xpass, Verdict::Error("e".into())] {
        let (_, ok) = script_test::summary(&[case(Verdict::Pass), case(v.clone())]);
        assert!(!ok, "{v:?}");
    }
    assert_eq!(
        script_test::summary(&[case(Verdict::Blessed)]),
        ("api: 0 passed, 1 blessed".into(), true)
    );
}

// ---------------------------------------------------------------------------
// Discovery, grouping, retirement.

/// A new directory under Cargo's temporary directory.
fn scratch(name: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("script-runner-{}-{n}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::canonicalize(&dir).unwrap()
}

fn write(root: &Path, rel: &str, text: &str) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

#[test]
fn wp090_discovery() {
    let root = scratch("discover");
    let e = script_test::discover(&root).unwrap_err();
    assert!(e.ends_with("no script test directory (one with a transform.cpp)"), "{e}");
    write(&root, "b/one/transform.cpp", "");
    write(&root, "a/two/transform.cpp", "");
    write(&root, "a/two/inc/x.h", "");
    write(&root, "a/two/t.cpp", "");
    let found: Vec<String> = script_test::discover(&root)
        .unwrap()
        .iter()
        .map(|d| script_test::case_name(&root, d))
        .collect();
    assert_eq!(found, ["a/two", "b/one"]);
    write(&root, "a/stray.txt", "");
    let e = script_test::discover(&root).unwrap_err();
    assert!(e.contains("stray.txt: a file outside every script test directory"), "{e}");
    std::fs::remove_file(root.join("a/stray.txt")).unwrap();
    write(&root, "b/one/inner/transform.cpp", "");
    let e = script_test::discover(&root).unwrap_err();
    assert!(e.contains("inner: a script test inside another one's directory"), "{e}");
}

fn script(dir: &str) -> Script {
    Script {
        dir: PathBuf::from(dir),
        spec: "§11".into(),
        args: vec![],
        budgets: Budgets::default(),
        owned: true,
        xfail: None,
        counters: vec![],
    }
}

#[test]
fn wp090_rule2_one_group_per_args_and_bytes() {
    let k = |args: &[&str], bytes: &str| {
        (args.iter().map(|a| (*a).to_owned()).collect::<Vec<_>>(), bytes.as_bytes().to_vec())
    };
    let keyed = vec![
        (0, k(&[], "A"), script("d0")),
        (1, k(&["-DX"], "A"), script("d1")),
        (2, k(&[], "B"), script("d2")),
        (3, k(&[], "A"), script("d3")),
        (4, k(&["-DX"], "A"), script("d4")),
    ];
    let groups: Vec<Vec<usize>> = script_test::groups(keyed)
        .into_iter()
        .map(|g| g.into_iter().map(|(i, _)| i).collect())
        .collect();
    assert_eq!(groups, [vec![0, 3], vec![1, 4], vec![2]]);
}

/// Runs scripts, retiring the transform at the run of `retire_at`.
struct Retiring {
    ran: Vec<String>,
    retire_at: usize,
}

impl GroupRun for Retiring {
    fn run(&mut self, script: &Script) -> Ran {
        self.ran.push(script.dir.display().to_string());
        if self.ran.len() == self.retire_at + 1 {
            Ran::NoResult("the run is a host-failure: boom".into())
        } else {
            Ran::Rendered("transform <id>\n".into())
        }
    }
    fn retired(&self) -> Option<String> {
        (self.ran.len() > self.retire_at).then(|| "host-failure: boom".to_owned())
    }
}

// The driver retires a transform after a host failure (spec/api.md §2.8,
// spec/landed/api/WP-089.md rule 4): the tests of the group after it are
// failures that name the retirement, and are not run.
#[test]
fn wp090_a_retired_transform_fails_the_rest_of_its_group() {
    let scripts = [script("d0"), script("d1"), script("d2"), script("d3")];
    let refs: Vec<&Script> = scripts.iter().collect();
    let mut r = Retiring { ran: vec![], retire_at: 1 };
    let out = script_test::run_each(&mut r, &refs);
    assert_eq!(r.ran, ["d0", "d1"]);
    assert_eq!(out[0], Ran::Rendered("transform <id>\n".into()));
    assert_eq!(out[1], Ran::NoResult("the run is a host-failure: boom".into()));
    for o in &out[2..] {
        assert_eq!(
            o,
            &Ran::NoResult(
                "not run: the transform was retired by the run of `d1`: host-failure: boom".into()
            )
        );
    }
    let (v, _) = judge(Some("an xfail does not mask it"), Some(b"x\n"), out[2].clone(), false);
    assert!(matches!(v, Verdict::Fail(_)), "{v:?}");
}

// ---------------------------------------------------------------------------
// A suite over a fixture tree, with real transforms.

fn resource() -> PathBuf {
    std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../resource")).unwrap()
}

const REPORT: &str = "#include <slip/meta.h>\n\
    consteval void transform() { slip::report(slip::severity::note, \"hi\"); }\n";
const RENDERED: &str = "transform <id>\nreport - note \"hi\"\n";

#[test]
fn wp090_a_suite() {
    let root = scratch("suite");
    let t = |dir: &str, header: &str, expect: Option<&str>| {
        write(&root, &format!("{dir}/transform.cpp"), &format!("{header}{REPORT}"));
        write(&root, &format!("{dir}/t.cpp"), "int x;\n");
        if let Some(e) = expect {
            write(&root, &format!("{dir}/expect.txt"), e);
        }
    };
    t("pass", "// spec: §11\n", Some(RENDERED));
    t("same", "// spec: §11\n", Some(RENDERED));
    t("fail", "// spec: §11\n// args: -DX\n", Some("transform <id>\n"));
    t("xpass", "// spec: §11\n// xfail: it passes\n", Some(RENDERED));
    t("xfail", "// spec: §11\n// xfail: it differs\n", Some("transform <id>\n"));
    t("missing", "// spec: §11\n", None);
    t("unloadable", "// spec: §11\n// arg: -DX\n", Some(RENDERED));
    t("nosubject", "// spec: §11\n", Some(RENDERED));
    std::fs::remove_file(root.join("nosubject/t.cpp")).unwrap();
    t("badsubject", "// spec: §11\n", Some(RENDERED));
    write(&root, "badsubject/t.cpp", "int x = ;\n");
    write(&root, "reads/transform.cpp", &format!("// spec: §11\n#include \"helper.h\"\n{REPORT}"));
    write(&root, "reads/helper.h", "\n");
    write(&root, "reads/t.cpp", "int x;\n");
    write(&root, "reads/expect.txt", RENDERED);
    let tokens = Mutex::new(0usize);
    let token = || -> Box<dyn std::any::Any> {
        *tokens.lock().unwrap() += 1;
        Box::new(())
    };
    let seen = Mutex::new(Vec::new());
    let report = |c: &Case| seen.lock().unwrap().push(c.name.clone());
    let opts = SuiteOptions { threads: 3, ..SuiteOptions::default() };
    let cases = script_test::run_suite(&root, &resource(), &opts, &token, &report).unwrap();
    let got: Vec<(String, &str)> =
        cases.iter().map(|c| (c.name.clone(), c.verdict.name())).collect();
    let want = [
        ("badsubject", "ERROR"),
        ("fail", "FAIL"),
        ("missing", "ERROR"),
        ("nosubject", "ERROR"),
        ("pass", "PASS"),
        ("reads", "ERROR"),
        ("same", "PASS"),
        ("unloadable", "ERROR"),
        ("xfail", "XFAIL"),
        ("xpass", "XPASS"),
    ];
    assert_eq!(got, want.map(|(a, b)| (a.to_owned(), b)), "{cases:#?}");
    let why = |name: &str| match &cases.iter().find(|c| c.name == name).unwrap().verdict {
        Verdict::Error(e) | Verdict::Fail(e) => e.clone(),
        v => panic!("{v:?}"),
    };
    assert!(why("badsubject").contains("the subject has 1 error(s)"), "{}", why("badsubject"));
    assert!(why("unloadable").starts_with("transform.cpp: line 2: "), "{}", why("unloadable"));
    assert!(why("nosubject").starts_with("t.cpp: cannot be read"), "{}", why("nosubject"));
    assert!(why("reads").contains("helper.h`, a file of its directory"), "{}", why("reads"));
    assert!(why("missing").starts_with("expect.txt is missing"), "{}", why("missing"));
    // Every case was reported once; and one token per group. `pass`,
    // `same`, `missing` and `badsubject` have the same transform.cpp
    // bytes and no args: one group. `fail`, `xpass`, `xfail` and `reads`
    // differ in their bytes; `unloadable` and `nosubject` do not load.
    assert_eq!(seen.lock().unwrap().len(), cases.len());
    assert_eq!(*tokens.lock().unwrap(), 5, "one token per group");
    let (_, ok) = script_test::summary(&cases);
    assert!(!ok);

    // Narrowed, then blessed: only the selected tests, and `missing` is
    // written; a second bless changes nothing.
    let narrow = SuiteOptions {
        filters: vec!["missing".into(), "xfail".into()],
        threads: 2,
        bless: true,
        exact: true,
    };
    let cases = script_test::run_suite(&root, &resource(), &narrow, &token, &report).unwrap();
    let got: Vec<(&str, &str)> =
        cases.iter().map(|c| (c.name.as_str(), c.verdict.name())).collect();
    assert_eq!(got, [("missing", "BLESSED"), ("xfail", "XFAIL")]);
    assert_eq!(std::fs::read_to_string(root.join("missing/expect.txt")).unwrap(), RENDERED);
    assert_eq!(std::fs::read_to_string(root.join("xfail/expect.txt")).unwrap(), "transform <id>\n");
    let cases = script_test::run_suite(&root, &resource(), &narrow, &token, &report).unwrap();
    assert_eq!(cases[0].verdict, Verdict::Pass);
    let _ = std::fs::remove_dir_all(&root);
}

// `load_script` is the reading a pipeline transform test of
// spec/harness.md §6 uses (§11 rule 4); `run_script` the run of one.
#[test]
fn s11_rule4_load_and_run_one_script() {
    let root = scratch("one");
    write(&root, "transform.cpp", &format!("// spec: §11\n// counters: api.reports\n{REPORT}"));
    write(&root, "t.cpp", "int x;\n");
    let s = load_script(&root).unwrap();
    assert_eq!(s.counters, ["api.reports"]);
    assert_eq!(
        script_test::run_script(&s, &resource()),
        Ok(format!("{RENDERED}counter api.reports 1\n"))
    );
    let _ = std::fs::remove_dir_all(&root);
}

// A `host-failure`, at load or in a run, is the host's (spec/api.md §2.8,
// §12): never a rendering, so neither `expect.txt` nor `xfail` can hide it.
#[test]
fn wp090_a_host_failure_is_never_rendered() {
    let host = LoadError::Host { message: "kiln panicked".into(), rendered: String::new() };
    let Ran::NoResult(why) = script_test::load_failed(&host) else { panic!() };
    assert!(why.contains("kiln panicked"), "{why}");
    let compile = LoadError::Compile { rendered: "t.cpp:1: error: x".into() };
    assert_eq!(
        script_test::load_failed(&compile),
        Ran::Rendered("error transform-compile\n".into())
    );
    assert_eq!(
        script_test::load_failed(&LoadError::Entry("entry-missing")),
        Ran::Rendered("error entry-missing\n".into())
    );
    let failure = |kind| Failure {
        kind,
        message: "what failed\nmore".into(),
        place: None,
        rendered: String::new(),
        refusal: None,
    };
    let Ran::NoResult(why) = script_test::run_failed(&failure(FailureKind::HostFailure)) else {
        panic!()
    };
    assert!(why.contains("host-failure: what failed"), "{why}");
    assert_eq!(
        script_test::run_failed(&failure(FailureKind::Uncaught)),
        Ran::Rendered("failed uncaught \"what failed\"\n".into())
    );
}

// Each group's thread has the stack the driver requires (WP-089).
#[test]
fn wp090_a_group_thread_has_the_drivers_stack() {
    const { assert!(script_test::GROUP_STACK >= driver::STACK) };
}

// libtest's filter rule: in the name, or with --exact the name.
#[test]
fn wp090_filters() {
    let f = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    assert!(script_test::selects(&[], false, "format/xfail"));
    assert!(script_test::selects(&[], true, "format/xfail"));
    assert!(script_test::selects(&f(&["xfail"]), false, "format/xfail"));
    assert!(!script_test::selects(&f(&["xfail"]), true, "format/xfail"));
    assert!(script_test::selects(&f(&["no", "format/xfail"]), true, "format/xfail"));
    assert!(!script_test::selects(&f(&["pass"]), true, "xpass"));
    assert!(script_test::selects(&f(&["pass"]), false, "xpass"));
}

// The subject is every file of the directory at any depth, but the
// transform and expect.txt at its top (§11's table).
#[test]
fn s11_the_subjects_files() {
    let root = scratch("files");
    for f in [
        "transform.cpp",
        "expect.txt",
        "t.cpp",
        "b.h",
        "inc/a.h",
        "inc/expect.txt",
        "inc/deep/transform.cpp",
    ] {
        write(&root, f, f);
    }
    let files = script_test::subject_files(&root).unwrap();
    let names: Vec<&str> = files.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(names, ["b.h", "inc/a.h", "inc/deep/transform.cpp", "inc/expect.txt", "t.cpp"]);
    assert!(files.iter().all(|(p, t)| p == t));
    let _ = std::fs::remove_dir_all(&root);
}

// The review of WP-095, F7: the script-test path (`run_one`) runs the
// transform inside the subject's key server, so no key question starts
// a server of its own (the counter `api.one_shot_key_servers` is absent).
#[test]
fn wp095_a_script_run_asks_every_key_of_its_own_server() {
    let root = scratch("keys");
    write(
        &root,
        "transform.cpp",
        "// spec: §7.5\n// counters: api.one_shot_key_servers\n#include <slip/meta.h>\n\
         using namespace slip;\n\
         consteval void transform() {\n\
           for (info e : lookup(\"g\"))\n\
             for (info c : calls_to(e)) expect_changed(c, \"why\");\n\
         }\n",
    );
    write(&root, "t.cpp", "int g(int);\nint h() { return g(1); }\n");
    let s = load_script(&root).unwrap();
    let out = script_test::run_script(&s, &resource()).expect("a result");
    assert!(out.contains("changed"), "{out}");
    assert!(out.contains("counter api.one_shot_key_servers absent"), "{out}");
    let _ = std::fs::remove_dir_all(&root);
}
