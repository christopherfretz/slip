//! `cargo xtask transform` (spec/harness.md §6.2 to §6.4), against the
//! stand-in `fake-slip` (`xtask/src/bin/fake-slip.rs`), which does what
//! the `fake.txt` of the test's tree says. Every test writes its transform
//! test under a fixture's `tests/transform/rename/<name>/` and works in
//! the fixture's `target/transform/rename/<name>/`; edit sets and results
//! the fake publishes are built here with `slip_model`. The parse tests
//! also read the committed fixtures of `xtask/tests/transform/`.

#![cfg(target_os = "linux")]

mod common;

use common::Fixture;
use slip_exec::store::{Access, Object, Store};
use slip_model::edits::{MacroSite, Piece, Place};
use slip_model::editset::{Edit, EditSet, SetState};
use slip_model::hash::content_digest;
use slip_model::result::{self, Category, Residue, RunVerdict, State, Tally, TransformResult};
use slip_model::{Digest, FileRef, formats};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use xtask::transform::mutate::{self, mutations};
use xtask::transform::run::{Lines, cmdline, first_line_difference, normalise, read_commands};
use xtask::transform::{
    Allow, Expected, Oracle, Outcome, Test, find_clang, parse_args, parse_test, run_one, run_test,
};

const FAKE: &str = env!("CARGO_BIN_EXE_fake-slip");

const PRE: &str = "namespace a { int f(); }\nint x = a::f();\n";
const POST: &str = "namespace a { int g(); }\nint x = a::g();\n";

fn hex(n: u8) -> String {
    format!("{n:02x}").repeat(32)
}
fn d(n: u8) -> Digest {
    Digest([n; 32])
}

/// `\n` escaped for a `fake.txt` line.
fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\n', "\\n")
}

// ---------------------------------------------------------------- a case

/// A transform test in a fixture: `tests/transform/rename/<name>/`.
struct Case {
    _fixture: Fixture,
    name: String,
    /// The fixture's root, canonical.
    root: PathBuf,
}

impl Case {
    fn new(name: &str) -> Case {
        let f = Fixture::empty(&format!("transform-{name}"));
        let root = f.root().canonicalize().unwrap();
        Case { _fixture: f, name: name.to_owned(), root }
    }
    fn dir(&self) -> PathBuf {
        self.root.join("tests/transform/rename").join(&self.name)
    }
    fn write(&self, rel: &str, text: &str) {
        let p = self.dir().join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    fn read(&self, rel: &str) -> Option<String> {
        std::fs::read_to_string(self.dir().join(rel)).ok()
    }
    fn work(&self) -> PathBuf {
        self.root.join("target/transform/rename").join(&self.name)
    }
    fn tree(&self) -> PathBuf {
        self.work().join("tree")
    }
    fn test(&self) -> Test {
        parse_test(&self.dir()).unwrap()
    }
    fn run(&self) -> Outcome {
        run_one(&self.test(), Path::new(FAKE), &self.work(), None, false)
    }
    fn run_with(&self, clang: Option<&Path>, bless: bool) -> Outcome {
        run_one(&self.test(), Path::new(FAKE), &self.work(), clang, bless)
    }
    /// The invocations the fake logged in `<work>/<store>/`.
    fn log(&self, store: &str) -> Vec<String> {
        let p = self.work().join(store).join("fake-slip.log");
        std::fs::read_to_string(p).unwrap_or_default().lines().map(str::to_owned).collect()
    }
    /// A file the fake publishes from, outside the test.
    fn object(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let p = self.root.join("objects").join(name);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, bytes).unwrap();
        p
    }
}

/// The main run's output: plan, transform, round, edit set, declared,
/// overlay, result, with the given state.
fn main_out(edits: &str, state: &str) -> Vec<String> {
    vec![
        format!("plan {}", hex(0x11)),
        format!("transform {}", hex(0x22)),
        "round 1".into(),
        "map: selected 1, ok 1, not ok 0, failed runs 0".into(),
        format!("edits {edits}"),
        "state accepted".into(),
        format!("declared {}: rules 1, exempt 0", hex(0x44)),
        format!("overlay {}: files 1, spellings 1", hex(0x55)),
        format!("result {}", hex(0x66)),
        format!("state {state}"),
    ]
}

/// The golden of an output: ids made `<id>`.
fn golden(lines: &[String]) -> String {
    let mut s = String::new();
    for l in lines {
        s.push_str(
            &l.replace(&hex(0x11), "<id>")
                .replace(&hex(0x22), "<id>")
                .replace(&hex(0x33), "<id>")
                .replace(&hex(0x44), "<id>")
                .replace(&hex(0x55), "<id>")
                .replace(&hex(0x66), "<id>"),
        );
        s.push('\n');
    }
    s
}

fn outs(lines: &[String]) -> String {
    lines.iter().map(|l| ["out ", l, "\n"].concat()).collect()
}

/// A script for a run that verifies and writes `POST`, whose inverse
/// writes `PRE` back, and whose `check` gives the run's result.
fn good_script(edits: &str) -> String {
    let main = outs(&main_out(edits, "verified"));
    format!(
        "== rename * --from a::g\n{main}write a.cpp {pre}\n\
         == rename *\n{main}write a.cpp {post}\n\
         == check 0\nout result {r}\nout state verified\n",
        pre = esc(PRE),
        post = esc(POST),
        r = hex(0x66)
    )
}

const TEST_TXT: &str = "spec: harness §6.3\nrun: rename --from a::f --to g\nstate: verified\n\
                        oracle: no-mutation the fake has no edit set\n";

/// The standard passing test, with `script` as its `fake.txt`.
fn standard(name: &str, test_txt: &str, script: &str) -> Case {
    let c = Case::new(name);
    c.write("test.txt", test_txt);
    c.write("pre/a.cpp", PRE);
    c.write("pre/fake.txt", script);
    c.write("post/a.cpp", POST);
    c.write("expect.txt", &golden(&main_out(&hex(0x33), "verified")));
    c
}

fn fail_message(o: &Outcome) -> &str {
    match o {
        Outcome::Fail(m) => m,
        other => panic!("not a FAIL: {other:?}"),
    }
}

// ---------------------------------------------------------------- parse_test (§6.2)

fn fixture_dir(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/transform").join(rel)
}

// §6.2: every directive, read from a committed fixture.
#[test]
fn s6_2_parse_every_directive() {
    let t = parse_test(&fixture_dir("rename/every-directive")).unwrap();
    assert_eq!((t.area.as_str(), t.name.as_str()), ("rename", "every-directive"));
    assert_eq!(t.spec, ["edits §9.3", "harness §6.2"]);
    assert_eq!(t.run, ["rename", "--from", "a::f", "--to", "g", "--kind", "function"]);
    assert_eq!(t.state, Expected::VerifiedWithResidue);
    assert_eq!(t.exit, 1);
    assert_eq!(t.std, "c++26");
    assert_eq!(t.args, ["-DX=1", "-fno-exceptions"]);
    assert_eq!(t.tu, ["b.cpp", "a.cpp"]);
    assert_eq!(t.owned, ["a.cpp", "inc"]);
    assert_eq!(t.readonly, ["dep"]);
    assert_eq!(t.allow, [Allow::Residue, Allow::Unparsed, Allow::Partial, Allow::Errors]);
    assert_eq!(
        t.oracle_off,
        [
            (Oracle::Back, "the inverse is refused".to_owned()),
            (Oracle::Clang, "kiln only".to_owned())
        ]
    );
    assert_eq!(t.xfail.as_deref(), Some("not yet"));
    assert!(t.cdb && t.expect);
    assert!(!t.oracle_on(Oracle::Back) && t.oracle_on(Oracle::Verify));
}

// §6.2: the defaults: exit of the state, c++20, no args, every *.cpp not
// under dep/ in byte order, owned `.`, readonly `dep` when pre/dep exists,
// every oracle on.
#[test]
fn s6_2_parse_defaults() {
    let t = parse_test(&fixture_dir("rename/defaults")).unwrap();
    assert_eq!(t.spec, ["edits §9.1"]);
    assert_eq!(t.state, Expected::NotVerified);
    assert_eq!(t.exit, 1);
    assert_eq!(t.std, "c++20");
    assert!(t.args.is_empty());
    assert_eq!(t.tu, ["B.cpp", "a.cpp", "sub/c.cpp"]);
    assert_eq!(t.owned, ["."]);
    assert_eq!(t.readonly, ["dep"]);
    assert!(t.allow.is_empty() && t.oracle_off.is_empty() && t.xfail.is_none());
    assert!(!t.cdb && !t.expect);
    // The default exit of each state.
    for (state, exit) in [
        ("verified", 0),
        ("verified_with_residue", 1),
        ("not_verified", 1),
        ("refused", 1),
        ("usage", 2),
        ("input", 3),
        ("incomplete", 4),
    ] {
        let c = Case::new("defaults");
        c.write(
            "test.txt",
            &format!("spec: edits §1\nrun: rename --from f --to g\nstate: {state}\n"),
        );
        c.write("pre/a.cpp", "");
        let t = c.test();
        assert_eq!((t.exit, t.state.name()), (exit, state));
        assert!(t.readonly.is_empty(), "no pre/dep, no default readonly");
    }
}

