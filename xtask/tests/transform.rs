//! `cargo xtask transform` (spec/harness.md §6.2 to §6.4), against the
//! stand-in `fake-slip` (`xtask/src/bin/fake-slip.rs`), which does what
//! the `fake.txt` of the test's tree says. Every test writes its transform
//! test under a fixture's `tests/transform/rename/<name>/` and works in
//! the fixture's `target/transform/rename/<name>/`. The records a run
//! leaves in its store (edit set, declared changes, overlay, result) are
//! built here with `slip_model`, consistent with each other, and published
//! by the fake; the runner reads them back (`run::read_record`). The parse
//! tests also read the committed fixtures of `xtask/tests/transform/`.

#![cfg(target_os = "linux")]
#![allow(
    clippy::many_single_char_names,
    clippy::too_many_lines,
    clippy::format_collect,
    clippy::format_push_string,
    reason = "a case is a table of short names and scripts built as text"
)]

mod common;

use common::Fixture;
use slip_exec::store::{Access, Object, Store};
use slip_model::declared::Declared;
use slip_model::edits::{MacroSite, Piece, Place};
use slip_model::editset::{Edit, EditSet, SetState};
use slip_model::hash::content_digest;
use slip_model::overlay::{Content, Obligation, Overlay, OverlayFile};
use slip_model::result::{self, Category, Residue, RunVerdict, State, Tally, TransformResult};
use slip_model::{Digest, FileRef, formats};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use xtask::transform::mutate::{self, mutations};
use xtask::transform::oracles::parse_depfile;
use xtask::transform::run::{
    Lines, check_output, cmdline, first_line_difference, normalise, read_commands,
};
use xtask::transform::{
    Allow, Expected, Note, Oracle, Outcome, Report, Test, discover, find_clang, parse_args,
    parse_test, run_one, run_test,
};

const FAKE: &str = env!("CARGO_BIN_EXE_fake-slip");

const PRE: &str = "namespace a { int f(); }\nint x = a::f();\n";
const POST: &str = "namespace a { int g(); }\nint x = a::g();\n";
/// The key of the spelled pieces of [`tree_set`].
const KEY: u8 = 0x10;

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