// §6.2: each malformed form is an error naming the line.
#[test]
fn s6_2_parse_malformed() {
    let base = "spec: edits §1\nrun: rename --from f --to g\nstate: verified\n";
    let cases: &[(&str, &str)] = &[
        ("nonsense\n", "test.txt:4: `nonsense` is not `<name>: <value>`"),
        ("colour: red\n", "test.txt:4: unknown directive `colour`"),
        ("std:c++20\n", "test.txt:4: `std:c++20` is not `<name>: <value>`"),
        ("std:  c++20\n", "test.txt:4: `std`: the value is empty or has white space"),
        ("std: c++20 \n", "test.txt:4: `std`: the value is empty or has white space"),
        ("spec: edits 9.3\n", "test.txt:4: spec: `edits 9.3` is not a section"),
        ("spec: Edits §9\n", "test.txt:4: spec: `Edits §9` is not a section"),
        ("spec: edits §9.\n", "test.txt:4: spec: `edits §9.` is not a section"),
        ("run: rename --from f --to h\n", "test.txt:4: `run` given twice"),
        ("state: refused\n", "test.txt:4: `state` given twice"),
        ("state: done\n", "test.txt:4: state: `done` is not a state"),
        ("exit: 256\n", "test.txt:4: exit: `256` is not a status 0 to 255"),
        ("exit: -1\n", "test.txt:4: exit: `-1` is not a status 0 to 255"),
        ("exit: 1\nexit: 1\n", "test.txt:5: `exit` given twice"),
        ("std: c++20\nstd: c++23\n", "test.txt:5: `std` given twice"),
        ("std: c++ 20\n", "test.txt:4: std: `c++ 20` is not one word"),
        ("args: -DX\nargs: -DY\n", "test.txt:5: `args` given twice"),
        ("args: -DX  -DY\n", "test.txt:4: two spaces in a row"),
        ("tu: b.cpp\n", "test.txt:4: tu: `b.cpp` is not a file of pre/"),
        ("tu: a.cpp a.cpp\n", "test.txt:4: tu: `a.cpp` given twice"),
        ("tu: ../a.cpp\n", "test.txt:4: `../a.cpp` is not a path relative to the tree"),
        ("owned: /abs\n", "test.txt:4: `/abs` is not a path relative to the tree"),
        ("owned: nowhere\n", "test.txt:4: owned: `nowhere` is not a file or directory of pre/"),
        ("readonly: a.cpp\nreadonly: a.cpp\n", "test.txt:5: readonly: `a.cpp` given twice"),
        ("allow: all\n", "test.txt:4: allow: `all` is not residue"),
        ("allow: residue\nallow: residue\n", "test.txt:5: allow: `residue` given twice"),
        ("oracle: no-speed fast\n", "test.txt:4: oracle: `no-speed` is not no-determinism"),
        ("oracle: back x\n", "test.txt:4: oracle: `back` is not no-determinism"),
        ("oracle: no-back\n", "test.txt:4: oracle: `no-back` needs a reason"),
        ("oracle: no-back a\noracle: no-back b\n", "test.txt:5: oracle: `no-back` given twice"),
        ("xfail: a\nxfail: b\n", "test.txt:5: `xfail` given twice"),
    ];
    for (extra, want) in cases {
        let c = Case::new("malformed");
        c.write("test.txt", &format!("{base}{extra}"));
        c.write("pre/a.cpp", "");
        let e = parse_test(&c.dir()).unwrap_err();
        assert!(e.starts_with(want), "{extra:?}: {e}");
    }
    let whole: &[(&str, &str)] = &[
        ("run: rename --from f --to g\nstate: verified\n", "test.txt: no `spec` directive"),
        ("spec: edits §1\nstate: verified\n", "test.txt: no `run` directive"),
        ("spec: edits §1\nrun: rename --from f --to g\n", "test.txt: no `state` directive"),
        (
            "spec: edits §1\nrun: map --from f --to g\nstate: verified\n",
            "test.txt: `run` is `map`, which is not the transform of its area `rename`",
        ),
    ];
    for (text, want) in whole {
        let c = Case::new("malformed");
        c.write("test.txt", text);
        c.write("pre/a.cpp", "");
        assert_eq!(parse_test(&c.dir()).unwrap_err(), *want);
    }
    // The options the runner gives itself.
    for opt in ["--write", "--store x", "--db=x", "-j 4", "-j4", "--allow-residue", "--patch p"] {
        let c = Case::new("malformed");
        c.write("test.txt", &format!("spec: edits §1\nrun: rename {opt}\nstate: verified\n"));
        c.write("pre/a.cpp", "");
        let e = parse_test(&c.dir()).unwrap_err();
        assert!(e.contains("is an option the runner gives"), "{opt}: {e}");
    }
    // Comments and blank lines are skipped; a test needs pre/ and test.txt,
    // has no other entry, and a pre/ with a main file.
    let c = Case::new("comments");
    c.write("test.txt", &format!("# a comment\n\n{base}"));
    c.write("pre/a.cpp", "");
    assert!(parse_test(&c.dir()).is_ok());
    c.write("notes.txt", "");
    assert!(parse_test(&c.dir()).unwrap_err().starts_with("`notes.txt` is not an entry of a test"));
    let c = Case::new("nopre");
    c.write("test.txt", base);
    assert_eq!(parse_test(&c.dir()).unwrap_err(), "no pre/ directory");
    let c = Case::new("notu");
    c.write("test.txt", base);
    c.write("pre/dep/x.cpp", "");
    assert!(parse_test(&c.dir()).unwrap_err().starts_with("pre/ has no *.cpp outside dep/"));
    let c = Case::new("notxt");
    c.write("pre/a.cpp", "");
    assert_eq!(parse_test(&c.dir()).unwrap_err(), "no test.txt");
}

// §6.2: "A post/ file equal to its pre/ file, or with no pre/ file, is an
// error of the test. A test whose state does not permit writing has no
// post/."
#[test]
fn s6_2_post_errors() {
    let base = "spec: edits §1\nrun: rename --from f --to g\nstate: verified\n";
    let c = Case::new("post");
    c.write("test.txt", base);
    c.write("pre/a.cpp", "x");
    c.write("post/a.cpp", "x");
    assert_eq!(parse_test(&c.dir()).unwrap_err(), "post/a.cpp is equal to its pre/ file");
    c.write("post/a.cpp", "y");
    c.write("post/b.cpp", "y");
    assert_eq!(parse_test(&c.dir()).unwrap_err(), "post/b.cpp has no pre/ file");
    let c = Case::new("post");
    c.write("test.txt", &base.replace("verified", "not_verified"));
    c.write("pre/a.cpp", "x");
    c.write("post/a.cpp", "y");
    assert_eq!(
        parse_test(&c.dir()).unwrap_err(),
        "the state `not_verified` does not permit writing, and the test has a post/"
    );
}

// ---------------------------------------------------------------- the run (§6.3)

// §6.3: a passing test, and the command lines the runner gave.
#[test]
fn s6_3_passing_test_and_its_command_lines() {
    let c = standard("pass", TEST_TXT, &good_script(&hex(0x33)));
    let report = run_test(&c.test(), Path::new(FAKE), &c.work(), None, false);
    assert_eq!(report.outcome, Outcome::Pass);
    assert!(report.clang_skipped, "the tree was written and there is no clang");
    let (work, tree) = (c.work(), c.tree());
    let (w, t) = (work.display(), tree.display());
    assert_eq!(
        c.log("store"),
        [
            format!(
                "rename --from a::f --to g --db {w}/cdb.json --owned {t} --store {w}/store \
                 --relative-to {t} --write -j 2"
            ),
            format!(
                "check --plan {} --round 1 --edits {} --store {w}/store -j 2",
                hex(0x11),
                hex(0x33)
            ),
        ]
    );
    assert_eq!(
        c.log("back/store"),
        [format!(
            "rename --from a::g --to f --db {w}/cdb.json --owned {t} --store {w}/back/store \
             --relative-to {t} --write -j 2"
        )]
    );
    assert_eq!(
        c.log("determinism/store"),
        [format!(
            "rename --from a::f --to g --db {w}/cdb.json --owned {t} --store \
             {w}/determinism/store --relative-to {t} --write -j 1"
        )]
    );
    // The default database (§6.2).
    let db = std::fs::read_to_string(work.join("cdb.json")).unwrap();
    assert_eq!(
        read_commands(&db).unwrap(),
        [(
            tree.clone(),
            ["kiln", "-std=c++20", &format!("-I{t}"), &format!("-I{t}/dep"), "-c", "a.cpp"]
                .map(str::to_owned)
                .to_vec()
        )]
    );
    assert!(db.contains("\"file\": \"a.cpp\""), "{db}");
    // Nothing was written into the test's directory.
    assert_eq!(c.read("expect.txt").unwrap(), golden(&main_out(&hex(0x33), "verified")));
    assert!(!c.dir().join("pre/store").exists());
}

// §6.2, §6.3: std, args, tu, owned, readonly and allow reach the command
// lines; `check` gets `--allow-unparsed`; cdb.json has @ROOT@ replaced.
#[test]
fn s6_3_directives_reach_the_command_lines() {
    let txt = "spec: harness §6.3\nrun: rename --from a::f --to g --kind function\nstate: verified\n\
               std: c++26\nargs: -DX -DY\ntu: b.cpp a.cpp\nowned: a.cpp\nowned: b.cpp\n\
               allow: unparsed\nallow: residue\noracle: no-mutation none\noracle: no-back none\n";
    let c = standard("lines", txt, &good_script(&hex(0x33)).replace("--from a::g", "--from a::x"));
    c.write("pre/b.cpp", "int b;\n");
    c.write("pre/dep/d.h", "int d;\n");
    assert_eq!(c.run(), Outcome::Pass);
    let (work, tree) = (c.work(), c.tree());
    let (w, t) = (work.display(), tree.display());
    let log = c.log("store");
    assert_eq!(
        log[0],
        format!(
            "rename --from a::f --to g --kind function --db {w}/cdb.json --owned {t}/a.cpp \
             --owned {t}/b.cpp --readonly {t}/dep --store {w}/store --relative-to {t} --write -j 2 \
             --allow-residue --allow-unparsed"
        )
    );
    assert!(log[1].ends_with(" -j 2 --allow-unparsed"), "{}", log[1]);
    let db = std::fs::read_to_string(work.join("cdb.json")).unwrap();
    let cmds = read_commands(&db).unwrap();
    let files: Vec<&str> = cmds.iter().map(|(_, a)| a.last().unwrap().as_str()).collect();
    assert_eq!(files, ["b.cpp", "a.cpp"]);
    assert_eq!(&cmds[0].1[..4], ["kiln", "-std=c++26", "-DX", "-DY"]);

    // A test's cdb.json, with @ROOT@ for the tree.
    let c = standard("cdb", TEST_TXT, &good_script(&hex(0x33)));
    c.write(
        "cdb.json",
        "[{\"directory\": \"@ROOT@\", \"command\": \"cc -I'@ROOT@/my inc' -c a.cpp\", \"file\": \"a.cpp\"}]\n",
    );
    assert_eq!(c.run(), Outcome::Pass);
    let t = c.tree();
    let db = std::fs::read_to_string(c.work().join("cdb.json")).unwrap();
    assert!(!db.contains("@ROOT@"));
    assert_eq!(
        read_commands(&db).unwrap(),
        [(
            t.clone(),
            vec!["cc".into(), format!("-I{}/my inc", t.display()), "-c".into(), "a.cpp".into()]
        )]
    );
}

// §6.3 step 3: each single difference is a FAIL naming it.
#[test]
fn s6_3_each_difference_fails() {
    let good = good_script(&hex(0x33));
    let main_line = format!("out state verified\nwrite a.cpp {}", esc(POST));
    let cases: Vec<(&str, String, &str)> = vec![
        (
            "status",
            good.replacen(&main_line, &format!("{main_line}\nexit 1"), 2),
            "exit status 1, expected 0",
        ),
        (
            "state",
            good.replace("out state verified\nwrite", "out state not_verified\nwrite"),
            "state not_verified, expected verified",
        ),
        (
            "line",
            good.replace("out round 1\n", "out round 2\n"),
            "output line 3: expected `round 1`, got `round 2`",
        ),
        (
            "extra",
            good.replace(&main_line, &format!("{main_line}\nout written 1 file(s)")),
            "output line 11: expected the end, got `written 1 file(s)`",
        ),
        (
            "different",
            good.replace(&esc(POST), &esc(&POST.replace("g()", "h()"))),
            "tree: file a.cpp differs (first at byte 18)",
        ),
        (
            "added",
            good.replace(&main_line, &format!("{main_line}\nwrite b.cpp new")),
            "tree: file b.cpp added",
        ),
        (
            "missing",
            good.replace(&main_line, &format!("{main_line}\nremove a.cpp")),
            "tree: file a.cpp missing",
        ),
    ];
    for (name, script, want) in cases {
        let c = standard(name, TEST_TXT, &script);
        let o = c.run();
        assert!(fail_message(&o).starts_with(want), "{name}: {o:?}");
    }
    // A state needs a result line; its absence shows standard error.
    let c = standard(
        "noresult",
        TEST_TXT,
        &good.replace(&format!("out result {}\n", hex(0x66)), "err slip: it broke\n"),
    );
    let o = c.run();
    assert!(
        fail_message(&o)
            .starts_with("the run printed no result line; standard error:\nslip: it broke"),
        "{o:?}"
    );
}

// §6.3 step 3: ids become <id>, the tree's path @ROOT@ and the store's
// @STORE@.
#[test]
fn s6_3_normalisation() {
    let tree = Path::new("/w/t/tree");
    let store = Path::new("/w/t/store");
    let id = hex(0xab);
    let out = format!(
        "edits {id}\nfile /w/t/tree/a.cpp\nmore (slip dump /w/t/store/x)\n{id}0 x{id}\nab12\n"
    );
    assert_eq!(
        normalise(&out, tree, store),
        format!("edits <id>\nfile @ROOT@/a.cpp\nmore (slip dump @STORE@/x)\n{id}0 x<id>\nab12\n")
    );
    // Through a run: the fake prints the tree's absolute path.
    let mut lines = main_out(&hex(0x33), "verified");
    lines.push("residue mention: @CWD@/a.cpp:1:1".into());
    let script = good_script(&hex(0x33)).replace(
        "out state verified\nwrite a.cpp",
        "out state verified\nout residue mention: @CWD@/a.cpp:1:1\nwrite a.cpp",
    );
    let c = standard("normalise", TEST_TXT, &script);
    c.write("expect.txt", &golden(&lines).replace("@CWD@", "@ROOT@"));
    assert_eq!(c.run(), Outcome::Pass);
    assert_eq!(
        first_line_difference("a\nb\n", "a\nc\n").unwrap(),
        "output line 2: expected `b`, got `c`"
    );
    assert_eq!(first_line_difference("a\n", "a\n"), None);
    // The ids are read from the lines of spec/edits.md §10.5.
    let l = Lines::read(&format!(
        "plan {a}\ntransform {a}\nround 3\nedits {a}\nstate accepted\ndeclared {b}: rules 1\n\
         overlay {b}: files 1\nresult {c}\nstate verified\nedits {b}\n",
        a = hex(1),
        b = hex(2),
        c = hex(3)
    ));
    assert_eq!(l.round.as_deref(), Some("3"));
    assert_eq!(l.edits, Some(hex(1)), "the first edits line");
    assert_eq!(l.declared, Some(hex(2)));
    assert_eq!(l.overlay, Some(hex(2)));
    assert_eq!(l.result, Some(hex(3)));
    assert_eq!(l.state.as_deref(), Some("verified"), "the last state line");
}

// §6.3: --bless rewrites expect.txt and post/ and not test.txt; it refuses
// when the state or the exit status differ.
#[test]
fn s6_3_bless() {
    let c =
        standard("bless", TEST_TXT, &good_script(&hex(0x33)).replace("out round 1", "out round 7"));
    c.write("expect.txt", "stale\n");
    c.write("post/a.cpp", "old post\n");
    let txt = c.read("test.txt").unwrap();
    assert_eq!(c.run_with(None, true), Outcome::Pass);
    let expect = golden(&main_out(&hex(0x33), "verified")).replace("round 1", "round 7");
    assert_eq!(c.read("expect.txt").unwrap(), expect);
    assert_eq!(c.read("post/a.cpp").unwrap(), POST);
    assert_eq!(c.read("test.txt").unwrap(), txt);
    // And the blessed test now passes without --bless.
    assert_eq!(c.run(), Outcome::Pass);
    // Without expect.txt, bless writes it; a plain run is an ERROR.
    std::fs::remove_file(c.dir().join("expect.txt")).unwrap();
    assert_eq!(c.run(), Outcome::Error("no expect.txt (`--bless` writes it)".into()));
    assert_eq!(c.run_with(None, true), Outcome::Pass);
    assert_eq!(c.read("expect.txt").unwrap(), expect);

    // A post/ file the run no longer changes is removed.
    let c = standard("bless-remove", TEST_TXT, &good_script(&hex(0x33)));
    c.write("pre/b.cpp", "b\n");
    c.write("post/b.cpp", "b changed\n");
    assert_eq!(c.run_with(None, true), Outcome::Pass);
    assert!(!c.dir().join("post/b.cpp").exists());
    assert_eq!(c.read("post/a.cpp").unwrap(), POST);

    for (name, script, want) in [
        (
            "bless-state",
            good_script(&hex(0x33))
                .replace("out state verified\nwrite", "out state not_verified\nwrite"),
            "cannot bless: state not_verified, expected verified",
        ),
        (
            "bless-status",
            good_script(&hex(0x33)).replacen(
                &format!("write a.cpp {}", esc(POST)),
                &format!("write a.cpp {}\nexit 3", esc(POST)),
                2,
            ),
            "cannot bless: exit status 3, expected 0",
        ),
        (
            "bless-added",
            good_script(&hex(0x33)).replace(
                &format!("write a.cpp {}", esc(POST)),
                &format!("write a.cpp {}\nwrite new.cpp x", esc(POST)),
            ),
            "cannot bless: post/ cannot hold the written tree: file new.cpp added",
        ),
    ] {
        let c = standard(name, TEST_TXT, &script);
        c.write("expect.txt", "stale\n");
        let o = c.run_with(None, true);
        assert_eq!(fail_message(&o), want);
        assert_eq!(c.read("expect.txt").unwrap(), "stale\n", "{name}: nothing rewritten");
        assert_eq!(c.read("post/a.cpp").unwrap(), POST);
    }
}