/// `text` with every maximal run of exactly 64 lower-case hexadecimal
/// digits made `<id>`, as the runner normalises.
fn ids(text: &str) -> String {
    let b = text.as_bytes();
    let hexdigit = |c: u8| c.is_ascii_digit() || (b'a'..=b'f').contains(&c);
    let mut out = String::new();
    let mut i = 0;
    while i < b.len() {
        let mut j = i;
        while j < b.len() && hexdigit(b[j]) {
            j += 1;
        }
        if j > i {
            out.push_str(if j - i == 64 { "<id>" } else { &text[i..j] });
            i = j;
        } else {
            let mut k = i;
            while k < b.len() && !hexdigit(b[k]) {
                k += 1;
            }
            out.push_str(&text[i..k]);
            i = k;
        }
    }
    out
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
    fn report(&self, clang: Option<&Path>) -> Report {
        run_test(&self.test(), Path::new(FAKE), &self.work(), clang, false)
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
    /// The tree's path of `rel`, as the records name it.
    fn tree_path(&self, rel: &str) -> Vec<u8> {
        self.tree().join(rel).to_string_lossy().as_bytes().to_vec()
    }
}

// ---------------------------------------------------------------- the records

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

/// An accepted edit set over the tree's `a.cpp`: edits at the two `f`s,
/// spelled pieces of [`KEY`], and the `a` of `a::f` kept.
fn tree_set(c: &Case) -> EditSet {
    let mut s = empty_set();
    s.files = vec![FileRef {
        path: c.tree_path("a.cpp"),
        size: PRE.len() as u64,
        content: content_digest(PRE.as_bytes()),
    }];
    s.edits =
        vec![edit(0, 18, 19, vec![spelled(KEY, "g")]), edit(0, 36, 37, vec![spelled(KEY, "g")])];
    s.kept = vec![Place { file: 0, lo: 33, hi: 34 }];
    assert_eq!(s.validate(), Ok(()));
    s
}

/// The overlay of `set` (whose edits are of the tree's `a.cpp`, one byte
/// for one byte): `a.cpp` from `PRE` to `POST`, an obligation at each
/// spelled piece's place, of its key.
fn overlay_of(c: &Case, set: &EditSet) -> Overlay {
    let id = formats::content_id(&set.to_bytes()).unwrap();
    let mut spellings: Vec<Obligation> = set
        .edits
        .iter()
        .flat_map(|e| {
            e.pieces.iter().filter_map(move |p| match p {
                Piece::Spelled { key, .. } => {
                    Some(Obligation { file: 0, lo: e.lo, hi: e.lo + 1, key: *key })
                }
                _ => None,
            })
        })
        .collect();
    spellings.sort();
    spellings.dedup();
    let o = Overlay {
        edits: id,
        files: vec![OverlayFile {
            path: c.tree_path("a.cpp"),
            pre: Content { size: PRE.len() as u64, content: content_digest(PRE.as_bytes()) },
            post: Content { size: POST.len() as u64, content: content_digest(POST.as_bytes()) },
        }],
        spellings,
    };
    assert_eq!(o.validate(), Ok(()));
    o
}

/// A result of `state` over the edit set `edits`, consistent as
/// `spec/edits.md` §8.5 and `slip_verify::conclude::permits` read it: a
/// `verified` or `verified_with_residue` one permits writing (the latter
/// with `allow: residue`, once residue is added).
fn result_of(
    state: State,
    edits: Digest,
    declared: Digest,
    overlay: Digest,
    obligations: u64,
) -> TransformResult {
    let refused = state == State::Refused;
    let (verdict, tally) = match state {
        State::Verified | State::VerifiedWithResidue => {
            (Some(RunVerdict::Verified), Tally { selected: 1, verified: 1, ..Tally::default() })
        }
        State::NotVerified => {
            (Some(RunVerdict::NotVerified), Tally { selected: 1, mismatch: 1, ..Tally::default() })
        }
        State::Incomplete => {
            (Some(RunVerdict::Incomplete), Tally { selected: 1, failed: 1, ..Tally::default() })
        }
        State::Refused => (None, Tally { selected: 1, ..Tally::default() }),
    };
    let obligations = if refused { 0 } else { obligations };
    TransformResult {
        scheme: 1,
        plan: d(0x11),
        round: 1,
        transform: d(0x22),
        edits,
        declared: (!refused).then_some(declared),
        overlay: (!refused).then_some(overlay),
        state,
        partial: false,
        allow_unparsed: false,
        verdict,
        tally,
        agreement: Vec::new(),
        unused: Vec::new(),
        unchecked: Vec::new(),
        spellings: result::Spellings { obligations, passed: obligations, failed: Vec::new() },
        residue: Vec::new(),
        exempt: Vec::new(),
    }
}

fn residue_at(path: Vec<u8>, category: Category, lo: u64, hi: u64) -> Residue {
    Residue { category, file: path, lo, hi, line: 1, column: lo + 1 }
}

/// The records of a run, written outside the test, and the fake's actions
/// that publish them into the store it is given.
struct Records {
    set: EditSet,
    state: State,
    edits: String,
    declared: Option<String>,
    overlay: Option<String>,
    result: String,
    obligations: usize,
    publish: String,
}

impl Case {
    /// Publishes `set`, no declared change, its overlay and a result of
    /// `state` (changed by `tweak`), as files named `<tag>.*`.
    fn records(
        &self,
        tag: &str,
        set: &EditSet,
        state: State,
        tweak: impl FnOnce(&mut TransformResult),
    ) -> Records {
        let set_bytes = set.to_bytes();
        let set_id = formats::content_id(&set_bytes).unwrap();
        let decl = Declared::none().to_bytes();
        let decl_id = formats::content_id(&decl).unwrap();
        let ov = overlay_of(self, set);
        let ov_bytes = ov.to_bytes();
        let ov_id = formats::content_id(&ov_bytes).unwrap();
        let mut r = result_of(state, set_id, decl_id, ov_id, ov.spellings.len() as u64);
        tweak(&mut r);
        assert_eq!(r.validate(), Ok(()), "{tag}");
        let r_bytes = r.to_bytes();
        let r_id = formats::content_id(&r_bytes).unwrap();
        let mut publish = format!(
            "publish edits {}\n",
            self.object(&format!("{tag}.edts"), &set_bytes).display()
        );
        if r.declared.is_some() {
            publish.push_str(&format!(
                "publish declared {}\npublish overlay {}\n",
                self.object(&format!("{tag}.decl"), &decl).display(),
                self.object(&format!("{tag}.ovly"), &ov_bytes).display()
            ));
        }
        publish.push_str(&format!(
            "publish result {}\n",
            self.object(&format!("{tag}.rslt"), &r_bytes).display()
        ));
        Records {
            set: set.clone(),
            state: r.state,
            edits: set_id.to_string(),
            declared: r.declared.map(|d| d.to_string()),
            overlay: r.overlay.map(|d| d.to_string()),
            result: r_id.to_string(),
            obligations: ov.spellings.len(),
            publish,
        }
    }

    /// [`tree_set`] with a result of `state`.
    fn tree_records(&self, state: State, tweak: impl FnOnce(&mut TransformResult)) -> Records {
        self.records("run", &tree_set(self), state, tweak)
    }
}

/// The main run's output for `r`: plan, transform, round, edit set,
/// declared, overlay, result, with the result's state.
fn main_out(r: &Records) -> Vec<String> {
    let set_state = if r.set.state == SetState::Accepted { "accepted" } else { "refused" };
    let mut v = vec![
        format!("plan {}", hex(0x11)),
        format!("transform {}", hex(0x22)),
        "round 1".into(),
        "map: selected 1, ok 1, not ok 0, failed runs 0".into(),
        format!("edits {}", r.edits),
        format!("state {set_state}"),
    ];
    if let (Some(dc), Some(ov)) = (&r.declared, &r.overlay) {
        v.push(format!("declared {dc}: rules 0, exempt 0"));
        v.push(format!("overlay {ov}: files 1, spellings {}", r.obligations));
    }
    v.push(format!("result {}", r.result));
    v.push(format!("state {}", r.state.name()));
    v
}

/// The golden of an output: ids made `<id>`.
fn golden(lines: &[String]) -> String {
    lines.iter().map(|l| format!("{}\n", ids(l))).collect()
}

fn outs(lines: &[String]) -> String {
    lines.iter().map(|l| ["out ", l, "\n"].concat()).collect()
}

/// A script for a run that publishes `r`, prints its output and writes
/// `POST`; whose inverse writes `PRE` back; and whose `check` gives the
/// run's result.
fn good_script(r: &Records) -> String {
    let main = outs(&main_out(r));
    format!(
        "== rename * --from a::g\n{main}write a.cpp {pre}\n\
         == rename *\n{publish}{main}write a.cpp {post}\n\
         == check 0\nout result {id}\nout state {state}\n",
        publish = r.publish,
        pre = esc(PRE),
        post = esc(POST),
        id = r.result,
        state = r.state.name()
    )
}

const TEST_TXT: &str = "spec: harness §6.3\nrun: rename --from a::f --to g\nstate: verified\n\
                        oracle: no-mutation the case answers no mutated check\n";

/// The standard passing test: `verified` records, `script` made from
/// [`good_script`] by `edit`.
fn standard(name: &str, test_txt: &str, edit: impl FnOnce(String) -> String) -> (Case, Records) {
    let c = Case::new(name);
    let r = c.tree_records(State::Verified, |_| {});
    let script = edit(good_script(&r));
    c.write("test.txt", test_txt);
    c.write("pre/a.cpp", PRE);
    c.write("pre/fake.txt", &script);
    c.write("post/a.cpp", POST);
    c.write("expect.txt", &golden(&main_out(&r)));
    (c, r)
}

/// A test that writes nothing: `state` records (not permitting a write),
/// no post/.
fn unwritten(name: &str, test_txt: &str, state: State) -> (Case, Records) {
    let c = Case::new(name);
    let r = c.tree_records(state, |_| {});
    let script = good_script(&r).replace(
        &format!("{}write a.cpp {}\n", outs(&main_out(&r)), esc(POST)),
        &outs(&main_out(&r)),
    );
    c.write("test.txt", test_txt);
    c.write("pre/a.cpp", PRE);
    c.write("pre/fake.txt", &script);
    c.write("expect.txt", &golden(&main_out(&r)));
    (c, r)
}

/// A change of a script.
type ScriptEdit = Box<dyn FnOnce(String) -> String>;

fn fail_message(o: &Outcome) -> &str {
    match o {
        Outcome::Fail(m) => m,
        other => panic!("not a FAIL: {other:?}"),
    }
}

/// The skips of a report, `(oracle, reason)`.
fn skips(r: &Report) -> Vec<(Oracle, String)> {
    r.notes.iter().filter(|n| !n.off).map(|n| (n.oracle, n.reason.clone())).collect()
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
    assert_eq!((t.timeout, t.post_none), (30, false));
    assert!(t.may_write(), "verified_with_residue with allow: residue");
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
    assert_eq!((t.timeout, t.post_none), (600, false));
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
            &format!(
                "spec: edits §1\nrun: rename --from f --to g\nstate: {state}\n{}",
                if state == "verified" { "post: none\n" } else { "" }
            ),
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
    let base = "spec: edits §1\nrun: rename --from f --to g\nstate: not_verified\n";
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
        ("post: some\n", "test.txt:4: post: `some` is not `none`"),
        ("post: none\npost: none\n", "test.txt:5: `post` given twice"),
        ("timeout: 0\n", "test.txt:4: timeout: `0` is not a number of seconds 1 to 86400"),
        ("timeout: 86401\n", "test.txt:4: timeout: `86401` is not a number of seconds 1 to"),
        ("timeout: +5\n", "test.txt:4: timeout: `+5` is not a number of seconds"),
        ("timeout: 9999999999999999999999\n", "test.txt:4: timeout: `9999999999999999999999`"),
        ("timeout: 5\ntimeout: 5\n", "test.txt:5: `timeout` given twice"),
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
    std::fs::remove_file(c.dir().join("notes.txt")).unwrap();
    // F11 of the review: no entry of a test is a symbolic link (a post/
    // that is one would lead --bless out of the test), nor of another kind.
    let elsewhere = c.root.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    std::fs::write(elsewhere.join("a.cpp"), "y").unwrap();
    std::os::unix::fs::symlink(&elsewhere, c.dir().join("post")).unwrap();
    assert_eq!(parse_test(&c.dir()).unwrap_err(), "`post` is a symbolic link");
    std::fs::remove_file(c.dir().join("post")).unwrap();
    std::os::unix::fs::symlink(elsewhere.join("a.cpp"), c.dir().join("expect.txt")).unwrap();
    assert_eq!(parse_test(&c.dir()).unwrap_err(), "`expect.txt` is a symbolic link");
    std::fs::remove_file(c.dir().join("expect.txt")).unwrap();
    std::fs::create_dir(c.dir().join("expect.txt")).unwrap();
    assert_eq!(parse_test(&c.dir()).unwrap_err(), "`expect.txt` is not a regular file");
    std::fs::remove_dir(c.dir().join("expect.txt")).unwrap();
    std::fs::write(c.dir().join("post"), "").unwrap();
    assert_eq!(parse_test(&c.dir()).unwrap_err(), "`post` is not a directory");
    std::fs::remove_file(c.dir().join("post")).unwrap();
    // ... and nothing under pre/ or post/ is a link.
    std::os::unix::fs::symlink(&elsewhere, c.dir().join("pre/inc")).unwrap();
    assert_eq!(
        parse_test(&c.dir()).unwrap_err(),
        "pre/: inc: a symbolic link, not a regular file or a directory"
    );
    std::fs::remove_file(c.dir().join("pre/inc")).unwrap();
    assert!(parse_test(&c.dir()).is_ok());
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
    // F3 of the review: a test whose run may write has a post/ or says
    // `post: none`.
    let c = Case::new("post");
    c.write("test.txt", base);
    c.write("pre/a.cpp", "x");
    assert_eq!(
        parse_test(&c.dir()).unwrap_err(),
        "the run may write (state `verified`): the test needs a post/, or `post: none` if the \
         run writes nothing"
    );
    c.write("test.txt", &format!("{base}post: none\n"));
    assert!(c.test().post_none);
    c.write("post/a.cpp", "y");
    assert_eq!(parse_test(&c.dir()).unwrap_err(), "the test has a post/ and says `post: none`");
    c.write("test.txt", &base.replace("verified", "refused\npost: none"));
    std::fs::remove_dir_all(c.dir().join("post")).unwrap();
    assert_eq!(
        parse_test(&c.dir()).unwrap_err(),
        "`post: none` is for a run that may write, and the state `refused` with these `allow` \
         lines cannot"
    );
    // An empty post/ (a directory, no file) says nothing.
    c.write("test.txt", base);
    std::fs::create_dir_all(c.dir().join("post/sub")).unwrap();
    assert_eq!(
        parse_test(&c.dir()).unwrap_err(),
        "post/ holds no file (a run that writes nothing says `post: none`)"
    );
    std::fs::remove_dir_all(c.dir().join("post")).unwrap();
    // F2 (P01): verified_with_residue permits writing only with `allow:
    // residue` or `allow: errors`.
    let vwr = base.replace("verified", "verified_with_residue");
    c.write("test.txt", &vwr);
    c.write("post/a.cpp", "y");
    assert_eq!(
        parse_test(&c.dir()).unwrap_err(),
        "the state `verified_with_residue` permits writing only with `allow: residue` or \
         `allow: errors`, and the test has a post/"
    );
    for allow in ["residue", "errors"] {
        c.write("test.txt", &format!("{vwr}allow: {allow}\n"));
        assert!(c.test().may_write(), "{allow}");
    }
    c.write("test.txt", &format!("{vwr}allow: unparsed\nallow: partial\n"));
    assert!(parse_test(&c.dir()).is_err());
    // A mode is part of a file: post/ differing only in mode is a change.
    c.write("test.txt", base);
    c.write("post/a.cpp", "x");
    std::fs::set_permissions(c.dir().join("post/a.cpp"), std::fs::Permissions::from_mode(0o755))
        .unwrap();
    assert!(c.test().may_write());
    std::fs::remove_dir_all(c.dir().join("post")).unwrap();
    c.write("post/a.cpp", "y");
    std::fs::create_dir_all(c.dir().join("post/a.cpp.d")).unwrap();
    assert_eq!(parse_test(&c.dir()).unwrap_err(), "post/a.cpp.d has no pre/ file");
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
    let (c, r) = standard("pass", TEST_TXT, |s| s);
    let report = c.report(None);
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
                r.edits
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
    assert_eq!(c.read("expect.txt").unwrap(), golden(&main_out(&r)));
    assert!(!c.dir().join("pre/store").exists());
    // F8: the oracle turned off and the one skipped are in the report.
    assert_eq!(
        report.notes,
        [
            Note {
                oracle: Oracle::Mutation,
                off: true,
                reason: "the case answers no mutated check".into()
            },
            Note {
                oracle: Oracle::Clang,
                off: false,
                reason: "no compiler (none found, or --no-clang)".into()
            },
        ]
    );
}

// §6.2, §6.3: std, args, tu, owned, readonly and allow reach the command
// lines; `check` gets `--allow-unparsed`; cdb.json has @ROOT@ replaced.
#[test]
fn s6_3_directives_reach_the_command_lines() {
    let txt = "spec: harness §6.3\nrun: rename --from a::f --to g --kind function\nstate: verified\n\
               std: c++26\nargs: -DX -DY\ntu: b.cpp a.cpp\nowned: a.cpp\nowned: b.cpp\n\
               allow: unparsed\nallow: residue\noracle: no-mutation none\noracle: no-back none\n";
    let c = Case::new("lines");
    let r = c.tree_records(State::Verified, |r| r.allow_unparsed = true);
    c.write("test.txt", txt);
    c.write("pre/a.cpp", PRE);
    c.write("pre/b.cpp", "int b;\n");
    c.write("pre/dep/d.h", "int d;\n");
    c.write("pre/fake.txt", &good_script(&r));
    c.write("post/a.cpp", POST);
    c.write("expect.txt", &golden(&main_out(&r)));
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
    // The result must say what the run was given: allow_unparsed.
    let r2 = c.tree_records(State::Verified, |_| {});
    c.write("pre/fake.txt", &good_script(&r2));
    c.write("expect.txt", &golden(&main_out(&r2)));
    assert!(fail_message(&c.run()).starts_with(
        "store: the result says allow_unparsed false, and the run was given --allow-unparsed"
    ));

    // A test's cdb.json, with @ROOT@ for the tree.
    let (c, _) = standard("cdb", TEST_TXT, |s| s);
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

/// The main section's line that writes `POST`, after its output.
fn post_write() -> String {
    format!("out state verified\nwrite a.cpp {}", esc(POST))
}

// §6.3 step 3: each single difference is a FAIL naming it.
#[test]
fn s6_3_each_difference_fails() {
    let cases: Vec<(&str, ScriptEdit, &str)> = vec![
        (
            "status",
            Box::new(|g: String| {
                g.replacen(&post_write(), &format!("{}\nexit 1", post_write()), 2)
            }),
            "exit status 1, expected 0",
        ),
        (
            "state",
            Box::new(|g: String| {
                g.replace("out state verified\nwrite", "out state not_verified\nwrite")
            }),
            "store: the run printed state not_verified, its result record says verified; \
             state not_verified, expected verified",
        ),
        (
            "line",
            Box::new(|g: String| g.replace("out map: selected 1", "out map: selected 2")),
            "output line 4: expected `map: selected 1, ok 1, not ok 0, failed runs 0`, got \
             `map: selected 2, ok 1, not ok 0, failed runs 0`",
        ),
        (
            "extra",
            Box::new(|g: String| {
                g.replace(&post_write(), &format!("{}\nout written 1 file(s)", post_write()))
            }),
            "output line 11: expected the end, got `written 1 file(s)`",
        ),
        (
            "different",
            Box::new(|g: String| g.replace(&esc(POST), &esc(&POST.replace("g()", "h()")))),
            "tree: file a.cpp differs (first at byte 18)",
        ),
        (
            "added",
            Box::new(|g: String| {
                g.replace(&post_write(), &format!("{}\nwrite b.cpp new", post_write()))
            }),
            "tree: file b.cpp added",
        ),
        (
            "missing",
            Box::new(|g: String| {
                g.replace(&post_write(), &format!("{}\nremove a.cpp", post_write()))
            }),
            "tree: file a.cpp missing",
        ),
        // F5 (P02): the mode is compared.
        (
            "mode",
            Box::new(|g: String| {
                g.replace(&post_write(), &format!("{}\nchmod a.cpp 755", post_write()))
            }),
            "tree: file a.cpp mode 755, expected 644",
        ),
        // F5 (P04a): a new directory, even an empty one.
        (
            "emptydir",
            Box::new(|g: String| {
                g.replace(&post_write(), &format!("{}\nmkdir new/sub", post_write()))
            }),
            "tree: directory new added",
        ),
        // F5 (P04b): a symbolic link in place of a file is a FAIL, not an
        // ERROR.
        (
            "symlink",
            Box::new(|g: String| {
                g.replace(
                    &post_write(),
                    &format!("{}\nremove a.cpp\nsymlink b a.cpp", post_write()),
                )
            }),
            "tree: file a.cpp is now a symbolic link",
        ),
    ];
    for (name, edit, want) in cases {
        let (c, _) = standard(name, TEST_TXT, edit);
        let o = c.run();
        assert!(fail_message(&o).starts_with(want), "{name}: {o:?}");
    }
    // A state needs a result line; its absence shows standard error.
    let (c, r) = standard("noresult", TEST_TXT, |s| s);
    let script = c.read("pre/fake.txt").unwrap();
    c.write(
        "pre/fake.txt",
        &script.replace(&format!("out result {}\n", r.result), "err slip: it broke\n"),
    );
    let o = c.run();
    assert!(
        fail_message(&o)
            .starts_with("the run printed no result line; standard error:\nslip: it broke"),
        "{o:?}"
    );
    // R13: the newline at the end of the output is compared.
    let (c, r) = standard("newline", TEST_TXT, |s| s);
    let g = golden(&main_out(&r));
    c.write("expect.txt", g.trim_end());
    assert_eq!(fail_message(&c.run()), "output line 10: the newline at its end differs");
}

// F5 (P03, P03b): a run that writes in the work directory outside its
// tree, or into the test's directory, fails; so does any later run.
#[test]
fn s6_3_writes_outside_the_tree() {
    let up = "../../../../..";
    for (name, extra, want) in [
        ("stray", "write ../stray x".to_owned(), "wrote outside its tree: @WORK@/stray"),
        ("stray-dir", "mkdir ../d".to_owned(), "wrote outside its tree: @WORK@/d"),
        (
            "stray-store",
            "mkdir ../back\nwrite ../back/x y".to_owned(),
            "wrote outside its tree: @WORK@/back/x",
        ),
        (
            "plant",
            format!("write {up}/tests/transform/rename/plant/pre/x y"),
            "changed the test's directory: file pre/x added",
        ),
        (
            "plant-expect",
            format!("write {up}/tests/transform/rename/plant-expect/expect.txt y"),
            "changed the test's directory: file expect.txt differs (first at byte 0)",
        ),
    ] {
        let (c, _) = standard(name, TEST_TXT, |g| {
            g.replace(&post_write(), &format!("{}\n{extra}", post_write()))
        });
        let o = c.run();
        let want = want.replace("@WORK@", &c.work().display().to_string());
        assert!(
            fail_message(&o).starts_with("`slip rename --from a::f --to g")
                && fail_message(&o).ends_with(&want),
            "{name}: {o:?}"
        );
    }
    // The back oracle's run, too.
    let (c, _) = standard("stray-back", TEST_TXT, |g| {
        g.replacen(
            &format!("write a.cpp {}\n", esc(PRE)),
            &format!("write a.cpp {}\nwrite ../z z\n", esc(PRE)),
            1,
        )
    });
    let o = c.run();
    assert!(
        fail_message(&o).starts_with("`slip rename --from a::g --to f")
            && fail_message(&o)
                .ends_with(&format!("wrote outside its tree: {}/z", c.work().display())),
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
    let (c, r) = standard("normalise", TEST_TXT, |g| {
        g.replace(
            "out state verified\nwrite a.cpp",
            "out state verified\nout residue mention: @CWD@/a.cpp:1:1\nwrite a.cpp",
        )
    });
    let mut lines = main_out(&r);
    lines.push("residue mention: @ROOT@/a.cpp:1:1".into());
    c.write("expect.txt", &golden(&lines));
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

// F10 (P14): the output's structure is checked before it is compared or
// blessed.
#[test]
fn s6_3_output_structure() {
    let (e, r, s) = (hex(1), hex(2), "state verified");
    let ok = format!("edits {e}\nstate accepted\nresult {r}\n{s}\n");
    assert_eq!(check_output(&ok, true), Ok(()));
    assert_eq!(check_output("plan x\n", true), Ok(()), "no result: the state check says so");
    for (out, want) in [
        (
            format!("{ok}{s}\n"),
            "output line 5: a `state` line that is neither the edit set's nor the result's",
        ),
        (format!("{ok}result {r}\n{s}\n"), "the output has 2 `result` lines"),
        (format!("edits {e}\nstate accepted\nedits {e}\n"), "the output has 2 `edits` lines"),
        (
            format!("edits {e}\nstate done\nresult {r}\n{s}\n"),
            "output line 2: the edit set's state `done` is not accepted or refused",
        ),
        (
            format!("result {r}\nstate Verified\n"),
            "output line 2: `Verified` is not a state of spec/edits.md §8.5",
        ),
        (
            format!("result {r}\nstate verified \n"),
            "output line 2: `verified ` is not a state of spec/edits.md §8.5",
        ),
        (
            format!("result {r}\nx\n"),
            "the `result` line is not followed by the result's `state` line",
        ),
        (
            format!("result {r}\n{s}\nedits {e}\nstate accepted\n"),
            "the result's `state` line is not the last `state` line",
        ),
        (format!("result {r}\nstate\n"), "output line 2: `state` without a value"),
    ] {
        assert_eq!(check_output(&out, true), Err(want.to_owned()), "{out}");
    }
    assert_eq!(
        check_output(&format!("result {r}\n"), false),
        Err("the output has a `result` line, and the run ends without a result".into())
    );
    assert!(check_output(&ok, false).is_err(), "no state line but the edit set's");
    assert_eq!(check_output(&format!("edits {e}\nstate refused\n"), false), Ok(()));
    // Through a run, and under --bless: nothing is blessed.
    let (c, _) = standard("structure", TEST_TXT, |g| {
        g.replacen(&post_write(), &format!("{}\nout state verified", post_write()), 1)
    });
    assert_eq!(
        fail_message(&c.run()),
        "output: output line 11: a `state` line that is neither the edit set's nor the result's"
    );
    c.write("expect.txt", "stale\n");
    assert_eq!(
        fail_message(&c.run_with(None, true)),
        "output: output line 11: a `state` line that is neither the edit set's nor the result's"
    );
    assert_eq!(c.read("expect.txt").unwrap(), "stale\n");
}

// F1 of the review (P07, P09): the run's result record, its edit set, its
// declared changes and its overlay are read from the store and must be
// what the run printed.
#[test]
fn s6_3_the_store_is_the_output() {
    type StoreEdit = Box<dyn Fn(&Case, &Records, String) -> String>;
    let cases: Vec<(&str, StoreEdit, String)> = vec![
        (
            // P07: nothing published.
            "empty",
            Box::new(|_, r, g| g.replace(&r.publish, "")),
            "store: the store has no result @R@".into(),
        ),
        (
            // P09: a printed state that is not the record's.
            "state",
            Box::new(|c, r, g| {
                let nv = c.records("nv", &r.set, State::NotVerified, |_| {});
                g.replace(&r.publish, &nv.publish).replace(
                    &format!("out result {}\n", r.result),
                    &format!("out result {}\n", nv.result),
                )
            }),
            "store: the run printed state verified, its result record says not_verified".into(),
        ),
        (
            "edits",
            Box::new(|_, r, g| {
                g.replace(&format!("out edits {}", r.edits), &format!("out edits {}", hex(0x33)))
            }),
            format!("store: the run printed edits {}, its result record says @E@", hex(0x33)),
        ),
        (
            "declared",
            Box::new(|_, r, g| {
                g.replace(
                    &format!("out declared {}: rules 0, exempt 0\n", r.declared.as_ref().unwrap()),
                    "",
                )
            }),
            "store: the run printed declared none, its result record says @D@".into(),
        ),
        (
            "overlay",
            Box::new(|_, r, g| {
                g.replace(
                    &format!("out overlay {}", r.overlay.as_ref().unwrap()),
                    &format!("out overlay {}", hex(0x55)),
                )
            }),
            format!("store: the run printed overlay {}, its result record says @O@", hex(0x55)),
        ),
        (
            "plan",
            Box::new(|_, _, g| {
                g.replace(&format!("out plan {}", hex(0x11)), &format!("out plan {}", hex(0x12)))
            }),
            format!(
                "store: the run printed plan {}, its result record says {}",
                hex(0x12),
                hex(0x11)
            ),
        ),
        (
            "round",
            Box::new(|_, _, g| g.replace("out round 1\n", "out round 2\n")),
            "store: the run printed round 2, its result record says 1".into(),
        ),
        (
            "no-set",
            Box::new(|_, r, g| {
                g.replace(
                    &r.publish,
                    &r.publish.lines().skip(1).map(|l| format!("{l}\n")).collect::<String>(),
                )
            }),
            "store: the store has no edit set @E@".into(),
        ),
        (
            "no-declared",
            Box::new(|_, r, g| {
                g.replace(
                    &r.publish,
                    &r.publish
                        .lines()
                        .filter(|l| !l.starts_with("publish declared"))
                        .map(|l| format!("{l}\n"))
                        .collect::<String>(),
                )
            }),
            "store: the store has no declared changes @D@".into(),
        ),
        (
            "no-overlay",
            Box::new(|_, r, g| {
                g.replace(
                    &r.publish,
                    &r.publish
                        .lines()
                        .filter(|l| !l.starts_with("publish overlay"))
                        .map(|l| format!("{l}\n"))
                        .collect::<String>(),
                )
            }),
            "store: the store has no overlay @O@".into(),
        ),
        (
            "bad-set",
            Box::new(|_, r, g| {
                g.replacen(
                    "out plan",
                    &format!("write ../store/edits/{}.edts garbage\nout plan", r.edits),
                    2,
                )
            }),
            "store: the edit set @E@: ".into(),
        ),
        (
            "journal",
            Box::new(|_, _, g| {
                g.replacen("out plan", "write ../store/write.journal x\nout plan", 2)
            }),
            "store: the store holds write.journal: a write was left unfinished".into(),
        ),
    ];
    for (name, edit, want) in cases {
        let c = Case::new(name);
        let r = c.tree_records(State::Verified, |_| {});
        let script = edit(&c, &r, good_script(&r));
        c.write("test.txt", &format!("{TEST_TXT}oracle: no-determinism the store is changed\n"));
        c.write("pre/a.cpp", PRE);
        c.write("pre/fake.txt", &script);
        c.write("post/a.cpp", POST);
        c.write("expect.txt", &ids(&golden(&main_out(&r))));
        let want = want
            .replace("@R@", &r.result)
            .replace("@E@", &r.edits)
            .replace("@D@", r.declared.as_ref().unwrap())
            .replace("@O@", r.overlay.as_ref().unwrap());
        let o = c.run();
        assert!(fail_message(&o).starts_with(&want), "{name}: {o:?}");
        // The written tree is then unpermitted as well.
        assert!(fail_message(&o).contains("does not permit writing"), "{name}: {o:?}");
        // And --bless refuses.
        c.write("expect.txt", "stale\n");
        let o = c.run_with(None, true);
        assert!(fail_message(&o).starts_with("cannot bless: "), "{name}: {o:?}");
        assert_eq!(c.read("expect.txt").unwrap(), "stale\n", "{name}");
    }
}

// §6.3: --bless rewrites expect.txt and post/ and not test.txt; it refuses
// when the state or the exit status differ.
#[test]
fn s6_3_bless() {
    let (c, r) = standard("bless", TEST_TXT, |g| {
        g.replace(&post_write(), &format!("{}\nout written 1 file(s)", post_write()))
    });
    c.write("expect.txt", "stale\n");
    c.write("post/a.cpp", "old post\n");
    let txt = c.read("test.txt").unwrap();
    assert_eq!(c.run_with(None, true), Outcome::Pass);
    let mut lines = main_out(&r);
    lines.push("written 1 file(s)".into());
    let expect = golden(&lines);
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

    // A post/ file the run no longer changes is removed; a mode is blessed.
    let txt = format!("{TEST_TXT}oracle: no-back the inverse keeps the mode\n");
    let (c, _) = standard("bless-remove", &txt, |g| {
        g.replace(&post_write(), &format!("{}\nchmod a.cpp 755", post_write()))
    });
    c.write("pre/b.cpp", "b\n");
    c.write("post/b.cpp", "b changed\n");
    assert_eq!(c.run_with(None, true), Outcome::Pass);
    assert!(!c.dir().join("post/b.cpp").exists());
    assert_eq!(c.read("post/a.cpp").unwrap(), POST);
    let mode = std::fs::metadata(c.dir().join("post/a.cpp")).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o755);
    assert_eq!(c.run(), Outcome::Pass);

    for (name, edit, want) in [
        (
            "bless-state",
            Box::new(|g: String| {
                g.replace("out state verified\nwrite", "out state not_verified\nwrite")
            }) as ScriptEdit,
            "cannot bless: state not_verified, expected verified; store: the run printed state \
             not_verified, its result record says verified; the result does not permit writing \
             (spec/edits.md §8.5)",
        ),
        (
            "bless-status",
            Box::new(|g: String| {
                g.replacen(&post_write(), &format!("{}\nexit 3", post_write()), 2)
            }),
            "cannot bless: exit status 3, expected 0",
        ),
        (
            "bless-added",
            Box::new(|g: String| {
                g.replace(&post_write(), &format!("{}\nwrite new.cpp x", post_write()))
            }),
            "cannot bless: post/ cannot hold the written tree: file new.cpp added",
        ),
        (
            "bless-dir",
            Box::new(|g: String| g.replace(&post_write(), &format!("{}\nmkdir e", post_write()))),
            "cannot bless: post/ cannot hold the written tree: directory e added",
        ),
        (
            "bless-link",
            Box::new(|g: String| {
                g.replace(
                    &post_write(),
                    &format!("{}\nremove a.cpp\nsymlink x a.cpp", post_write()),
                )
            }),
            "cannot bless: post/ cannot hold the written tree: file a.cpp is now a symbolic link",
        ),
        (
            "bless-nothing",
            Box::new(|g: String| g.replace(&post_write(), "out state verified")),
            "cannot bless: the run wrote nothing: say `post: none` in test.txt",
        ),
    ] {
        let (c, _) = standard(name, TEST_TXT, edit);
        c.write("expect.txt", "stale\n");
        let o = c.run_with(None, true);
        assert_eq!(fail_message(&o), want, "{name}");
        assert_eq!(c.read("expect.txt").unwrap(), "stale\n", "{name}: nothing rewritten");
        assert_eq!(c.read("post/a.cpp").unwrap(), POST);
    }
    // `post: none` and a run that writes.
    let (c, _) = standard("bless-none", &format!("{TEST_TXT}post: none\n"), |g| g);
    std::fs::remove_dir_all(c.dir().join("post")).unwrap();
    c.write("expect.txt", "stale\n");
    assert_eq!(
        fail_message(&c.run_with(None, true)),
        "cannot bless: the test says `post: none`, and the run wrote"
    );
    assert!(!c.dir().join("post").exists());
}

// F2 of the review (P01, R22): whether the run may write is
// `slip_verify::conclude::permits` over its records and the test's
// `allow` lines, never the expected state's name; the check and --bless
// refuse an unpermitted write.
#[test]
fn s6_3_write_without_permission() {
    // A verified result whose tally has a mismatch: the state's name says
    // verified, the record does not permit writing.
    let c = Case::new("unpermitted");
    let r = c.tree_records(State::Verified, |r| {
        r.tally.verified = 0;
        r.tally.mismatch = 1;
    });
    c.write("test.txt", TEST_TXT);
    c.write("pre/a.cpp", PRE);
    c.write("pre/fake.txt", &good_script(&r));
    c.write("post/a.cpp", POST);
    c.write("expect.txt", &golden(&main_out(&r)));
    assert_eq!(
        fail_message(&c.run()),
        "the tree was written, and the result (state verified) does not permit writing \
         (spec/edits.md §8.5): file a.cpp differs (first at byte 18)"
    );
    c.write("expect.txt", "stale\n");
    assert_eq!(
        fail_message(&c.run_with(None, true)),
        "cannot bless: the result does not permit writing (spec/edits.md §8.5)"
    );
    assert_eq!(c.read("expect.txt").unwrap(), "stale\n");

    // verified_with_residue: residue that is not a mention needs `allow:
    // residue`; with it, the record permits; a test with `allow: errors`
    // only may write, the record (residue, no error) does not permit.
    let vwr = "spec: harness §6.3\nrun: rename --from a::f --to g\nstate: verified_with_residue\n\
               oracle: no-mutation none\n";
    for (name, allow, pass) in
        [("vwr-residue", "allow: residue\n", true), ("vwr-errors", "allow: errors\n", false)]
    {
        let c = Case::new(name);
        let path = c.tree_path("a.cpp");
        let r = c.tree_records(State::VerifiedWithResidue, |r| {
            r.residue = vec![residue_at(path, Category::NotCompiled, 0, 9)];
        });
        c.write("test.txt", &format!("{vwr}{allow}"));
        c.write("pre/a.cpp", PRE);
        let post = format!("write a.cpp {}", esc(POST));
        c.write("pre/fake.txt", &good_script(&r).replace(&post, &format!("{post}\nexit 1")));
        c.write("post/a.cpp", POST);
        c.write("expect.txt", &golden(&main_out(&r)));
        let o = c.run();
        if pass {
            assert_eq!(o, Outcome::Pass, "{name}");
        } else {
            assert!(
                fail_message(&o).starts_with(
                    "the tree was written, and the result (state verified_with_residue) does not \
                     permit writing"
                ),
                "{name}: {o:?}"
            );
        }
    }

    // A not_verified test whose run writes; the same without the write
    // passes.
    let txt = "spec: harness §6.3\nrun: rename --from a::f --to g\nstate: not_verified\nexit: 0\n";
    let (c, r) = unwritten("nv-writes", txt, State::NotVerified);
    let script = c.read("pre/fake.txt").unwrap();
    let main = outs(&main_out(&r));
    c.write(
        "pre/fake.txt",
        &script.replacen(
            &format!("{}{main}", r.publish),
            &format!("{}{main}write a.cpp {}\n", r.publish, esc(POST)),
            1,
        ),
    );
    let o = c.run();
    assert_eq!(
        fail_message(&o),
        "tree: file a.cpp differs (first at byte 18); the tree was written, and the result \
         (state not_verified) does not permit writing (spec/edits.md §8.5): file a.cpp differs \
         (first at byte 18)"
    );
    let (c, _) = unwritten("nv-ok", txt, State::NotVerified);
    assert_eq!(c.run(), Outcome::Pass);
}

// F3 of the review (P05): a run that may write and writes nothing fails
// against its post/; with `post: none` it passes, and the oracles that
// need a written tree are skipped and counted.
#[test]
fn s6_3_a_run_that_writes_nothing() {
    let (c, _) = standard("nothing", TEST_TXT, |g| g.replace(&post_write(), "out state verified"));
    assert_eq!(fail_message(&c.run()), "tree: file a.cpp differs (first at byte 18)");
    let (c, _) = standard("nothing-none", &format!("{TEST_TXT}post: none\n"), |g| {
        g.replace(&post_write(), "out state verified")
    });
    std::fs::remove_dir_all(c.dir().join("post")).unwrap();
    let r = c.report(None);
    assert_eq!(r.outcome, Outcome::Pass);
    assert!(!r.clang_skipped, "nothing written: not a skip for want of a compiler");
    assert_eq!(
        skips(&r),
        [
            (Oracle::Clang, "the tree was not written".to_owned()),
            (Oracle::Back, "the tree was not written".to_owned())
        ]
    );
    assert!(c.log("back/store").is_empty());
    // `post: none` and a run that writes: the tree differs from pre/.
    let (c, _) = standard("nothing-wrote", &format!("{TEST_TXT}post: none\n"), |g| g);
    std::fs::remove_dir_all(c.dir().join("post")).unwrap();
    assert_eq!(fail_message(&c.run()), "tree: file a.cpp differs (first at byte 18)");
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
    let r = c.report(None);
    assert_eq!(r.outcome, Outcome::Pass);
    // The determinism oracle ran; the others are skipped, with reasons.
    assert_eq!(c.log("determinism/store").len(), 1);
    assert!(!c.work().join("back").exists());
    assert_eq!(
        skips(&r),
        [
            (Oracle::Clang, "the tree was not written".to_owned()),
            (Oracle::Back, "the tree was not written".to_owned()),
            (Oracle::Verify, "the run has no result".to_owned()),
            (Oracle::Mutation, "the run has no result".to_owned()),
        ]
    );
    let c = Case::new("input");
    c.write("test.txt", &txt.replace("usage", "input"));
    c.write("pre/a.cpp", PRE);
    c.write("pre/fake.txt", "== rename *\nexit 2\n");
    c.write("expect.txt", "");
    assert_eq!(fail_message(&c.run()), "exit status 2, expected 3");
    // A usage run that prints a result fails on the structure.
    c.write(
        "pre/fake.txt",
        &format!("== rename *\nout result {}\nout state refused\nexit 3\n", hex(1)),
    );
    assert_eq!(
        fail_message(&c.run()),
        "output: output line 2: a `state` line that is neither the edit set's nor the result's"
    );
}

// §6: KILLED, never masked by xfail.
#[test]
fn s6_killed() {
    let txt = format!("{TEST_TXT}xfail: known\n");
    let (c, _) = standard("killed", &txt, |_| "== rename *\nout plan x\nkill\n".into());
    match c.run() {
        Outcome::Killed(m) => assert!(m.contains("killed by signal 9"), "{m}"),
        o => panic!("{o:?}"),
    }
}

// F6 of the review: each invocation has a timeout (600 s, or `timeout:`);
// at its end the process group is killed and the test is KILLED, also
// when a grandchild holds the output open.
#[test]
fn s6_timeout() {
    let txt = format!("{TEST_TXT}timeout: 1\nxfail: known\n");
    let (c, _) = standard("sleeps", &txt, |_| "== rename *\nout plan x\nsleep 30\n".into());
    let start = std::time::Instant::now();
    match c.run() {
        Outcome::Killed(m) => assert!(
            m.starts_with("`slip rename --from a::f --to g")
                && m.ends_with("did not end within 1 s; its process group was killed"),
            "{m}"
        ),
        o => panic!("{o:?}"),
    }
    assert!(start.elapsed() < std::time::Duration::from_secs(20));
    // The fake ends at once; its grandchild keeps standard output open.
    let (c, _) = standard("grandchild", &txt, |_| {
        "== rename *\nout plan x\nspawn-sleep 30 pid.txt\nexit 0\n".into()
    });
    let start = std::time::Instant::now();
    assert!(matches!(c.run(), Outcome::Killed(m) if m.ends_with("its process group was killed")));
    assert!(start.elapsed() < std::time::Duration::from_secs(20));
    let pid = std::fs::read_to_string(c.tree().join("pid.txt")).unwrap();
    // The grandchild was in the group: it is gone (or a zombie no one has
    // reaped yet).
    let gone = || {
        std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .map_or(true, |s| s.rsplit_once(") ").is_some_and(|(_, r)| r.starts_with('Z')))
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !gone() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(gone(), "the grandchild {pid} still runs");
    // The default.
    assert_eq!(standard("default-timeout", TEST_TXT, |g| g).0.test().timeout, 600);
}

// §6: xfail turns FAIL into XFAIL and PASS into XPASS; an ERROR stays.
#[test]
fn s6_xfail() {
    let txt = format!("{TEST_TXT}xfail: the rename is not written yet\n");
    let (c, _) =
        standard("xfail", &txt, |g| g.replace("out map: selected 1", "out map: selected 2"));
    assert_eq!(c.run(), Outcome::XFail);
    let (c, _) = standard("xpass", &txt, |g| g);
    assert_eq!(c.run(), Outcome::XPass);
    let (c, _) = standard("xerror", &txt, |g| g);
    std::fs::remove_file(c.dir().join("expect.txt")).unwrap();
    assert!(matches!(c.run(), Outcome::Error(_)));
}

// §6.3: the work directory must be <target>/transform/<area>/<name>; it is
// removed and made again for each run.
#[test]
fn s6_3_work_directory() {
    let (c, _) = standard("workdir", TEST_TXT, |g| g);
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

// §6.4 determinism: a different id, output or written tree with -j 1
// fails; no-determinism.
#[test]
fn s6_4_determinism() {
    // Another result under -j 1.
    let c = Case::new("det");
    let r = c.tree_records(State::Verified, |_| {});
    let other = c.records("other2", &r.set, State::Verified, |t| {
        t.tally = Tally { selected: 2, verified: 2, ..Tally::default() };
    });
    let j1 = format!(
        "== rename * -j 1\n{}{}write a.cpp {}\n",
        other.publish,
        outs(&main_out(&other)),
        esc(POST)
    );
    c.write("test.txt", TEST_TXT);
    c.write("pre/a.cpp", PRE);
    c.write("pre/fake.txt", &format!("{j1}{}", good_script(&r)));
    c.write("post/a.cpp", POST);
    c.write("expect.txt", &golden(&main_out(&r)));
    let o = c.run();
    let m = fail_message(&o);
    assert!(
        m.starts_with(&format!(
            "determinism: the result id is {} with -j 1, {} with -j 2",
            other.result, r.result
        )),
        "{m}"
    );
    assert!(m.contains("determinism: with -j 1, output line 9"), "{m}");
    let (c, r) = standard("det-out", TEST_TXT, |g| g);
    let j1 = format!(
        "== rename * -j 1\n{}{}out extra\nwrite a.cpp {}\n",
        r.publish,
        outs(&main_out(&r)),
        esc(POST)
    );
    c.write("pre/fake.txt", &format!("{j1}{}", good_script(&r)));
    assert_eq!(
        fail_message(&c.run()),
        "determinism: with -j 1, output line 11: expected the end, got `extra`"
    );
    let (c, r) = standard("det-exit", TEST_TXT, |g| g);
    let j1 = format!("== rename * -j 1\n{}{}exit 1\n", r.publish, outs(&main_out(&r)));
    c.write("pre/fake.txt", &format!("{j1}{}", c.read("pre/fake.txt").unwrap()));
    let m = c.run();
    assert!(
        fail_message(&m).starts_with("determinism: exit status 1 with -j 1, 0 with -j 2"),
        "{m:?}"
    );
    // F7 (P06): the same output and a different tree.
    let (c, r) = standard("det-tree", TEST_TXT, |g| g);
    let j1 = format!(
        "== rename * -j 1\n{}{}write a.cpp {}\n",
        r.publish,
        outs(&main_out(&r)),
        esc(&format!("{POST}// -j 1\n"))
    );
    c.write("pre/fake.txt", &format!("{j1}{}", good_script(&r)));
    assert_eq!(
        fail_message(&c.run()),
        format!(
            "determinism: the tree written with -j 1 is not the first run's: file a.cpp differs \
             (first at byte {})",
            POST.len()
        )
    );
    // Each store's path is the same `@STORE@`.
    let line = "out more (slip dump @STORE@/edits/x.edts)\n";
    let (c, r) = standard("det-store", TEST_TXT, |g| {
        g.replace("out round 1\n", &format!("out round 1\n{line}"))
    });
    let mut lines = main_out(&r);
    lines.insert(3, "more (slip dump @STORE@/edits/x.edts)".into());
    c.write("expect.txt", &golden(&lines));
    assert_eq!(c.run(), Outcome::Pass);
    // Turned off.
    let (c, r) = standard(
        "det-off",
        &format!("{TEST_TXT}oracle: no-determinism j1 is known to differ\n"),
        |g| g,
    );
    let j1 = format!("== rename * -j 1\n{}exit 1\n", outs(&main_out(&r)));
    c.write("pre/fake.txt", &format!("{j1}{}", good_script(&r)));
    let rep = c.report(None);
    assert_eq!(rep.outcome, Outcome::Pass);
    assert!(c.log("determinism/store").is_empty());
    assert!(rep.notes.contains(&Note {
        oracle: Oracle::Determinism,
        off: true,
        reason: "j1 is known to differ".into()
    }));
}

// §6.4 verify: slip check over the run's own edit set gives the run's
// result; no-verify; F4: it runs for verified and verified_with_residue
// results only, and is a counted skip otherwise.
#[test]
fn s6_4_verify() {
    let (c, r) = standard("verify", TEST_TXT, |g| g);
    let script = c.read("pre/fake.txt").unwrap();
    c.write(
        "pre/fake.txt",
        &script.replace(
            &format!("== check 0\nout result {}", r.result),
            &format!("== check 0\nout result {}", hex(0x67)),
        ),
    );
    assert_eq!(
        fail_message(&c.run()),
        format!("verify: slip check gives result {}, the run {}", hex(0x67), r.result)
    );
    c.write(
        "pre/fake.txt",
        &script.replace(
            &format!("== check 0\nout result {}\n", r.result),
            "== check 0\nerr no\nexit 3\n",
        ),
    );
    assert_eq!(fail_message(&c.run()), "verify: slip check printed no result line (exit 3): no");
    // The state check printed is the record's.
    c.write(
        "pre/fake.txt",
        &script.replace(
            &format!("== check 0\nout result {}\nout state verified", r.result),
            &format!("== check 0\nout result {}\nout state not_verified", r.result),
        ),
    );
    assert_eq!(
        fail_message(&c.run()),
        "verify: slip check printed state not_verified, its result record says verified"
    );
    // Its output's structure.
    c.write(
        "pre/fake.txt",
        &script.replace(
            &format!("== check 0\nout result {}\n", r.result),
            &format!("== check 0\nout result {0}\nout result {0}\n", r.result),
        ),
    );
    assert_eq!(
        fail_message(&c.run()),
        "verify: slip check's output: the output has 2 `result` lines"
    );
    let (c, _) = standard("verify-off", &format!("{TEST_TXT}oracle: no-verify none\n"), |g| {
        g.replace("== check 0", "== check 9")
    });
    assert_eq!(c.run(), Outcome::Pass);
    assert_eq!(c.log("store").len(), 1, "no check ran");
    // A not_verified, an incomplete and a refused result run no check
    // (P10, P11), and say so.
    for (name, state) in [
        ("verify-nv", State::NotVerified),
        ("verify-inc", State::Incomplete),
        ("verify-ref", State::Refused),
    ] {
        let txt = format!(
            "spec: harness §6.4\nrun: rename --from a::f --to g\nstate: {}\nexit: 0\n",
            state.name()
        );
        let (c, _) = unwritten(name, &txt, state);
        let r = c.report(None);
        assert_eq!(r.outcome, Outcome::Pass, "{name}");
        assert_eq!(c.log("store").len(), 1, "{name}");
        let why = format!("the result is {}", state.name());
        assert_eq!(
            skips(&r)[2..],
            [(Oracle::Verify, why.clone()), (Oracle::Mutation, why)],
            "{name}"
        );
    }
}

// §6.4 back: the inverse ends in the same state and leaves pre/; no-back.
#[test]
fn s6_4_back() {
    let first = format!("write a.cpp {}\n== rename *\n", esc(PRE));
    let (c, _) = standard("back", TEST_TXT, |g| {
        g.replacen(&first, &format!("write a.cpp {}\n== rename *\n", esc("int y;\n")), 1)
    });
    assert_eq!(
        fail_message(&c.run()),
        "back: the tree is not pre/ again: file a.cpp differs (first at byte 0)"
    );
    let (c, _) = standard("back-state", TEST_TXT, |g| {
        g.replacen("out state verified\n", "out state not_verified\n", 1)
    });
    assert_eq!(
        fail_message(&c.run()),
        "back: `rename --from a::g --to f` ends in state not_verified, the run in verified"
    );
    let (c, _) = standard(
        "back-off",
        &format!("{TEST_TXT}oracle: no-back the inverse is ambiguous\n"),
        |g| g.replacen(&first, "exit 1\n== rename *\n", 1),
    );
    assert_eq!(c.run(), Outcome::Pass);
    assert!(c.log("back/store").is_empty());
    // A run that has no inverse cannot be judged.
    let (c, _) = standard(
        "back-none",
        &TEST_TXT.replace("--from a::f --to g", "--from a::f --to b::g"),
        |g| g,
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

/// A stand-in clang that logs `<dir> <args>`, rejects a main file that
/// holds `CLANG_REJECTS`, and with `-MF <file>` writes there that it read
/// its main file (`dep: <file>`).
fn fake_clang(c: &Case) -> PathBuf {
    let p = c.root.join("bin/clang");
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    let log = c.root.join("clang.log");
    std::fs::write(
        &p,
        format!(
            "#!/bin/sh\n[ \"$1\" = --probe ] && exit 0\necho \"$(pwd -P) $*\" >> '{}'\n\
             prev=; mf=; for a in \"$@\"; do [ \"$prev\" = -c ] && f=$a; [ \"$prev\" = -MF ] && mf=$a; prev=$a; done\n\
             if grep -q CLANG_REJECTS \"$f\"; then echo \"$f: rejected\" >&2; exit 1; fi\n\
             [ -n \"$mf\" ] && printf 'dep: %s \\\\\\n  %s\\n' \"$f\" \"$CLANG_ALSO\" > \"$mf\"\nexit 0\n",
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
// without one; no-clang. The item-13 rule: every file the run changed is
// read by some command (from its -MD dependency file), else FAIL.
#[test]
fn s6_4_clang() {
    let (c, _) = standard("clang", TEST_TXT, |g| g);
    let clang = fake_clang(&c);
    let r = c.report(Some(&clang));
    assert_eq!(r.outcome, Outcome::Pass);
    assert!(!r.clang_skipped && skips(&r).is_empty(), "{r:?}");
    let (t, w) = (c.tree(), c.work());
    let (t, w) = (t.display(), w.display());
    let line = format!("{t} -std=c++20 -I{t} -I{t}/dep -c a.cpp -fsyntax-only");
    assert_eq!(
        std::fs::read_to_string(c.root.join("clang.log")).unwrap(),
        format!("{line}\n{line} -MD -MF {w}/clang/0.d -MT dep\n")
    );

    let (c, _) = standard("clang-pre", TEST_TXT, |g| g);
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
    let (c, _) = standard("clang-post", TEST_TXT, |g| g.replace(&esc(POST), &esc(&post)));
    c.write("post/a.cpp", &post);
    let clang = fake_clang(&c);
    let o = c.run_with(Some(&clang), false);
    assert!(fail_message(&o).starts_with("clang: the written tree is not accepted: `"), "{o:?}");
    assert!(fail_message(&o).contains("a.cpp: rejected"), "{o:?}");

    // P08: a changed file that no command read.
    let txt = format!("{TEST_TXT}oracle: no-back the inverse keeps b.h\n");
    let (c, _) = standard("clang-unread", &txt, |g| {
        g.replace(&post_write(), &format!("{}\nwrite b.h changed", post_write()))
    });
    c.write("pre/b.h", "b\n");
    c.write("post/b.h", "changed");
    let clang = fake_clang(&c);
    assert_eq!(
        fail_message(&c.run_with(Some(&clang), false)),
        "clang: post-image not compiled: b.h"
    );
    // A file the dependency file names (with a space, as `\ `) counts.
    assert_eq!(
        parse_depfile("dep: a.cpp \\\n  my\\ inc/b.h x$$y.h\n"),
        ["a.cpp", "my inc/b.h", "x$y.h"]
    );

    // Turned off: no clang runs, nothing is counted as skipped.
    let (c, _) =
        standard("clang-off", &format!("{TEST_TXT}oracle: no-clang kiln extension\n"), |g| g);
    let clang = fake_clang(&c);
    let r = c.report(Some(&clang));
    assert_eq!((r.outcome, r.clang_skipped), (Outcome::Pass, false));
    let r = c.report(None);
    assert!(!r.clang_skipped);
    assert!(!c.root.join("clang.log").exists());
    assert!(r.notes.iter().any(|n| n.oracle == Oracle::Clang && n.off));
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

/// A check's answer to a mutated set: the state, the residue `(category,
/// file of the tree, lo, hi)`, and the obligations (indices into the
/// overlay of the mutated set) that failed.
type Answer = (State, Vec<(Category, &'static str, u64, u64)>, Vec<u64>);

/// A check section that publishes, for the mutated set `m`, its overlay
/// and a result of `state`, and prints it.
fn check_section(c: &Case, n: usize, m: &EditSet, declared: &str, a: &Answer) -> String {
    let (state, res, failed) = a;
    let ov = overlay_of(c, m);
    let ov_bytes = ov.to_bytes();
    let ov_id = formats::content_id(&ov_bytes).unwrap();
    let set_id = formats::content_id(&m.to_bytes()).unwrap();
    let dc = Digest::from_hex(declared).unwrap();
    let mut r = result_of(*state, set_id, dc, ov_id, ov.spellings.len() as u64);
    r.residue =
        res.iter().map(|(cat, f, lo, hi)| residue_at(c.tree_path(f), *cat, *lo, *hi)).collect();
    if !failed.is_empty() {
        r.spellings.passed -= failed.len() as u64;
        r.spellings.failed = failed
            .iter()
            .map(|&i| result::FailedSpelling {
                obligation: i,
                why: result::Why::DenotesOther,
                observers: 1,
                first: Some(d(0xa0)),
                found: Some(d(0x12)),
                name: "g".into(),
            })
            .collect();
    }
    assert_eq!(r.validate(), Ok(()), "check {n}");
    let bytes = r.to_bytes();
    let id = formats::content_id(&bytes).unwrap();
    let p = c.object(&format!("r{n}.rslt"), &bytes);
    let po = c.object(&format!("o{n}.ovly"), &ov_bytes);
    format!(
        "== check {n}\npublish overlay {}\npublish result {}\nout result {id}\nout state {}\n",
        po.display(),
        p.display(),
        state.name()
    )
}

/// The mutation case: the run publishes `set` (default [`tree_set`]) with
/// a verified result and answers checks 1 to n (for [`tree_set`]: drop 0,
/// drop 1, add at kept 0, key 0, key 1) with `answers`.
fn mutation_case_of(
    name: &str,
    set: impl FnOnce(&Case) -> EditSet,
    answers: &[Answer],
) -> (Case, Records) {
    let c = Case::new(name);
    let set = set(&c);
    let r = c.records("run", &set, State::Verified, |_| {});
    let mut script = good_script(&r);
    let declared = r.declared.clone().unwrap();
    for (i, ((_, m), a)) in mutations(&set).iter().zip(answers).enumerate() {
        script.push_str(&check_section(&c, i + 1, m, &declared, a));
    }
    c.write(
        "test.txt",
        &TEST_TXT.replace("oracle: no-mutation the case answers no mutated check\n", ""),
    );
    c.write("pre/a.cpp", PRE);
    c.write("pre/fake.txt", &script);
    c.write("post/a.cpp", POST);
    c.write("expect.txt", &golden(&main_out(&r)));
    (c, r)
}

fn mutation_case(name: &str, answers: &[Answer]) -> (Case, Records) {
    mutation_case_of(name, tree_set, answers)
}

fn good_answers() -> Vec<Answer> {
    vec![
        (State::VerifiedWithResidue, vec![(Category::NotCompiled, "a.cpp", 18, 19)], vec![]),
        (State::NotVerified, vec![], vec![]),
        (State::NotVerified, vec![], vec![]),
        // The key of edit 0 is obligation 0 of its overlay, of edit 1
        // obligation 1.
        (State::NotVerified, vec![], vec![0]),
        (State::NotVerified, vec![], vec![1]),
    ]
}

// §6.4 mutation: every mutation rejected passes; the checks were given
// the mutated sets, published in the store, with the original's declared.
#[test]
fn s6_4_mutation_passes() {
    let (c, r) = mutation_case("mut-pass", &good_answers());
    assert_eq!(c.run(), Outcome::Pass);
    let log = c.log("store");
    assert_eq!(log.len(), 7, "{log:#?}");
    let store = Store::open(&c.work().join("store"), Access::Shared).unwrap();
    for (i, (label, m)) in mutations(&r.set).into_iter().enumerate() {
        let id = formats::content_id(&m.to_bytes()).unwrap();
        let path = store.object_path(Object::EditSet, &id);
        let w = c.work();
        assert_eq!(
            log[i + 2],
            format!(
                "check --plan {} --round 1 --edits {} --declared {} --store {}/store -j 2",
                hex(0x11),
                path.display(),
                r.declared.as_ref().unwrap(),
                w.display()
            ),
            "{label}"
        );
        assert!(path.is_file());
    }
}

// §6.4 mutation: a mutation reported `verified` fails with its label;
// `verified_with_residue` passes for a dropped edit only with the dropped
// range in the residue (not a mention, of the dropped edit's file); it
// fails for an added edit and a changed key; a changed key needs a failed
// spelling obligation of the changed key itself (R29 of the review); a
// refused check is right only for a refused mutated set (R6).
#[test]
fn s6_4_mutation_failures() {
    let with = |i: usize, a: Answer| -> Vec<Answer> {
        let mut v = good_answers();
        v[i] = a;
        v
    };
    let cases: Vec<(&str, Vec<Answer>, &str)> = vec![
        (
            "mut-verified",
            with(1, (State::Verified, vec![], vec![])),
            "mutation `drop edit 1`: verified: an edit was lost or invented and nothing noticed",
        ),
        (
            "mut-elsewhere",
            with(
                0,
                (
                    State::VerifiedWithResidue,
                    vec![(Category::NotCompiled, "a.cpp", 36, 37)],
                    vec![],
                ),
            ),
            "mutation `drop edit 0`: verified_with_residue without the dropped range",
        ),
        (
            // R3: the right range of another file.
            "mut-other-file",
            with(
                0,
                (
                    State::VerifiedWithResidue,
                    vec![(Category::NotCompiled, "b.cpp", 18, 19)],
                    vec![],
                ),
            ),
            "mutation `drop edit 0`: verified_with_residue without the dropped range",
        ),
        (
            "mut-mention",
            with(
                0,
                (State::VerifiedWithResidue, vec![(Category::Mention, "a.cpp", 18, 19)], vec![]),
            ),
            "mutation `drop edit 0`: verified_with_residue without the dropped range",
        ),
        (
            "mut-wider",
            with(
                0,
                (State::VerifiedWithResidue, vec![(Category::Dependent, "a.cpp", 17, 19)], vec![]),
            ),
            "mutation `drop edit 0`: verified_with_residue without the dropped range",
        ),
        (
            "mut-add",
            with(
                2,
                (
                    State::VerifiedWithResidue,
                    vec![(Category::NotCompiled, "a.cpp", 33, 34)],
                    vec![],
                ),
            ),
            "mutation `add edit at kept 0`: verified_with_residue, expected not_verified",
        ),
        (
            "mut-key",
            with(
                3,
                (
                    State::VerifiedWithResidue,
                    vec![(Category::NotCompiled, "a.cpp", 18, 19)],
                    vec![],
                ),
            ),
            "mutation `change key of edit 0 piece 0`: verified_with_residue, expected not_verified",
        ),
        (
            "mut-key-spelling",
            with(4, (State::NotVerified, vec![], vec![])),
            "mutation `change key of edit 1 piece 0`: not_verified, but by no failed spelling \
             obligation of the changed key",
        ),
        (
            // R29: a failed obligation, of the key that was not changed.
            "mut-key-other",
            with(4, (State::NotVerified, vec![], vec![0])),
            "mutation `change key of edit 1 piece 0`: not_verified, but by no failed spelling \
             obligation of the changed key",
        ),
        (
            "mut-incomplete",
            with(1, (State::Incomplete, vec![], vec![])),
            "mutation `drop edit 1`: incomplete, expected not_verified or verified_with_residue",
        ),
        (
            // R6: refused for a set that is not refused.
            "mut-refused",
            with(1, (State::Refused, vec![], vec![])),
            "mutation `drop edit 1`: refused, expected not_verified or verified_with_residue",
        ),
        (
            "mut-add-refused",
            with(2, (State::Refused, vec![], vec![])),
            "mutation `add edit at kept 0`: refused, expected not_verified",
        ),
        (
            "mut-key-verified",
            with(4, (State::Verified, vec![], vec![])),
            "mutation `change key of edit 1 piece 0`: verified: an edit was lost",
        ),
    ];
    for (name, answers, want) in cases {
        let (c, _) = mutation_case(name, &answers);
        let o = c.run();
        assert!(fail_message(&o).starts_with(want), "{name}: {o:?}");
    }
    // A printed state other than the record's fails.
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
    // A result the store does not have.
    let (c, _) = mutation_case("mut-unpublished", &good_answers());
    let script = c.read("pre/fake.txt").unwrap();
    let cut = script.find("== check 2").unwrap();
    c.write(
        "pre/fake.txt",
        &format!(
            "{}== check 2\nout result {}\nout state not_verified\n{}",
            &script[..cut],
            hex(0x68),
            &script[cut..]
        ),
    );
    assert_eq!(
        fail_message(&c.run()),
        format!("mutation `drop edit 1`: the store has no result {}", hex(0x68))
    );
    // Turned off.
    let (c, _) = mutation_case("mut-off", &with(1, (State::Verified, vec![], vec![])));
    c.write(
        "test.txt",
        &TEST_TXT.replace("the case answers no mutated check", "the dropped edit changes nothing"),
    );
    assert_eq!(c.run(), Outcome::Pass);
    assert_eq!(c.log("store").len(), 2);
}

// §6.4 mutation: the drop of the only edit makes a refused set, which must
// end `refused`.
#[test]
fn s6_4_mutation_single_edit() {
    let one = |c: &Case| {
        let mut set = tree_set(c);
        set.edits.truncate(1);
        set.kept.clear();
        set
    };
    let (c, _) = mutation_case_of(
        "mut-one",
        one,
        &[(State::Refused, vec![], vec![]), (State::NotVerified, vec![], vec![0])],
    );
    assert_eq!(c.run(), Outcome::Pass);
    let (c, _) = mutation_case_of(
        "mut-one-nv",
        one,
        &[(State::NotVerified, vec![], vec![]), (State::NotVerified, vec![], vec![0])],
    );
    assert!(
        fail_message(&c.run())
            .starts_with("mutation `drop edit 0`: not_verified, expected refused")
    );
}

// F4 of the review (P10, P11, R29): the mutation oracle runs only for a
// verified or verified_with_residue result; otherwise it is a counted
// skip and no check runs. R23: a run whose output names no declared
// changes, while its result does, fails on the store before any mutation.
#[test]
fn s6_4_mutation_runs_for_verified_results_only() {
    for (name, state) in [
        ("mut-nv", State::NotVerified),
        ("mut-inc", State::Incomplete),
        ("mut-ref", State::Refused),
    ] {
        let txt = format!(
            "spec: harness §6.4\nrun: rename --from a::f --to g\nstate: {}\nexit: 0\n",
            state.name()
        );
        let (c, _) = unwritten(name, &txt, state);
        let r = c.report(None);
        assert_eq!(r.outcome, Outcome::Pass, "{name}");
        assert_eq!(c.log("store").len(), 1, "{name}: no check");
        assert!(
            skips(&r).contains(&(Oracle::Mutation, format!("the result is {}", state.name()))),
            "{name}"
        );
    }
    // A verified_with_residue result does run it.
    let c = Case::new("mut-vwr");
    let path = c.tree_path("a.cpp");
    let r = c.tree_records(State::VerifiedWithResidue, |r| {
        r.residue = vec![residue_at(path, Category::NotCompiled, 0, 9)];
    });
    let mut script = good_script(&r);
    for (i, ((_, m), a)) in mutations(&r.set).iter().zip(&good_answers()).enumerate() {
        script.push_str(&check_section(&c, i + 1, m, r.declared.as_ref().unwrap(), a));
    }
    c.write(
        "test.txt",
        "spec: harness §6.4\nrun: rename --from a::f --to g\nstate: verified_with_residue\nexit: 0\n\
         allow: residue\n",
    );
    c.write("pre/a.cpp", PRE);
    c.write("pre/fake.txt", &script);
    c.write("post/a.cpp", POST);
    c.write("expect.txt", &golden(&main_out(&r)));
    assert_eq!(c.run(), Outcome::Pass);
    assert_eq!(c.log("store").len(), 7);
    // R23.
    let (c, r) = mutation_case("mut-nodecl", &good_answers());
    let script = c.read("pre/fake.txt").unwrap();
    c.write(
        "pre/fake.txt",
        &script.replace(
            &format!("out declared {}: rules 0, exempt 0\n", r.declared.as_ref().unwrap()),
            "",
        ),
    );
    assert!(fail_message(&c.run()).starts_with("store: the run printed declared none"));
    assert_eq!(c.log("store").len(), 1, "no check ran");
}

// ---------------------------------------------------------------- the command (§6.3)

/// Runs `xtask transform` over the fixture of `c` (its root through
/// `CARGO_MANIFEST_DIR`), with no clang on `PATH` and the environment
/// `env`.
fn xtask_env(root: &Path, args: &[&str], env: &[(&str, &str)]) -> (Option<i32>, String, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_xtask"));
    cmd.arg("transform")
        .args(args)
        .env("CARGO_MANIFEST_DIR", root.join("xtask"))
        .env("CARGO_TARGET_DIR", root.join("target"))
        .env("PATH", root.join("no-such-bin"))
        .env_remove("SLIP_CLANG")
        .env_remove("SLIP_REQUIRE_ALL");
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().unwrap();
    (
        out.status.code(),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

fn xtask(root: &Path, args: &[&str]) -> (Option<i32>, String, String) {
    xtask_env(root, args, &[])
}

// §6.3: the outcome lines, the summary, the oracles off and skipped, the
// clang skips and the exit status; a failing test's directory is kept and
// printed, a passing one's removed; --only, --verbose, SLIP_REQUIRE_ALL.
#[test]
fn s6_3_the_command() {
    let (a, _) = standard("a-pass", TEST_TXT, |g| g);
    let root = a.root.clone();
    let dir = root.join("tests/transform/rename");
    // More tests in the same fixture: the same tree path is not theirs,
    // so each has records of its own.
    let add = |name: &str, txt: &str, edit: &dyn Fn(String) -> String| {
        let c =
            Case { _fixture: Fixture::empty("unused"), name: name.to_owned(), root: root.clone() };
        let r = c.records(name, &tree_set(&c), State::Verified, |_| {});
        c.write("test.txt", txt);
        c.write("pre/a.cpp", PRE);
        c.write("pre/fake.txt", &edit(good_script(&r)));
        c.write("post/a.cpp", POST);
        c.write("expect.txt", &golden(&main_out(&r)));
    };
    let off = format!("{TEST_TXT}oracle: no-clang none\n");
    let round2 = |g: String| g.replace("out map: selected 1", "out map: selected 2");
    add("b-fail", &off, &round2);
    add("c-xfail", &format!("{off}xfail: known\n"), &round2);
    add("d-xpass", &format!("{off}xfail: fixed\n"), &|g| g);
    std::fs::create_dir_all(dir.join("e-error")).unwrap();
    add("f-killed", &off, &|_| "== rename *\nkill\n".into());

    let (code, out, _) = xtask(&root, &["--slip", FAKE]);
    let work = root.join("target/transform/rename");
    let w = work.display();
    assert_eq!(code, Some(1));
    let got = |out: &str| {
        // The killed run's command line names its paths: shown up to them.
        out.lines()
            .map(|l| match l.split_once(" --db ") {
                Some((head, _)) if l.starts_with("KILLED") => format!("{head} …"),
                _ => l.to_owned(),
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert_eq!(
        got(&out),
        format!(
            "FAIL rename/b-fail: output line 4: expected `map: selected 1, ok 1, not ok 0, failed \
             runs 0`, got `map: selected 2, ok 1, not ok 0, failed runs 0`\n  kept {w}/b-fail\n\
             XFAIL rename/c-xfail: xfail: known\n\
             XPASS rename/d-xpass: xfail: fixed\n  kept {w}/d-xpass\n\
             ERROR rename/e-error: no test.txt\n\
             KILLED rename/f-killed: `slip rename --from a::f --to g …\n  kept {w}/f-killed\n\
             transform: 1 passed, 1 failed, 1 xfail, 1 xpass, 1 error, 1 killed\n\
             oracles: 9 off, 1 skipped (--verbose names them)\n\
             clang oracle skipped: 1 (no compiler: $SLIP_CLANG, clang++-19 and clang++ not found)"
        )
    );
    assert!(!work.join("a-pass").exists() && !work.join("c-xfail").exists());
    assert!(work.join("b-fail/tree/a.cpp").is_file());
    // R19 of the review: a KILLED test alone fails the gate.
    let (code, out, _) = xtask(&root, &["--slip", FAKE, "--only", "f-killed"]);
    assert_eq!(code, Some(1), "{out}");
    assert!(out.contains("transform: 0 passed, 0 failed, 1 killed\n"), "{out}");

    // --only, and the clang skip alone failing the gate unless --no-clang;
    // --verbose names each oracle that did not run.
    let (code, out, _) =
        xtask(&root, &["--slip", FAKE, "--only", "a-pass", "-j", "1", "--verbose"]);
    assert_eq!(
        (code, out.as_str()),
        (
            Some(1),
            "  rename/a-pass: oracle mutation off: the case answers no mutated check\n  \
             rename/a-pass: oracle clang skipped: no compiler (none found, or --no-clang)\n\
             transform: 1 passed, 0 failed\noracles: 1 off, 1 skipped (--verbose names them)\n\
             clang oracle skipped: 1 (no compiler: $SLIP_CLANG, clang++-19 and clang++ not found)\n"
        )
    );
    let (code, out, _) = xtask(&root, &["--slip", FAKE, "--only", "a-pass", "--no-clang"]);
    assert_eq!(
        (code, out.as_str()),
        (
            Some(0),
            "transform: 1 passed, 0 failed\noracles: 1 off, 1 skipped (--verbose names them)\n\
             clang oracle skipped: 1 (--no-clang)\n"
        )
    );
    // SLIP_REQUIRE_ALL=1: a clang oracle skipped for want of a compiler is
    // an ERROR; with --no-clang it is not.
    let (code, out, _) =
        xtask_env(&root, &["--slip", FAKE, "--only", "a-pass"], &[("SLIP_REQUIRE_ALL", "1")]);
    assert_eq!(code, Some(1));
    assert!(
        out.starts_with(
            "ERROR rename/a-pass: the clang oracle was skipped: no compiler (SLIP_REQUIRE_ALL=1)\n"
        ),
        "{out}"
    );
    let (code, _, _) = xtask_env(
        &root,
        &["--slip", FAKE, "--only", "a-pass", "--no-clang"],
        &[("SLIP_REQUIRE_ALL", "1")],
    );
    assert_eq!(code, Some(0));
    let (code, out, _) = xtask(&root, &["--slip", FAKE, "--only", "c-x", "--only", "d-x"]);
    assert_eq!(code, Some(1));
    assert!(out.contains("transform: 0 passed, 0 failed, 1 xfail, 1 xpass\n"), "{out}");
    let (code, out, _) = xtask(&root, &["--slip", FAKE, "--only", "c-x"]);
    assert_eq!(
        (code, out.as_str()),
        (
            Some(0),
            "XFAIL rename/c-xfail: xfail: known\ntransform: 0 passed, 0 failed, 1 xfail\n\
             oracles: 2 off, 0 skipped (--verbose names them)\n"
        )
    );
    // F9 (P13a): --only that selects nothing is an error; nothing is built
    // or run.
    let (code, out, _) = xtask(&root, &["--only", "nothing", "--only", "else"]);
    assert_eq!(
        (code, out.as_str()),
        (Some(1), "transform: --only nothing --only else matches no test\n")
    );
}

// F9 of the review (P13b, P13c): a symbolic link, or a file, among the
// areas or the tests is an error of discovery, not passed over.
#[test]
fn s6_3_discovery() {
    let f = Fixture::empty("transform-discover");
    let root = f.root().canonicalize().unwrap();
    let base = root.join("tests/transform");
    assert_eq!(discover(&root), Ok(vec![]));
    std::fs::create_dir_all(base.join("rename/t")).unwrap();
    std::fs::create_dir_all(base.join("empty")).unwrap();
    assert_eq!(discover(&root), Ok(vec![base.join("rename/t")]), "an empty area is allowed");
    std::fs::write(base.join("rename/test.txt"), "").unwrap();
    assert_eq!(
        discover(&root).unwrap_err(),
        "tests/transform/rename/test.txt: not a directory (tests are tests/transform/<area>/<name>/)"
    );
    std::fs::remove_file(base.join("rename/test.txt")).unwrap();
    std::os::unix::fs::symlink(base.join("rename/t"), base.join("rename/u")).unwrap();
    assert_eq!(
        discover(&root).unwrap_err(),
        "tests/transform/rename/u: a symbolic link, not a test or an area directory"
    );
    std::fs::remove_file(base.join("rename/u")).unwrap();
    std::os::unix::fs::symlink(base.join("rename"), base.join("again")).unwrap();
    assert_eq!(
        discover(&root).unwrap_err(),
        "tests/transform/again: a symbolic link, not a test or an area directory"
    );
    std::fs::remove_file(base.join("again")).unwrap();
    std::fs::write(base.join("README"), "").unwrap();
    assert!(discover(&root).unwrap_err().starts_with("tests/transform/README: not a directory"));
    // Through the command.
    let (code, out, _) = xtask(&root, &[]);
    assert_eq!(
        (code, out.as_str()),
        (
            Some(1),
            "transform: tests/transform/README: not a directory (tests are \
             tests/transform/<area>/<name>/)\n"
        )
    );
}

// §6.3: the command line.
#[test]
fn s6_3_usage() {
    let o = parse_args(
        &[
            "--only",
            "a",
            "--only",
            "b",
            "--bless",
            "-j",
            "3",
            "--no-clang",
            "--slip",
            "/s",
            "--verbose",
        ]
        .map(String::from),
    )
    .unwrap();
    assert_eq!(o.only, ["a", "b"]);
    assert!(o.bless && o.no_clang && o.verbose);
    assert!(!parse_args(&[]).unwrap().verbose);
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