// §6.2, §6.3: a run that writes although its state does not permit it.
#[test]
fn s6_3_write_without_permission() {
    let txt = "spec: harness §6.3\nrun: rename --from a::f --to g\nstate: not_verified\nexit: 0\n\
               oracle: no-mutation none\n";
    let script =
        good_script(&hex(0x33)).replace("out state verified\n", "out state not_verified\n");
    let c = standard("unpermitted", txt, &script);
    std::fs::remove_dir_all(c.dir().join("post")).unwrap();
    c.write("expect.txt", &golden(&main_out(&hex(0x33), "not_verified")));
    let o = c.run();
    assert_eq!(
        fail_message(&o),
        "the tree was written, and the state `not_verified` does not permit writing: \
         file a.cpp differs (first at byte 18)"
    );
    // The same run without the write passes.
    let c = standard(
        "unpermitted-ok",
        txt,
        &script.replace(&format!("write a.cpp {}\n", esc(POST)), ""),
    );
    std::fs::remove_dir_all(c.dir().join("post")).unwrap();
    c.write("expect.txt", &golden(&main_out(&hex(0x33), "not_verified")));
    assert_eq!(c.run(), Outcome::Pass);
}

// §6: usage and input tests compare the status and the output; no state.
#[test]
fn s6_3_usage_and_input() {
    let txt = "spec: harness §6.3\nrun: rename --from a::f\nstate: usage\n";
    let c = Case::new("usage");
    c.write("test.txt", txt);
    c.write("pre/a.cpp", PRE);
    c.write("pre/fake.txt", "== rename *\nerr slip: --to is required\nexit 2\n");
    c.write("expect.txt", "");
    assert_eq!(c.run(), Outcome::Pass);
    // The determinism oracle ran; nothing else.
    assert_eq!(c.log("determinism/store").len(), 1);
    assert!(!c.work().join("back").exists());
    let c = Case::new("input");
    c.write("test.txt", &txt.replace("usage", "input"));
    c.write("pre/a.cpp", PRE);
    c.write("pre/fake.txt", "== rename *\nexit 2\n");
    c.write("expect.txt", "");
    assert_eq!(fail_message(&c.run()), "exit status 2, expected 3");
}

// §6: KILLED, never masked by xfail.
#[test]
fn s6_killed() {
    let txt = format!("{TEST_TXT}xfail: known\n");
    let c = standard("killed", &txt, "== rename *\nout plan x\nkill\n");
    match c.run() {
        Outcome::Killed(m) => assert!(m.contains("killed by signal 9"), "{m}"),
        o => panic!("{o:?}"),
    }
}

// §6: xfail turns FAIL into XFAIL and PASS into XPASS; an ERROR stays.
#[test]
fn s6_xfail() {
    let txt = format!("{TEST_TXT}xfail: the rename is not written yet\n");
    let c = standard("xfail", &txt, &good_script(&hex(0x33)).replace("out round 1", "out round 9"));
    assert_eq!(c.run(), Outcome::XFail);
    let c = standard("xpass", &txt, &good_script(&hex(0x33)));
    assert_eq!(c.run(), Outcome::XPass);
    let c = standard("xerror", &txt, &good_script(&hex(0x33)));
    std::fs::remove_file(c.dir().join("expect.txt")).unwrap();
    assert!(matches!(c.run(), Outcome::Error(_)));
}

// §6.3: the work directory must be <target>/transform/<area>/<name>; it is
// removed and made again for each run.
#[test]
fn s6_3_work_directory() {
    let c = standard("workdir", TEST_TXT, &good_script(&hex(0x33)));
    let t = c.test();
    for bad in [
        c.root.join("target/other/rename/workdir"),
        c.root.join("target/transform/rename/other"),
        c.root.join("target/transform/area/workdir"),
        c.root.join("target/transform/rename/../rename/workdir"),
        PathBuf::from("target/transform/rename/workdir"),
        c.dir().join("transform/rename/workdir"),
    ] {
        match run_one(&t, Path::new(FAKE), &bad, None, false) {
            Outcome::Error(m) => {
                assert!(m.starts_with("the work directory"), "{}: {m}", bad.display());
            }
            o => panic!("{}: {o:?}", bad.display()),
        }
    }
    assert!(!c.root.join("target/other").exists());
    // A stale file in the work directory is gone after a run.
    std::fs::create_dir_all(c.work()).unwrap();
    std::fs::write(c.work().join("stale"), "x").unwrap();
    assert_eq!(c.run(), Outcome::Pass);
    assert!(!c.work().join("stale").exists());
}

// ---------------------------------------------------------------- oracles (§6.4)

// §6.4 determinism: a different id or output with -j 1 fails; no-determinism.
#[test]
fn s6_4_determinism() {
    let good = good_script(&hex(0x33));
    let j1 = format!(
        "== rename * -j 1\n{}write a.cpp {}\n",
        outs(&main_out(&hex(0x77), "verified")),
        esc(POST)
    );
    let c = standard("det", TEST_TXT, &format!("{j1}{good}"));
    let o = c.run();
    let m = fail_message(&o);
    assert!(
        m.starts_with(&format!(
            "determinism: the edit set id is {} with -j 1, {} with -j 2",
            hex(0x77),
            hex(0x33)
        )),
        "{m}"
    );
    assert!(m.contains("determinism: with -j 1, output line 5"), "{m}");
    let j1 = format!(
        "== rename * -j 1\n{}out extra\nwrite a.cpp {}\n",
        outs(&main_out(&hex(0x33), "verified")),
        esc(POST)
    );
    let c = standard("det-out", TEST_TXT, &format!("{j1}{good}"));
    assert_eq!(
        fail_message(&c.run()),
        "determinism: with -j 1, output line 11: expected the end, got `extra`"
    );
    let j1 = format!("== rename * -j 1\n{}exit 1\n", outs(&main_out(&hex(0x33), "verified")));
    let c = standard("det-exit", TEST_TXT, &format!("{j1}{good}"));
    assert_eq!(fail_message(&c.run()), "determinism: exit status 1 with -j 1, 0 with -j 2");
    // Each store's path is the same `@STORE@`.
    let line = "out more (slip dump @STORE@/edits/x.edts)\n";
    let c = standard(
        "det-store",
        TEST_TXT,
        &good.replace("out round 1\n", &format!("out round 1\n{line}")),
    );
    let mut lines = main_out(&hex(0x33), "verified");
    lines.insert(3, "more (slip dump @STORE@/edits/x.edts)".into());
    c.write("expect.txt", &golden(&lines));
    assert_eq!(c.run(), Outcome::Pass);
    // Turned off.
    let c = standard(
        "det-off",
        &format!("{TEST_TXT}oracle: no-determinism j1 is known to differ\n"),
        &format!("{j1}{good}"),
    );
    assert_eq!(c.run(), Outcome::Pass);
    assert!(c.log("determinism/store").is_empty());
}

// §6.4 verify: slip check over the run's own edit set gives the same
// result id; no-verify; it does not run for other states.
#[test]
fn s6_4_verify() {
    let good = good_script(&hex(0x33));
    let c = standard(
        "verify",
        TEST_TXT,
        &good.replace(
            &format!("== check 0\nout result {}", hex(0x66)),
            &format!("== check 0\nout result {}", hex(0x67)),
        ),
    );
    assert_eq!(
        fail_message(&c.run()),
        format!("verify: slip check gives result {}, the run {}", hex(0x67), hex(0x66))
    );
    let c = standard(
        "verify-none",
        TEST_TXT,
        &good.replace(
            &format!("== check 0\nout result {}\n", hex(0x66)),
            "== check 0\nerr no\nexit 3\n",
        ),
    );
    assert_eq!(fail_message(&c.run()), "verify: slip check printed no result line (exit 3): no");
    let c = standard(
        "verify-off",
        &format!("{TEST_TXT}oracle: no-verify none\n"),
        &good.replace("== check 0", "== check 9"),
    );
    assert_eq!(c.run(), Outcome::Pass);
    assert_eq!(c.log("store").len(), 1, "no check ran");
    // A not_verified test runs no check.
    let txt = "spec: harness §6.4\nrun: rename --from a::f --to g\nstate: not_verified\nexit: 0\noracle: no-mutation none\n";
    let c = standard(
        "verify-nv",
        txt,
        &good
            .replace("out state verified\n", "out state not_verified\n")
            .replace(&format!("write a.cpp {}\n", esc(POST)), ""),
    );
    std::fs::remove_dir_all(c.dir().join("post")).unwrap();
    c.write("expect.txt", &golden(&main_out(&hex(0x33), "not_verified")));
    assert_eq!(c.run(), Outcome::Pass);
    assert_eq!(c.log("store").len(), 1);
}

// §6.4 back: the inverse ends in the same state and leaves pre/; no-back.
#[test]
fn s6_4_back() {
    let good = good_script(&hex(0x33));
    let first = format!("write a.cpp {}\n== rename *\n", esc(PRE));
    let c = standard(
        "back",
        TEST_TXT,
        &good.replacen(&first, &format!("write a.cpp {}\n== rename *\n", esc("int y;\n")), 1),
    );
    assert_eq!(
        fail_message(&c.run()),
        "back: the tree is not pre/ again: file a.cpp differs (first at byte 0)"
    );
    let c = standard(
        "back-state",
        TEST_TXT,
        &good.replacen("out state verified\n", "out state not_verified\n", 1),
    );
    assert_eq!(
        fail_message(&c.run()),
        "back: `rename --from a::g --to f` ends in state not_verified, the run in verified"
    );
    let c = standard(
        "back-off",
        &format!("{TEST_TXT}oracle: no-back the inverse is ambiguous\n"),
        &good.replacen(&first, "exit 1\n== rename *\n", 1),
    );
    assert_eq!(c.run(), Outcome::Pass);
    assert!(c.log("back/store").is_empty());
    // A run that has no inverse cannot be judged.
    let c = standard(
        "back-none",
        &TEST_TXT.replace("--from a::f --to g", "--from a::f --to b::g"),
        &good,
    );
    assert!(
        matches!(c.run(), Outcome::Error(m) if m.starts_with("the back oracle has no inverse"))
    );
    assert_eq!(
        cmdline::inverse(
            &["rename", "--from", "::a::b::f", "--to", "g", "--kind", "function"].map(String::from)
        ),
        Some(
            ["rename", "--from", "::a::b::g", "--to", "f", "--kind", "function"]
                .map(String::from)
                .to_vec()
        )
    );
    assert_eq!(
        cmdline::inverse(&["rename", "--from", "f", "--to", "g"].map(String::from)),
        Some(["rename", "--from", "g", "--to", "f"].map(String::from).to_vec())
    );
    assert_eq!(cmdline::inverse(&["rename", "--from", "f"].map(String::from)), None);
    assert_eq!(
        cmdline::inverse(&["rename", "--from", "f", "--to", "g", "--to", "h"].map(String::from)),
        None
    );
}

/// A stand-in clang that logs `<dir> <args>` and rejects a main file that
/// holds `CLANG_REJECTS`.
fn fake_clang(c: &Case) -> PathBuf {
    let p = c.root.join("bin/clang");
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    let log = c.root.join("clang.log");
    std::fs::write(
        &p,
        format!(
            "#!/bin/sh\n[ \"$1\" = --probe ] && exit 0\necho \"$(pwd -P) $*\" >> '{}'\n\
             prev=; for a in \"$@\"; do [ \"$prev\" = -c ] && f=$a; prev=$a; done\n\
             if grep -q CLANG_REJECTS \"$f\"; then echo \"$f: rejected\" >&2; exit 1; fi\nexit 0\n",
            log.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    common::wait_executable(&p);
    p
}

// §6.4 clang: on the pre-image (ERROR) and on the written tree (FAIL),
// with the compiler replaced and -fsyntax-only added; skipped and counted
// without one; no-clang.
#[test]
fn s6_4_clang() {
    let c = standard("clang", TEST_TXT, &good_script(&hex(0x33)));
    let clang = fake_clang(&c);
    let r = run_test(&c.test(), Path::new(FAKE), &c.work(), Some(&clang), false);
    assert_eq!(r, xtask::transform::Report { outcome: Outcome::Pass, clang_skipped: false });
    let t = c.tree();
    let t = t.display();
    let line = format!("{t} -std=c++20 -I{t} -I{t}/dep -c a.cpp -fsyntax-only");
    assert_eq!(
        std::fs::read_to_string(c.root.join("clang.log")).unwrap(),
        format!("{line}\n{line}\n")
    );

    let c = standard("clang-pre", TEST_TXT, &good_script(&hex(0x33)));
    c.write("pre/a.cpp", &format!("{PRE}// CLANG_REJECTS\n"));
    c.write("post/a.cpp", &format!("{POST}// x\n"));
    let clang = fake_clang(&c);
    match c.run_with(Some(&clang), false) {
        Outcome::Error(m) => {
            assert!(m.starts_with("the pre-image is not accepted by clang: `"), "{m}");
        }
        o => panic!("{o:?}"),
    }
    assert!(c.log("store").is_empty(), "the run never started");
    // An ERROR is never blessed away.
    c.write("expect.txt", "stale\n");
    assert!(matches!(c.run_with(Some(&clang), true), Outcome::Error(_)));
    assert_eq!(c.read("expect.txt").unwrap(), "stale\n");

    let post = format!("{POST}// CLANG_REJECTS\n");
    let c =
        standard("clang-post", TEST_TXT, &good_script(&hex(0x33)).replace(&esc(POST), &esc(&post)));
    c.write("post/a.cpp", &post);
    let clang = fake_clang(&c);
    let o = c.run_with(Some(&clang), false);
    assert!(fail_message(&o).starts_with("clang: the written tree is not accepted: `"), "{o:?}");
    assert!(fail_message(&o).contains("a.cpp: rejected"), "{o:?}");
    // Turned off: no clang runs, nothing is counted.
    let c = standard(
        "clang-off",
        &format!("{TEST_TXT}oracle: no-clang kiln extension\n"),
        &good_script(&hex(0x33)),
    );
    let clang = fake_clang(&c);
    let r = run_test(&c.test(), Path::new(FAKE), &c.work(), Some(&clang), false);
    assert_eq!(r, xtask::transform::Report { outcome: Outcome::Pass, clang_skipped: false });
    let r = run_test(&c.test(), Path::new(FAKE), &c.work(), None, false);
    assert!(!r.clang_skipped);
    assert!(!c.root.join("clang.log").exists());
}

// §6.4: the compiler is $SLIP_CLANG, else clang++-19, else clang++ on PATH.
#[test]
fn s6_4_find_clang() {
    let f = Fixture::empty("findclang");
    let (a, b) = (f.path("a"), f.path("b"));
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();
    std::fs::write(b.join("clang++"), "").unwrap();
    let path = std::env::join_paths([&a, &b]).unwrap();
    let env = |vars: Vec<(&'static str, std::ffi::OsString)>| {
        move |n: &str| vars.iter().find(|(k, _)| *k == n).map(|(_, v)| v.clone())
    };
    assert_eq!(find_clang(&env(vec![("PATH", path.clone())])), Some(b.join("clang++")));
    std::fs::write(a.join("clang++-19"), "").unwrap();
    assert_eq!(find_clang(&env(vec![("PATH", path.clone())])), Some(a.join("clang++-19")));
    std::fs::write(b.join("clang++-19"), "").unwrap();
    std::fs::remove_file(a.join("clang++-19")).unwrap();
    assert_eq!(find_clang(&env(vec![("PATH", path.clone())])), Some(b.join("clang++-19")));
    assert_eq!(
        find_clang(&env(vec![("PATH", path.clone()), ("SLIP_CLANG", "/x/clang".into())])),
        Some(PathBuf::from("/x/clang"))
    );
    assert_eq!(
        find_clang(&env(vec![("PATH", path), ("SLIP_CLANG", "".into())])),
        Some(b.join("clang++-19"))
    );
    assert_eq!(find_clang(&env(vec![("PATH", f.path("none").into_os_string())])), None);
    assert_eq!(find_clang(&env(vec![])), None);
}

// ---------------------------------------------------------------- mutation (§6.4)

fn file(path: &[u8], size: u64) -> FileRef {
    FileRef { path: path.to_vec(), size, content: d(0x99) }
}

fn spelled(key: u8, text: &str) -> Piece {
    Piece::Spelled { key: d(key), text: text.as_bytes().to_vec() }
}

fn edit(file: u64, lo: u64, hi: u64, pieces: Vec<Piece>) -> Edit {
    Edit {
        file,
        lo,
        hi,
        pieces,
        subjects: Vec::new(),
        tiers: 1,
        macro_site: None,
        notes: Vec::new(),
        observers: 1,
        votes: 1,
        first: vec![d(0xa0)],
    }
}

fn empty_set() -> EditSet {
    EditSet {
        scheme: 1,
        plan: d(0x11),
        round: 1,
        transform: d(0x22),
        basis: d(0x03),
        state: SetState::Accepted,
        files: Vec::new(),
        subjects: Vec::new(),
        rules: Vec::new(),
        edits: Vec::new(),
        groups: Vec::new(),
        changed: Vec::new(),
        kept: Vec::new(),
        unresolved: Vec::new(),
        refusals: Vec::new(),
        reports: Vec::new(),
        failures: Vec::new(),
        unparsed: Vec::new(),
        counters: Vec::new(),
    }
}

/// `n` edits of `/s/a.cc` at `10i..10i+3`, `kept` kept ranges at
/// `5000+10k..+3`, and `spelled` spelled pieces, two to an edit from the
/// first.
fn synthetic(n: u64, kept: u64, spelled_pieces: u64) -> EditSet {
    let mut s = empty_set();
    s.files = vec![file(b"/s/a.cc", 10_000)];
    for i in 0..n {
        let mut pieces = vec![Piece::Literal(b"x".to_vec())];
        for k in 0..2 {
            if 2 * i + k < spelled_pieces {
                pieces.push(spelled((2 * i + k) as u8, "g"));
            }
        }
        s.edits.push(edit(0, 10 * i, 10 * i + 3, pieces));
    }
    s.kept = (0..kept).map(|k| Place { file: 0, lo: 5000 + 10 * k, hi: 5003 + 10 * k }).collect();
    assert_eq!(s.validate(), Ok(()));
    s
}

// §6.4: the mutated sets, their count and labels, in the order of §6.4.
#[test]
fn s6_4_mutation_sample() {
    let labels = |s: &EditSet| mutations(s).into_iter().map(|(l, _)| l).collect::<Vec<_>>();
    assert_eq!(labels(&synthetic(1, 0, 0)), ["drop edit 0"]);
    assert_eq!(
        labels(&synthetic(1, 1, 1)),
        ["drop edit 0", "add edit at kept 0", "change key of edit 0 piece 1"]
    );
    let eight = labels(&synthetic(8, 0, 0));
    assert_eq!(eight, (0..8).map(|i| format!("drop edit {i}")).collect::<Vec<_>>());
    let twenty = labels(&synthetic(20, 6, 6));
    let mut want: Vec<String> =
        [0, 2, 5, 7, 10, 12, 15, 17].iter().map(|i| format!("drop edit {i}")).collect();
    want.extend((0..4).map(|k| format!("add edit at kept {k}")));
    want.extend(
        [
            "change key of edit 0 piece 1",
            "change key of edit 0 piece 2",
            "change key of edit 1 piece 1",
            "change key of edit 1 piece 2",
        ]
        .map(String::from),
    );
    assert_eq!(twenty, want);
    assert_eq!(mutate::dropped_indices(100), [0, 12, 25, 37, 50, 62, 75, 87]);
    assert_eq!(mutate::dropped_indices(9), [0, 1, 2, 3, 4, 5, 6, 7]);
    assert_eq!(labels(&synthetic(2, 1, 0)).len(), 3);
    assert!(labels(&synthetic(1, 6, 0)).iter().filter(|l| l.starts_with("add")).count() == 4);
    // Every mutated set validates as EDTS and reads back.
    for s in [synthetic(1, 1, 1), synthetic(8, 6, 6), synthetic(20, 6, 6)] {
        for (label, m) in mutations(&s) {
            assert_eq!(m.validate(), Ok(()), "{label}");
            let bytes = m.to_bytes();
            assert_eq!(EditSet::from_bytes(&bytes).unwrap(), m, "{label}");
            assert!(formats::content_id(&bytes).is_ok());
        }
    }
}

// §6.4: the added edit has the pieces of the first edit at the kept range;
// a changed key is the bitwise complement and nothing else differs; a
// dropped edit is gone, the only one leaves a refused set.
#[test]
fn s6_4_mutation_shapes() {
    let mut s = synthetic(3, 1, 2);
    // The first edit is of a macro's invocation and carries a note: the
    // added edit takes the note and not the invocation, which is of the
    // first edit's place.
    s.edits[0].tiers = 3;
    s.edits[0].macro_site = Some(MacroSite { name_start: 0, name: b"M".to_vec() });
    s.edits[0].notes = vec![b"n".to_vec()];
    assert_eq!(s.validate(), Ok(()));
    let all = mutate::all(&s);
    let add = all.iter().find(|m| m.label == "add edit at kept 0").unwrap();
    let added = add.set.edits.iter().find(|e| (e.lo, e.hi) == (5000, 5003)).unwrap();
    assert_eq!(added.pieces, s.edits[0].pieces);
    assert_eq!((added.tiers, &added.notes), (3, &s.edits[0].notes));
    assert_eq!(added.macro_site, None);
    assert_eq!(add.set.edits.len(), 4);
    assert_eq!(add.set.edits[3], *added, "sorted by range");
    let key = all.iter().find(|m| m.label == "change key of edit 0 piece 2").unwrap();
    let mut back = key.set.clone();
    if let Piece::Spelled { key, .. } = &mut back.edits[0].pieces[2] {
        assert_eq!(key.0, [!1u8; 32]);
        *key = d(1);
    }
    assert_eq!(back, s, "only that key differs");
    let drop = all.iter().find(|m| m.label == "drop edit 1").unwrap();
    assert_eq!(drop.set.edits.len(), 2);
    assert_eq!((drop.set.edits[1].lo, drop.set.state), (20, SetState::Accepted));
    assert_eq!(
        drop.kind,
        mutate::Kind::Dropped { index: 1, file: b"/s/a.cc".to_vec(), lo: 10, hi: 13 }
    );
    let one = mutate::all(&synthetic(1, 0, 0));
    assert_eq!(one[0].set.state, SetState::Refused);
    assert_eq!(one[0].set.validate(), Ok(()));

    // A kept range at which no edit can stand (crossing an edit, or empty)
    // is passed over.
    let mut s = synthetic(1, 0, 0);
    s.kept = vec![
        Place { file: 0, lo: 0, hi: 3 },
        Place { file: 0, lo: 1, hi: 2 },
        Place { file: 0, lo: 2, hi: 5 },
        Place { file: 0, lo: 7, hi: 7 },
    ];
    assert_eq!(s.validate(), Ok(()));
    let labels: Vec<String> = mutations(&s).into_iter().map(|(l, _)| l).collect();
    assert_eq!(labels, ["drop edit 0", "add edit at kept 1"]);

    // A dropped edit takes its file, groups and in_force with it.
    let mut s = synthetic(3, 0, 0);
    s.files.push(file(b"/s/b.cc", 100));
    s.edits.push(edit(1, 0, 4, vec![Piece::Literal(b"y".to_vec())]));
    s.groups = vec![vec![0, 3], vec![1, 2]];
    s.changed = vec![slip_model::editset::Exempt {
        key: d(5),
        place: Place { file: 1, lo: 1, hi: 2 },
        reason: b"r".to_vec(),
        in_force: true,
        observers: 1,
        first: vec![],
    }];
    s.kept = vec![Place { file: 1, lo: 50, hi: 51 }];
    assert_eq!(s.validate(), Ok(()));
    let m = mutate::all(&s);
    let d3 = m.iter().find(|m| m.label == "drop edit 3").unwrap();
    assert_eq!(d3.set.validate(), Ok(()));
    assert_eq!(d3.set.groups, [vec![1, 2]]);
    assert!(!d3.set.changed[0].in_force);
    let d0 = m.iter().find(|m| m.label == "drop edit 0").unwrap();
    assert_eq!(d0.set.groups, [vec![0, 1]]);
    assert_eq!(d0.set.validate(), Ok(()));
    // Only the edit of b.cc and a kept range of it: dropping the edit keeps
    // the file (the kept range names it); without the kept range it goes.
    s.kept.clear();
    s.changed.clear();
    let d3 = mutate::all(&s).into_iter().find(|m| m.label == "drop edit 3").unwrap();
    assert_eq!(d3.set.files.len(), 1);
    assert_eq!(d3.set.validate(), Ok(()));
}

/// An accepted edit set over the tree's `a.cpp`: edits at the two `f`s,
/// spelled pieces, and the `a` of `a::f` kept.
fn tree_set(c: &Case) -> EditSet {
    let path = c.tree().join("a.cpp");
    let mut s = empty_set();
    s.files = vec![FileRef {
        path: path.to_string_lossy().as_bytes().to_vec(),
        size: PRE.len() as u64,
        content: content_digest(PRE.as_bytes()),
    }];
    s.edits =
        vec![edit(0, 18, 19, vec![spelled(0x10, "g")]), edit(0, 36, 37, vec![spelled(0x10, "g")])];
    s.kept = vec![Place { file: 0, lo: 33, hi: 34 }];
    assert_eq!(s.validate(), Ok(()));
    s
}

fn result_bytes(state: State, residue: Vec<Residue>, failed_spellings: usize) -> Vec<u8> {
    let refused = state == State::Refused;
    let r = TransformResult {
        scheme: 1,
        plan: d(0x11),
        round: 1,
        transform: d(0x22),
        edits: d(0x33),
        declared: (!refused).then_some(d(0x44)),
        overlay: (!refused).then_some(d(0x55)),
        state,
        partial: false,
        allow_unparsed: false,
        verdict: (!refused).then_some(RunVerdict::NotVerified),
        tally: Tally { selected: 1, ..Tally::default() },
        agreement: Vec::new(),
        unused: Vec::new(),
        spellings: result::Spellings {
            obligations: 2,
            passed: 2 - failed_spellings as u64,
            failed: (0..failed_spellings)
                .map(|i| result::FailedSpelling {
                    obligation: i as u64,
                    why: result::Why::DenotesOther,
                    observers: 1,
                    first: Some(d(0xa0)),
                    found: Some(d(0x12)),
                    name: "g".into(),
                })
                .collect(),
        },
        residue,
        exempt: Vec::new(),
    };
    assert_eq!(r.validate(), Ok(()));
    r.to_bytes()
}

fn residue(c: &Case, category: Category, lo: u64, hi: u64) -> Residue {
    Residue {
        category,
        file: c.tree().join("a.cpp").to_string_lossy().as_bytes().to_vec(),
        lo,
        hi,
        line: 1,
        column: lo + 1,
    }
}

/// A check section that publishes a result of `state` and prints it.
fn check_section(c: &Case, n: usize, state: State, residue: Vec<Residue>, failed: usize) -> String {
    let bytes = result_bytes(state, residue, failed);
    let id = formats::content_id(&bytes).unwrap();
    let p = c.object(&format!("r{n}.rslt"), &bytes);
    let name = match state {
        State::Verified => "verified",
        State::VerifiedWithResidue => "verified_with_residue",
        State::NotVerified => "not_verified",
        State::Incomplete => "incomplete",
        State::Refused => "refused",
    };
    format!("== check {n}\npublish result {}\nout result {id}\nout state {name}\n", p.display())
}

/// The mutation case: the run publishes `tree_set` and answers checks
/// 1 to 5 (drop 0, drop 1, add at kept 0, key 0, key 1) with `answers`.
fn mutation_case(name: &str, answers: &[Answer]) -> (Case, EditSet) {
    let c = Case::new(name);
    let set = tree_set(&c);
    let bytes = set.to_bytes();
    let id = formats::content_id(&bytes).unwrap().to_string();
    let p = c.object("set.edts", &bytes);
    let mut script = good_script(&id).replace(
        "== rename *\nout plan",
        &format!("== rename *\npublish edits {}\nout plan", p.display()),
    );
    for (i, (state, res, failed)) in answers.iter().enumerate() {
        let res = res.iter().map(|(cat, lo, hi)| residue(&c, *cat, *lo, *hi)).collect();
        script.push_str(&check_section(&c, i + 1, *state, res, *failed));
    }
    c.write("test.txt", &TEST_TXT.replace("oracle: no-mutation the fake has no edit set\n", ""));
    c.write("pre/a.cpp", PRE);
    c.write("pre/fake.txt", &script);
    c.write("post/a.cpp", POST);
    c.write("expect.txt", &golden(&main_out(&id, "verified")).replace(&id, "<id>"));
    (c, set)
}

/// A check's answer: the state, the residue `(category, lo, hi)` and the
/// number of failed spelling obligations.
type Answer = (State, Vec<(Category, u64, u64)>, usize);

fn good_answers() -> Vec<Answer> {
    vec![
        (State::VerifiedWithResidue, vec![(Category::NotCompiled, 18, 19)], 0),
        (State::NotVerified, vec![], 0),
        (State::NotVerified, vec![], 0),
        (State::NotVerified, vec![], 1),
        (State::NotVerified, vec![], 1),
    ]
}

// §6.4 mutation: every mutation rejected passes; the checks were given
// the mutated sets, published in the store, with the original's declared.
#[test]
fn s6_4_mutation_passes() {
    let (c, set) = mutation_case("mut-pass", &good_answers());
    assert_eq!(c.run(), Outcome::Pass);
    let log = c.log("store");
    assert_eq!(log.len(), 7, "{log:#?}");
    let store = Store::open(&c.work().join("store"), Access::Shared).unwrap();
    for (i, (label, m)) in mutations(&set).into_iter().enumerate() {
        let id = formats::content_id(&m.to_bytes()).unwrap();
        let path = store.object_path(Object::EditSet, &id);
        let w = c.work();
        assert_eq!(
            log[i + 2],
            format!(
                "check --plan {} --round 1 --edits {} --declared {} --store {}/store -j 2",
                hex(0x11),
                path.display(),
                hex(0x44),
                w.display()
            ),
            "{label}"
        );
        assert!(path.is_file());
    }
}

// §6.4 mutation: a mutation reported `verified` fails with its label;
// `verified_with_residue` passes for a dropped edit only with the dropped
// range in the residue (not a mention); it fails for an added edit and a
// changed key; a changed key needs a failed spelling obligation.
#[test]
fn s6_4_mutation_failures() {
    type A = Vec<(State, Vec<(Category, u64, u64)>, usize)>;
    let with = |i: usize, a: (State, Vec<(Category, u64, u64)>, usize)| -> A {
        let mut v = good_answers();
        v[i] = a;
        v
    };
    let cases: Vec<(&str, A, &str)> = vec![
        (
            "mut-verified",
            with(1, (State::Verified, vec![], 0)),
            "mutation `drop edit 1`: verified: an edit was lost or invented and nothing noticed",
        ),
        (
            "mut-elsewhere",
            with(0, (State::VerifiedWithResidue, vec![(Category::NotCompiled, 36, 37)], 0)),
            "mutation `drop edit 0`: verified_with_residue without the dropped range",
        ),
        (
            "mut-mention",
            with(0, (State::VerifiedWithResidue, vec![(Category::Mention, 18, 19)], 0)),
            "mutation `drop edit 0`: verified_with_residue without the dropped range",
        ),
        (
            "mut-wider",
            with(0, (State::VerifiedWithResidue, vec![(Category::Dependent, 17, 19)], 0)),
            "mutation `drop edit 0`: verified_with_residue without the dropped range",
        ),
        (
            "mut-add",
            with(2, (State::VerifiedWithResidue, vec![(Category::NotCompiled, 33, 34)], 0)),
            "mutation `add edit at kept 0`: verified_with_residue, expected not_verified",
        ),
        (
            "mut-key",
            with(3, (State::VerifiedWithResidue, vec![(Category::NotCompiled, 18, 19)], 0)),
            "mutation `change key of edit 0 piece 0`: verified_with_residue, expected not_verified",
        ),
        (
            "mut-key-spelling",
            with(4, (State::NotVerified, vec![], 0)),
            "mutation `change key of edit 1 piece 0`: not_verified, but by no failed spelling obligation",
        ),
        (
            "mut-incomplete",
            with(1, (State::Incomplete, vec![], 0)),
            "mutation `drop edit 1`: incomplete, expected not_verified or verified_with_residue",
        ),
        (
            "mut-key-verified",
            with(4, (State::Verified, vec![], 0)),
            "mutation `change key of edit 1 piece 0`: verified: an edit was lost",
        ),
    ];
    for (name, answers, want) in cases {
        let (c, _) = mutation_case(name, &answers);
        let o = c.run();
        assert!(fail_message(&o).starts_with(want), "{name}: {o:?}");
    }
    // The drop with the residue inside the dropped range passes (shown by
    // good_answers); a printed state other than the record's fails.
    let (c, _) = mutation_case("mut-printed", &good_answers());
    let script = c.read("pre/fake.txt").unwrap();
    c.write(
        "pre/fake.txt",
        &script.replacen("out state verified_with_residue\n", "out state not_verified\n", 1),
    );
    assert!(fail_message(&c.run()).starts_with(
        "mutation `drop edit 0`: slip check printed state not_verified, its result record says verified_with_residue"
    ));
    // No result line.
    let (c, _) = mutation_case("mut-noresult", &good_answers());
    let script = c.read("pre/fake.txt").unwrap();
    let cut = script.find("== check 2").unwrap();
    c.write(
        "pre/fake.txt",
        &format!("{}== check 2\nerr refused\nexit 3\n{}", &script[..cut], &script[cut..]),
    );
    assert!(fail_message(&c.run()).starts_with(
        "mutation `drop edit 1`: slip check printed no result line (exit 3): refused"
    ));
    // Turned off.
    let (c, _) = mutation_case("mut-off", &with(1, (State::Verified, vec![], 0)));
    c.write(
        "test.txt",
        &TEST_TXT.replace("the fake has no edit set", "the dropped edit changes nothing"),
    );
    assert_eq!(c.run(), Outcome::Pass);
    assert_eq!(c.log("store").len(), 2);
}

// §6.4 mutation: the drop of the only edit makes a refused set, which must
// end `refused`; the mutation oracle needs the edit set in the store.
#[test]
fn s6_4_mutation_single_edit() {
    let c = Case::new("mut-one");
    let mut set = tree_set(&c);
    set.edits.truncate(1);
    set.kept.clear();
    let bytes = set.to_bytes();
    let id = formats::content_id(&bytes).unwrap().to_string();
    let p = c.object("set.edts", &bytes);
    let base = good_script(&id).replace(
        "== rename *\nout plan",
        &format!("== rename *\npublish edits {}\nout plan", p.display()),
    );
    let script = format!(
        "{base}{}{}",
        check_section(&c, 1, State::Refused, vec![], 0),
        check_section(&c, 2, State::NotVerified, vec![], 1)
    );
    c.write("test.txt", &TEST_TXT.replace("oracle: no-mutation the fake has no edit set\n", ""));
    c.write("pre/a.cpp", PRE);
    c.write("pre/fake.txt", &script);
    c.write("post/a.cpp", POST);
    c.write("expect.txt", &golden(&main_out(&id, "verified")).replace(&id, "<id>"));
    assert_eq!(c.run(), Outcome::Pass);
    let script2 = script.replacen("out state refused", "out state not_verified", 1);
    let bytes = result_bytes(State::NotVerified, vec![], 0);
    let rid = formats::content_id(&bytes).unwrap();
    let p2 = c.object("nv.rslt", &bytes);
    let cut = script2.find("== check 1").unwrap();
    c.write(
        "pre/fake.txt",
        &format!(
            "{}== check 1\npublish result {}\nout result {rid}\nout state not_verified\n{}",
            &script2[..cut],
            p2.display(),
            &script2[cut..]
        ),
    );
    assert!(
        fail_message(&c.run())
            .starts_with("mutation `drop edit 0`: not_verified, expected refused")
    );
    // An edits line naming no edit set of the store.
    let c = standard(
        "mut-missing",
        &TEST_TXT.replace("oracle: no-mutation the fake has no edit set\n", ""),
        &good_script(&hex(0x33)),
    );
    assert_eq!(
        fail_message(&c.run()),
        format!("mutation: the store has no edit set {}", hex(0x33))
    );
}

// ---------------------------------------------------------------- the command (§6.3)

/// Runs `xtask transform` over the fixture of `c` (its root through
/// `CARGO_MANIFEST_DIR`), with no clang on `PATH`.
fn xtask(root: &Path, args: &[&str]) -> (Option<i32>, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("transform")
        .args(args)
        .env("CARGO_MANIFEST_DIR", root.join("xtask"))
        .env("CARGO_TARGET_DIR", root.join("target"))
        .env("PATH", root.join("no-such-bin"))
        .env_remove("SLIP_CLANG")
        .env_remove("SLIP_REQUIRE_ALL")
        .output()
        .unwrap();
    (
        out.status.code(),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

// §6.3: the outcome lines, the summary, the clang skips and the exit
// status; a failing test's directory is kept and printed, a passing one's
// removed; --only.
#[test]
fn s6_3_the_command() {
    let a = standard("a-pass", TEST_TXT, &good_script(&hex(0x33)));
    let root = a.root.clone();
    let dir = root.join("tests/transform/rename");
    // More tests in the same fixture.
    let add = |name: &str, txt: &str, script: &str| {
        let src = a.dir();
        let dst = dir.join(name);
        for f in ["test.txt", "pre/a.cpp", "post/a.cpp", "expect.txt"] {
            std::fs::create_dir_all(dst.join(f).parent().unwrap()).unwrap();
            std::fs::copy(src.join(f), dst.join(f)).unwrap();
        }
        std::fs::write(dst.join("test.txt"), txt).unwrap();
        std::fs::write(dst.join("pre/fake.txt"), script).unwrap();
    };
    let good = good_script(&hex(0x33));
    let off = format!("{TEST_TXT}oracle: no-clang none\n");
    add("b-fail", &off, &good.replace("out round 1", "out round 2"));
    add("c-xfail", &format!("{off}xfail: known\n"), &good.replace("out round 1", "out round 2"));
    add("d-xpass", &format!("{off}xfail: fixed\n"), &good);
    std::fs::create_dir_all(dir.join("e-error")).unwrap();

    let (code, out, _) = xtask(&root, &["--slip", FAKE]);
    let work = root.join("target/transform/rename");
    assert_eq!(code, Some(1));
    assert_eq!(
        out,
        format!(
            "FAIL rename/b-fail: output line 3: expected `round 1`, got `round 2`\n  kept {w}/b-fail\n\
             XFAIL rename/c-xfail: xfail: known\n\
             XPASS rename/d-xpass: xfail: fixed\n  kept {w}/d-xpass\n\
             ERROR rename/e-error: no test.txt\n\
             transform: 1 passed, 1 failed, 1 xfail, 1 xpass, 1 error\n\
             clang oracle skipped: 1 (no compiler: $SLIP_CLANG, clang++-19 and clang++ not found)\n",
            w = work.display()
        )
    );
    assert!(!work.join("a-pass").exists() && !work.join("c-xfail").exists());
    assert!(work.join("b-fail/tree/a.cpp").is_file());

    // --only, and the clang skip alone failing the gate unless --no-clang.
    let (code, out, _) = xtask(&root, &["--slip", FAKE, "--only", "a-pass", "-j", "1"]);
    assert_eq!(
        (code, out.as_str()),
        (
            Some(1),
            "transform: 1 passed, 0 failed\nclang oracle skipped: 1 (no compiler: $SLIP_CLANG, clang++-19 and clang++ not found)\n"
        )
    );
    let (code, out, _) = xtask(&root, &["--slip", FAKE, "--only", "a-pass", "--no-clang"]);
    assert_eq!(
        (code, out.as_str()),
        (Some(0), "transform: 1 passed, 0 failed\nclang oracle skipped: 1 (--no-clang)\n")
    );
    let (code, out, _) = xtask(&root, &["--slip", FAKE, "--only", "c-x", "--only", "d-x"]);
    assert_eq!(code, Some(1));
    assert!(out.ends_with("transform: 0 passed, 0 failed, 1 xfail, 1 xpass\n"), "{out}");
    let (code, out, _) = xtask(&root, &["--slip", FAKE, "--only", "c-x"]);
    assert_eq!(
        (code, out.as_str()),
        (Some(0), "XFAIL rename/c-xfail: xfail: known\ntransform: 0 passed, 0 failed, 1 xfail\n")
    );
    // Nothing selected: nothing built, nothing run.
    let (code, out, _) = xtask(&root, &["--only", "nothing"]);
    assert_eq!((code, out.as_str()), (Some(0), "transform: 0 passed, 0 failed\n"));
}

// §6.3: the command line.
#[test]
fn s6_3_usage() {
    let o = parse_args(
        &["--only", "a", "--only", "b", "--bless", "-j", "3", "--no-clang", "--slip", "/s"]
            .map(String::from),
    )
    .unwrap();
    assert_eq!(o.only, ["a", "b"]);
    assert!(o.bless && o.no_clang);
    assert_eq!((o.jobs, o.slip.as_deref()), (Some(3), Some(Path::new("/s"))));
    for (args, want) in [
        (&["-j"][..], "`-j` needs a value"),
        (&["-j", "0"], "-j: `0` is not a positive integer"),
        (&["-j", "+1"], "-j: `+1` is not a positive integer"),
        (&["-j", "1", "-j", "2"], "`-j` given twice"),
        (&["--slip", "a", "--slip", "b"], "`--slip` given twice or empty"),
        (&["--only"], "`--only` needs a value"),
        (&["--frob"], "unexpected argument `--frob`"),
    ] {
        assert_eq!(
            parse_args(&args.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>()).unwrap_err(),
            want
        );
    }
    let f = Fixture::empty("transform-usage");
    let (code, _, err) = xtask(f.root(), &["--frob"]);
    assert_eq!(code, Some(2));
    assert!(
        err.starts_with(
            "xtask transform: unexpected argument `--frob`\nusage: cargo xtask transform"
        ),
        "{err}"
    );
}
