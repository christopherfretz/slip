//! `cargo xtask demo 1` (spec/harness.md §10) without a corpus.
//!
//! The pure parts (`read_config`, `read_edit`, `read_expect`, `check`,
//! `indeterminate_units`) are given texts and the report goldens of
//! `xtask/tests/demo/reports/`, one per row of the result table of §10.3.
//! `plan_edit` and `build_tree` work on trees the test makes. The command
//! runs against a fake `slip`: a shell script that copies a store the test
//! made with `slip_exec::store` into `--store`, writes the golden report
//! named for each round to `--report`, and prints fixed lines; the corpus
//! is a fixture with one row and stamps that make it configured.

#![cfg(target_os = "linux")]

mod common;

use common::Fixture;
use slip_exec::store::{Access, Store};
use slip_model::json::{self, Value};
use slip_model::{
    Command as Cmd, Configuration, Counter, Digest, End, FileRef, Job, Language, Observer, Origin,
    Outcome, Phase, Plan, Run, RunRecord, Shard, Status,
};
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use xtask::corpus::{self, Layout, rows};
use xtask::demo::{
    self, CaseResult, Config, Entry, Expect, Line, Outcome as DemoOutcome, Wanted, check,
    indeterminate_units, read_config, read_edit, read_expect,
};
use xtask::invariants::{Env, Failure};

// ---------------------------------------------------------------- helpers

const SRC: &str = "/corpus/src";
const GOLDEN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/demo");

/// The golden report `name` with its placeholders filled.
fn golden_text(name: &str, src: &str, plan: &str, round: u64) -> String {
    std::fs::read_to_string(Path::new(GOLDEN).join("reports").join(name))
        .unwrap()
        .replace("@SRC@", src)
        .replace("@PLAN@", plan)
        .replace("@ROUND@", &round.to_string())
        .replace("@TREE@", "/corpus/demo1/trees/x")
        .replace("@RUN@", "/corpus/demo1/store/runs/x/verify.0.1.run")
}

fn golden(name: &str) -> Value {
    json::parse(golden_text(name, SRC, &"0".repeat(64), 0).as_bytes()).unwrap()
}

fn wanted(kind: &str, name: &str, at: Option<(&str, u64)>) -> Wanted {
    Wanted { kind: kind.into(), name: name.into(), at: at.map(|(f, l)| (f.into(), l)) }
}

fn set(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|s| (*s).to_owned()).collect()
}

fn none() -> BTreeSet<String> {
    BTreeSet::new()
}

fn run_check(expect: &Expect, report: &str) -> (CaseResult, Vec<Line>) {
    check(expect, &golden(report), Path::new(SRC), &none(), &none())
}

fn mismatch(lines: Vec<Wanted>) -> Expect {
    Expect::Mismatch(lines)
}

// ---------------------------------------------------------------- §10.1 demo.toml

// §10.1: `demo.toml` is `corpus`, `roots` and `only`.
#[test]
fn s10_1_demo_toml() {
    let text = "# Demo 1\ncorpus = \"llvm\"\nroots = [\"llvm\", 'clang/lib']\nonly = [\n  \"/llvm/lib/Support/\",\n]\n";
    assert_eq!(
        read_config(text),
        Ok(Config {
            corpus: "llvm".into(),
            roots: vec!["llvm".into(), "clang/lib".into()],
            only: vec!["/llvm/lib/Support/".into()],
        })
    );
    // The committed file reads.
    let committed =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../bench/demo1/demo.toml"))
            .unwrap();
    let c = read_config(&committed).unwrap();
    assert_eq!((c.corpus.as_str(), &c.roots[..]), ("llvm", &["llvm".to_owned()][..]));
    for (bad, msg) in [
        ("roots = [\"a\"]\nonly = [\"b\"]\n", "demo.toml has no `corpus`"),
        ("corpus = \"x\"\nonly = [\"b\"]\n", "demo.toml has no `roots`"),
        ("corpus = \"x\"\nroots = [\"a\"]\n", "demo.toml has no `only`"),
        ("corpus = x\n", "line 1: `corpus` must be one string"),
        ("corpus = \"x\"\nroots = \"a\"\n", "line 2: `roots` must be an array of strings"),
        ("corpus = \"x\"\nroots = [\"a\", 3]\n", "line 2: `roots` must be an array of strings"),
        ("corpus = \"x\"\nroots = []\n", "line 2: `roots` is empty"),
        ("corpus = \"x\"\nroots = [\"../a\"]\n", "line 2: root `../a` is not a relative path"),
        ("corpus = \"x\"\nroots = [\"/a\"]\n", "line 2: root `/a` is not a relative path"),
        ("corpus = \"x\"\ncorpus = \"y\"\n", "line 2: `corpus` is given twice (first at line 1)"),
        ("corpus = \"x\"\nmore = 1\n", "line 2: unknown key `more`"),
        ("[t]\ncorpus = \"x\"\n", "line 1: demo.toml has no tables"),
    ] {
        let e = read_config(bad).unwrap_err();
        assert!(e.starts_with(msg), "{bad:?}: {e}");
    }
}

// ---------------------------------------------------------------- §10.1 edit.txt

// §10.1: every form of `edit.txt`: replace, replace all, create, several
// files, an empty new text, texts as bytes with their line feeds.
#[test]
fn s10_1_edit_forms() {
    let text = b"file lib/a.h\nreplace\nint x;\nwith\nlong x;\nint y;\nend\n\nreplace all\n  f();\nwith\nend\nfile lib/b.cpp\nreplace\n\xffbytes\nwith\nend\ncreate lib/new/c.h\n#pragma once\n\nend\ncreate lib/d.h\nend\n";
    let e = read_edit(text).unwrap();
    assert_eq!(
        e,
        vec![
            Entry::Replace {
                file: "lib/a.h".into(),
                all: false,
                old: b"int x;\n".to_vec(),
                new: b"long x;\nint y;\n".to_vec(),
                line: 2
            },
            Entry::Replace {
                file: "lib/a.h".into(),
                all: true,
                old: b"  f();\n".to_vec(),
                new: vec![],
                line: 9
            },
            Entry::Replace {
                file: "lib/b.cpp".into(),
                all: false,
                old: b"\xffbytes\n".to_vec(),
                new: vec![],
                line: 14
            },
            Entry::Create {
                file: "lib/new/c.h".into(),
                content: b"#pragma once\n\n".to_vec(),
                line: 18
            },
            Entry::Create { file: "lib/d.h".into(), content: vec![], line: 22 },
        ]
    );
    // Every committed edit.txt reads, and stays under the roots.
    let cases = concat!(env!("CARGO_MANIFEST_DIR"), "/../bench/demo1/cases");
    let read = demo::read_cases(Path::new(cases), &["llvm".to_owned()]).unwrap();
    assert_eq!(read.len(), 8);
    assert_eq!(read[0].name, "00-control");
    assert!(read[0].edit.is_empty() && read[7].edit.is_empty());
    assert!(read[1].edit.len() > 10);
    assert_eq!(read[6].base.as_deref(), Some("01-relocate"));
    assert!(read[1].declared.is_some() && read[2].declared.is_none());
    assert_eq!(read[2].observers, ["/llvm/lib/Support/CommandLine.cpp"]);
    assert_eq!(read.iter().map(|c| c.round).collect::<Vec<_>>(), (0..8).collect::<Vec<_>>());
}

// §10.1: each error of `edit.txt` names its line.
#[test]
fn s10_1_edit_errors() {
    for (bad, msg) in [
        (&b""[..], "the edit has no entry"),
        (b"\n\n", "the edit has no entry"),
        (b"file lib/a.h\n", "the edit has no entry"),
        (b"replace\nx\nwith\ny\nend\n", "line 1: `replace` with no `file` line above it"),
        (b"file lib/a.h\nreplace\nx\n", "line 2: no `with` line after it"),
        (b"file lib/a.h\nreplace\nx\nwith\ny\n", "line 2: no `end` line after it"),
        (b"file lib/a.h\nreplace\nwith\ny\nend\n", "line 2: the old text of `replace` is empty"),
        (b"file lib/a.h\nreplace some\nx\nwith\nend\n", "line 2: `replace some` is not"),
        (b"file lib/a.h\nedit\n", "line 2: `edit` is not `file`, `replace [all]` or `create`"),
        (b"file /abs/a.h\n", "line 1: `/abs/a.h` is not a relative path"),
        (b"file lib/../a.h\n", "line 1: `lib/../a.h` is not a relative path"),
        (b"file lib/./a.h\n", "line 1: `lib/./a.h` is not a relative path"),
        (b"file lib//a.h\n", "line 1: `lib//a.h` is not a relative path"),
        (b"file \n", "line 1: `` is not a relative path"),
        (b"create ../x\nend\n", "line 1: `../x` is not a relative path"),
        (b"create lib/x\nabc\n", "line 1: no `end` line after it"),
        (b"\xff\n", "line 1: a keyword line is not UTF-8"),
    ] {
        let e = read_edit(bad).unwrap_err();
        assert!(e.starts_with(msg), "{:?}: {e}", String::from_utf8_lossy(bad));
    }
    // A path that leaves the roots.
    let e = read_edit(b"file other/a.h\nreplace\nx\nwith\nend\n").unwrap();
    assert_eq!(
        demo::check_roots(&e, &["lib".into()]),
        Err("line 2: `other/a.h` is not under a root (lib)".into())
    );
    let e = read_edit(b"create lib\nend\n").unwrap();
    assert!(demo::check_roots(&e, &["lib".into()]).is_err(), "the root itself is not under it");
    let e = read_edit(b"file lib/x/a.h\nreplace\nx\nwith\nend\n").unwrap();
    assert_eq!(demo::check_roots(&e, &["lib".into()]), Ok(()));
}

// ---------------------------------------------------------------- §10.3 trees

/// A pristine tree: `src/lib/{a.h,b.cpp,sub/c.h}` and a symbolic link.
fn pristine(f: &Fixture) -> PathBuf {
    f.write("src/lib/a.h", "int x;\n  f();\nint z;\n  f();\n");
    f.write("src/lib/b.cpp", "#include \"a.h\"\n");
    f.write("src/lib/sub/c.h", "// c\n");
    std::fs::set_permissions(f.path("src/lib/b.cpp"), std::fs::Permissions::from_mode(0o640))
        .unwrap();
    std::os::unix::fs::symlink("sub", f.path("src/lib/link")).unwrap();
    // G6 (spec/verify.md §5.3): a link keeps its target bytes, a route
    // out of the root and bytes that are not UTF-8 included.
    std::os::unix::fs::symlink(OsStr::from_bytes(b"../out/\xff/./x//"), f.path("src/lib/away"))
        .unwrap();
    f.path("src")
}

/// The edit of the tree tests: two replaced files and a created one.
const EDIT: &[u8] = b"file lib/a.h\nreplace\nint x;\nwith\nlong x;\nend\nreplace all\n  f();\nwith\n  g();\nend\nfile lib/b.cpp\nreplace\n#include \"a.h\"\nwith\n#import \"a.h\"\nend\ncreate lib/new/d.h\nnew\nend\n";

fn roots() -> Vec<String> {
    vec!["lib".to_owned()]
}

// §10.3, §11.3: `plan_edit` computes the edit without writing; `build_tree`
// links what the edit leaves (and links as links) and writes what it
// writes as new files; the pristine files are unchanged.
#[test]
fn s11_3_build_tree() {
    let f = Fixture::empty("demo-tree");
    let src = pristine(&f);
    let tree = f.path("trees/01");
    let written = demo::plan_edit(&src, &read_edit(EDIT).unwrap()).unwrap();
    let paths: Vec<&Path> = written.iter().map(|w| w.path.as_path()).collect();
    assert_eq!(paths, [Path::new("lib/a.h"), Path::new("lib/b.cpp"), Path::new("lib/new/d.h")]);
    assert_eq!(written[1].mode.map(|m| m & 0o777), Some(0o640));
    assert_eq!(written[2].mode, None);
    assert!(!tree.exists(), "planning writes nothing");
    let n = demo::build_tree(&mut demo::Disk, &src, &tree, &roots(), &written, demo::Make::Link);
    // c.h, the two symbolic links: what the edit leaves.
    assert_eq!(n, Ok(3));
    let ino = |p: &Path| std::fs::metadata(p).unwrap().ino();
    assert_eq!(ino(&tree.join("lib/sub/c.h")), ino(&src.join("lib/sub/c.h")));
    assert_eq!(std::fs::read_link(tree.join("lib/link")).unwrap(), Path::new("sub"));
    assert_eq!(
        std::fs::read_link(tree.join("lib/away")).unwrap().as_os_str().as_bytes(),
        b"../out/\xff/./x//"
    );
    let read = |p: PathBuf| std::fs::read_to_string(p).unwrap();
    assert_eq!(read(tree.join("lib/a.h")), "long x;\n  g();\nint z;\n  g();\n");
    assert_eq!(read(tree.join("lib/b.cpp")), "#import \"a.h\"\n");
    assert_eq!(read(tree.join("lib/new/d.h")), "new\n");
    // The pristine files are as they were and were never linked; the
    // written ones are files of their own, with the old permission bits.
    assert_eq!(read(src.join("lib/a.h")), "int x;\n  f();\nint z;\n  f();\n");
    assert_eq!(read(src.join("lib/b.cpp")), "#include \"a.h\"\n");
    assert!(!src.join("lib/new").exists());
    let nlink = |p: PathBuf| std::fs::metadata(p).unwrap().nlink();
    assert_eq!(nlink(src.join("lib/a.h")), 1);
    assert_eq!(nlink(tree.join("lib/a.h")), 1);
    assert_eq!(nlink(tree.join("lib/new/d.h")), 1);
    let mode = |p: PathBuf| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(tree.join("lib/b.cpp")), 0o640);
    let again = demo::build_tree(&mut demo::Disk, &src, &tree, &roots(), &[], demo::Make::Link);
    assert!(again.unwrap_err().ends_with("exists"));

    // A tree from another tree (a `base`): the base's edit is kept, and
    // the new tree's edit leaves the base's file alone.
    let next = f.path("trees/02");
    let second = read_edit(b"file lib/a.h\nreplace\nint z;\nwith\nint w;\nend\n").unwrap();
    let w2 = demo::plan_edit(&tree, &second).unwrap();
    demo::build_tree(&mut demo::Disk, &tree, &next, &roots(), &w2, demo::Make::Link).unwrap();
    assert_eq!(read(next.join("lib/a.h")), "long x;\n  g();\nint w;\n  g();\n");
    assert_eq!(read(tree.join("lib/a.h")), "long x;\n  g();\nint z;\n  g();\n");
    assert_eq!(ino(&next.join("lib/b.cpp")), ino(&tree.join("lib/b.cpp")));

    // `Copy`: no file of the tree is a link to the pristine tree.
    let copied = f.path("trees/03");
    demo::build_tree(&mut demo::Disk, &src, &copied, &roots(), &written, demo::Make::Copy).unwrap();
    assert_eq!(nlink(copied.join("lib/sub/c.h")), 1);
    assert_eq!(nlink(src.join("lib/sub/c.h")), 3, "linked by trees/01 and trees/02 only");
    assert_eq!(read(copied.join("lib/a.h")), "long x;\n  g();\nint z;\n  g();\n");
}

/// A [`demo::TreeOps`] that records every call on the file system and
/// makes it, optionally planting other bytes in one written file.
#[derive(Default)]
struct Recorder {
    calls: Vec<(String, PathBuf)>,
    plant: Option<PathBuf>,
}

impl demo::TreeOps for Recorder {
    fn create_dir(&mut self, path: &Path) -> std::io::Result<()> {
        self.calls.push(("dir".into(), path.to_owned()));
        demo::Disk.create_dir(path)
    }
    fn hard_link(&mut self, from: &Path, to: &Path) -> std::io::Result<()> {
        self.calls.push(("link".into(), to.to_owned()));
        demo::Disk.hard_link(from, to)
    }
    fn copy(&mut self, from: &Path, to: &Path) -> std::io::Result<()> {
        self.calls.push(("copy".into(), to.to_owned()));
        demo::Disk.copy(from, to)
    }
    fn symlink(&mut self, target: &Path, at: &Path) -> std::io::Result<()> {
        self.calls.push(("symlink".into(), at.to_owned()));
        demo::Disk.symlink(target, at)
    }
    fn write_new(&mut self, path: &Path, bytes: &[u8], mode: Option<u32>) -> std::io::Result<()> {
        // Before the write, the name does not exist: it cannot be a link.
        assert!(std::fs::symlink_metadata(path).is_err(), "{} exists", path.display());
        self.calls.push(("write".into(), path.to_owned()));
        if self.plant.as_deref() == Some(path) {
            return demo::Disk.write_new(path, b"planted\n", mode);
        }
        demo::Disk.write_new(path, bytes, mode)
    }
    fn read(&mut self, path: &Path) -> std::io::Result<Vec<u8>> {
        self.calls.push(("read".into(), path.to_owned()));
        demo::Disk.read(path)
    }
}

// §11.3: an edited and a created file are never a hard link at any
// moment: every name the edit writes is made by one `write_new` (of a
// name that does not exist) and never by a link, a copy or a symbolic
// link; every written file is read back, after every write.
#[test]
fn s11_3_written_names_never_linked() {
    let f = Fixture::empty("demo-record");
    let src = pristine(&f);
    let tree = f.path("t");
    let written = demo::plan_edit(&src, &read_edit(EDIT).unwrap()).unwrap();
    let mut rec = Recorder::default();
    demo::build_tree(&mut rec, &src, &tree, &roots(), &written, demo::Make::Link).unwrap();
    for w in &written {
        let path = tree.join(&w.path);
        let made: Vec<&str> =
            rec.calls.iter().filter(|(_, p)| *p == path).map(|(c, _)| c.as_str()).collect();
        assert_eq!(made, ["write", "read"], "{}", path.display());
    }
    let last_write = rec.calls.iter().rposition(|(c, _)| c == "write").unwrap();
    let first_read = rec.calls.iter().position(|(c, _)| c == "read").unwrap();
    assert!(first_read > last_write, "read back after every write");
    let links = rec.calls.iter().filter(|(c, _)| c == "link").count();
    assert_eq!(links, 1, "only lib/sub/c.h is linked");
}

// §11.3: the read-back check fails on a planted difference, naming the
// file.
#[test]
fn s11_3_read_back_finds_a_difference() {
    let f = Fixture::empty("demo-plant");
    let src = pristine(&f);
    let tree = f.path("t");
    let written = demo::plan_edit(&src, &read_edit(EDIT).unwrap()).unwrap();
    let mut rec = Recorder { plant: Some(tree.join("lib/b.cpp")), ..Recorder::default() };
    let e =
        demo::build_tree(&mut rec, &src, &tree, &roots(), &written, demo::Make::Link).unwrap_err();
    assert!(e.contains("lib/b.cpp: read back 8 bytes that are not the 14 written"), "{e}");
    // `read_back` alone, on a tree changed after it was made.
    std::fs::write(tree.join("lib/b.cpp"), "#import \"a.h\"\n").unwrap();
    assert_eq!(demo::read_back(&mut demo::Disk, &tree, &written), Ok(()));
    std::fs::write(tree.join("lib/new/d.h"), "old\n").unwrap();
    let e = demo::read_back(&mut demo::Disk, &tree, &written).unwrap_err();
    assert!(e.ends_with("lib/new/d.h: read back 4 bytes that are not the 4 written"), "{e}");
}

// §10.1: the errors of an edit name the line, and nothing is written.
#[test]
fn s10_1_plan_edit_errors() {
    let f = Fixture::empty("demo-apply");
    let src = pristine(&f);
    for (bad, msg) in [
        (&b"file lib/none.h\nreplace\nx\nwith\nend\n"[..], "line 2: `lib/none.h` does not exist"),
        (b"file lib/sub\nreplace\nx\nwith\nend\n", "line 2: `lib/sub` is not a regular file"),
        (b"create lib/b.cpp\nend\n", "line 1: `lib/b.cpp` exists"),
        (b"file lib/a.h\nreplace\nnope\nwith\nend\n", "line 2: the old text is not found"),
        (b"file lib/a.h\nreplace\n  f();\nwith\nend\n", "line 2: the old text is found 2 times"),
        (
            b"file lib/a.h\nreplace\nint x;\nwith\nint y;\nend\nreplace\nint x;\nwith\nend\n",
            "line 7: the old text is not found",
        ),
        (
            b"file lib/link/c.h\nreplace\n// c\nwith\nend\n",
            "line 2: `lib/link/c.h` goes through the symbolic link `link`",
        ),
        (
            b"create lib/a.h/x\nend\n",
            "line 1: `lib/a.h/x` goes through `a.h`, which is not a directory",
        ),
    ] {
        let e = demo::plan_edit(&src, &read_edit(bad).unwrap()).unwrap_err();
        assert!(e.starts_with(msg), "{:?}: {e}", String::from_utf8_lossy(bad));
    }
    assert_eq!(
        std::fs::read_to_string(src.join("lib/a.h")).unwrap(),
        "int x;\n  f();\nint z;\n  f();\n"
    );
    // A written path outside the roots.
    let w = demo::Written { path: "other/x.h".into(), content: vec![], mode: None };
    let e = demo::build_tree(&mut demo::Disk, &src, &f.path("t"), &roots(), &[w], demo::Make::Link);
    assert_eq!(e, Err("`other/x.h` is not under a root".into()));
}

// ---------------------------------------------------------------- §10.1 expect.txt

// §10.1: `expect.txt` forms and errors.
#[test]
fn s10_1_expect() {
    assert_eq!(read_expect("verified\n"), Ok(Expect::Verified));
    assert_eq!(
        read_expect(
            "mismatch\n  changed llvm::StringRef::find(char, unsigned long) const @ llvm/include/llvm/ADT/StringRef.h:282\n\n  removed a::b\n  added x @ y @ lib/a.h:3\n"
        ),
        Ok(Expect::Mismatch(vec![
            wanted(
                "changed",
                "llvm::StringRef::find(char, unsigned long) const",
                Some(("llvm/include/llvm/ADT/StringRef.h", 282))
            ),
            wanted("removed", "a::b", None),
            wanted("added", "x @ y", Some(("lib/a.h", 3))),
        ]))
    );
    for (bad, msg) in [
        ("", "expect.txt is empty"),
        ("ok\n", "the first line is `ok`"),
        ("verified\n  changed x\n", "line 2: `verified` takes no finding line"),
        ("mismatch\n", "`mismatch` needs at least one finding line"),
        ("mismatch\n  moved x\n", "line 2: `moved` is not a kind"),
        ("mismatch\n  changed\n", "line 2: `changed` has no name"),
        ("mismatch\n  changed x @ a.h\n", "line 2: the place `a.h` is not `<file>:<line>`"),
        ("mismatch\n  changed x @ a.h:0\n", "line 2: the line `0` of `a.h:0` is not a positive"),
        ("mismatch\n  changed x @ a.h:1x\n", "line 2: the line `1x`"),
        ("mismatch\n  changed x @ /a.h:1\n", "line 2: `/a.h` is not a relative path"),
        ("mismatch\n  changed  @ a.h:1\n", "line 2: `changed  @ a.h:1` has no name"),
    ] {
        let e = read_expect(bad).unwrap_err();
        assert!(e.starts_with(msg), "{bad:?}: {e}");
    }
    // Every committed expect.txt reads.
    let cases = concat!(env!("CARGO_MANIFEST_DIR"), "/../bench/demo1/cases");
    for c in demo::read_cases(Path::new(cases), &["llvm".to_owned()]).unwrap() {
        let n = &c.name;
        match (&c.expect, n.as_str()) {
            (Expect::Verified, "00-control" | "01-relocate") => {}
            (Expect::Mismatch(w), _) => assert!(!w.is_empty(), "{n}"),
            other => panic!("{other:?}"),
        }
    }
}

// spec/exec.md §8.9: the names `slip units` printed with the flag `i`.
#[test]
fn s10_5_indeterminate_units() {
    let out = "0123456789abcdef function declared - f(int)\n\
               1123456789abcdef function declared i g(long, char *)\n\
               2123456789abcdef class derived if S<int>\n\
               3123456789abcdef function declared f h\\x5cx()\n\
               4123456789abcdef variable tu i a\\x09b\n\
               slip: warning: not a line\n";
    assert_eq!(indeterminate_units(out), set(&["g(long, char *)", "S<int>", "a\tb"]));
    assert_eq!(indeterminate_units(""), none());
}

// ---------------------------------------------------------------- §10.3 the result

// §10.3: `verified` when the run's verdict is verified.
#[test]
fn s10_3_verified() {
    assert_eq!(run_check(&Expect::Verified, "verified.json"), (CaseResult::Verified, vec![]));
}

// §10.3: a `clean` run (only indeterminate findings) is never printed as
// `verified`: the run is not verified.
#[test]
fn clean_is_never_printed_as_verified() {
    let (r, lines) = run_check(&Expect::Verified, "clean.json");
    assert_eq!(r, CaseResult::Clean);
    assert!(lines.is_empty());
    assert_eq!(r.word(), "clean");
    assert!(!r.ok() && !r.fails());
    // One observer whose findings are not all indeterminate makes it
    // DIFFERENT, by the counts or by a group.
    assert_eq!(run_check(&Expect::Verified, "different.json").0, CaseResult::Different);
    let mut v = golden("clean.json");
    set_path(&mut v, &["observers", "indeterminate_only"], Value::Int(1));
    assert_eq!(
        check(&Expect::Verified, &v, Path::new(SRC), &none(), &none()).0,
        CaseResult::Different
    );
}

// §10.3: `DIFFERENT` when an observer has another finding.
#[test]
fn s10_3_different() {
    let (r, _) = run_check(&Expect::Verified, "different.json");
    assert_eq!((r.word(), r.fails()), ("DIFFERENT", true));
}

/// Replaces the member at `path` of `v`.
fn set_path(v: &mut Value, path: &[&str], to: Value) {
    let Value::Object(members) = v else { panic!("not an object") };
    let m = members.iter_mut().find(|(k, _)| k == path[0]).unwrap();
    if path.len() == 1 {
        m.1 = to;
    } else {
        set_path(&mut m.1, &path[1..], to);
    }
}

fn found_lines() -> Vec<Wanted> {
    vec![
        wanted("changed", "f(int)", Some(("lib/a.h", 3))),
        wanted("removed", "ns::old", Some(("lib/b.cpp", 5))),
        wanted("added", "ns::fresh", Some(("lib/b.cpp", 5))),
    ]
}

// §10.1, §10.3: `found` when every line is found at its place; `removed`
// takes the pre site, `added` the post site.
#[test]
fn s10_3_found() {
    let (r, lines) = run_check(&mismatch(found_lines()), "found.json");
    assert_eq!(r, CaseResult::Found);
    assert_eq!(lines, [Line::Found, Line::Found, Line::Found]);
    // A line without a place is found by kind and name.
    let (r, lines) = run_check(&mismatch(vec![wanted("changed", "f(int)", None)]), "found.json");
    assert_eq!((r, lines), (CaseResult::Found, vec![Line::Found]));
    // `removed` reads `where.pre`: its post site is empty, and a line for
    // the post site's place would be no place, not found.
    let (_, lines) = run_check(
        &mismatch(vec![wanted("removed", "ns::old", Some(("lib/b.cpp", 5)))]),
        "found.json",
    );
    assert_eq!(lines, [Line::Found]);
    let (_, lines) =
        run_check(&mismatch(vec![wanted("changed", "f(int)", Some(("lib/a.h", 3)))]), "found.json");
    assert_eq!(lines, [Line::Found]);
}

// §10.1: a place that is one line off is a wrong place, and fails the demo.
#[test]
fn wrong_place_fails_the_demo() {
    let mut lines = found_lines();
    lines[0] = wanted("changed", "f(int)", Some(("lib/a.h", 4)));
    let (r, got) = run_check(&mismatch(lines), "found.json");
    assert_eq!(r, CaseResult::Missed);
    assert_eq!(got[0], Line::WrongPlace { file: "lib/a.h".into(), line: 3 });
    assert!(r.fails());
    // Another file is a wrong place too; `removed` is compared with its pre site.
    let (_, got) = run_check(
        &mismatch(vec![wanted("removed", "ns::old", Some(("lib/a.h", 5)))]),
        "found.json",
    );
    assert_eq!(got, [Line::WrongPlace { file: "lib/b.cpp".into(), line: 5 }]);
}

// §10.1: a line with no such group, whose name `slip units` does not flag
// indeterminate, is missing, and fails the demo.
#[test]
fn missing_fails_the_demo() {
    let mut lines = found_lines();
    lines.push(wanted("changed", "nobody()", Some(("lib/a.h", 1))));
    let (r, got) = run_check(&mismatch(lines.clone()), "found.json");
    assert_eq!(r, CaseResult::Missed);
    assert_eq!(got[3], Line::Missing);
    // Flagged in the wrong image, it is still missing.
    let (r, got) = check(
        &mismatch(lines),
        &golden("found.json"),
        Path::new(SRC),
        &none(),
        &set(&["nobody()"]),
    );
    assert_eq!((r, &got[3]), (CaseResult::Missed, &Line::Missing));
    // The missed row: a line missing, the file `missed.json`.
    let (r, _) = run_check(
        &mismatch(vec![
            wanted("changed", "f(int)", Some(("lib/a.h", 3))),
            wanted("changed", "k()", None),
        ]),
        "missed.json",
    );
    assert_eq!(r, CaseResult::Missed);
}

// §10.3: a wrong tree reported verified is MISSED, whatever its lines.
#[test]
fn s10_3_missed_when_verified() {
    let lines = vec![wanted("changed", "h(long)", None)];
    let (r, got) = check(
        &mismatch(lines),
        &golden("verified.json"),
        Path::new(SRC),
        &set(&["h(long)"]),
        &none(),
    );
    assert_eq!((r, got), (CaseResult::Missed, vec![Line::Blocked]));
}

// §10.1: blocked needs the indeterminate flag in the line's image: a line
// with no group is `blocked` only when `slip units` flags its name there.
#[test]
fn blocked_needs_the_indeterminate_flag() {
    let lines = vec![
        wanted("changed", "f(int)", Some(("lib/a.h", 3))),
        wanted("changed", "h(long)", Some(("lib/a.h", 7))),
    ];
    // The report has an indeterminate group of the name, but no flag: missing.
    let (r, got) = run_check(&mismatch(lines.clone()), "partly.json");
    assert_eq!((r, got[1].clone()), (CaseResult::Missed, Line::Missing));
    // Flagged in the post-image only, for a `changed` line: missing.
    let (r, _) = check(
        &mismatch(lines.clone()),
        &golden("partly.json"),
        Path::new(SRC),
        &none(),
        &set(&["h(long)"]),
    );
    assert_eq!(r, CaseResult::Missed);
    // Flagged in the pre-image: blocked, and the case is partly.
    let (r, got) = check(
        &mismatch(lines.clone()),
        &golden("partly.json"),
        Path::new(SRC),
        &set(&["h(long)"]),
        &none(),
    );
    assert_eq!((r, got), (CaseResult::Partly, vec![Line::Found, Line::Blocked]));
    // Its cause is the report's indeterminate group of that name.
    let w = &lines[1];
    assert_eq!(
        demo::blocked_cause(w, &golden("partly.json"), Path::new(SRC)).as_deref(),
        Some("unsupported ExprKind/CXXReflectExpr")
    );
    assert_eq!(
        demo::blocked_cause(
            &wanted("changed", "q()", None),
            &golden("partly.json"),
            Path::new(SRC)
        ),
        None
    );
    // `added` is blocked by the post-image's flag, not the pre-image's;
    // its cause is the post site's.
    let added = vec![wanted("added", "ns::later", Some(("lib/b.cpp", 9)))];
    let (r, got) = check(
        &mismatch(added.clone()),
        &golden("partly.json"),
        Path::new(SRC),
        &set(&["ns::later"]),
        &none(),
    );
    assert_eq!((r, got), (CaseResult::Missed, vec![Line::Missing]));
    let (r, got) = check(
        &mismatch(added.clone()),
        &golden("partly.json"),
        Path::new(SRC),
        &none(),
        &set(&["ns::later"]),
    );
    assert_eq!((r, got), (CaseResult::Partly, vec![Line::Blocked]));
    assert_eq!(
        demo::blocked_cause(&added[0], &golden("partly.json"), Path::new(SRC)).as_deref(),
        Some("unaccounted walk.depth")
    );
}

// §10.1, §10.3: a group without `where` is found with no place: partly.
#[test]
fn s10_3_partly_no_place() {
    let lines = vec![
        wanted("changed", "f(int)", Some(("lib/a.h", 3))),
        wanted("changed", "g()", Some(("lib/a.h", 9))),
    ];
    let (r, got) = run_check(&mismatch(lines), "partly.json");
    assert_eq!((r, got), (CaseResult::Partly, vec![Line::Found, Line::NoPlace]));
    // A group whose site on the line's side is the empty site is no place.
    let (_, got) = run_check(
        &mismatch(vec![wanted("added", "ns::fresh", Some(("lib/b.cpp", 5)))]),
        "partly.json",
    );
    assert_eq!(got, [Line::Missing]);
}

// §10.3: an observer `not_parsed` (or unverifiable, never_parsed, failed)
// leaves the case with no result: ERROR.
#[test]
fn not_parsed_is_error() {
    let (r, lines) = run_check(&Expect::Verified, "not-parsed.json");
    assert_eq!(r, CaseResult::Error("1 observer(s) not_parsed".into()));
    assert!(lines.is_empty() && r.fails());
    let (r, _) = run_check(&mismatch(found_lines()), "not-parsed.json");
    assert_eq!(r.word(), "ERROR");
    for state in ["unverifiable", "never_parsed", "failed"] {
        let mut v = golden("verified.json");
        set_path(&mut v, &["observers", state], Value::Int(2));
        let (r, _) = check(&Expect::Verified, &v, Path::new(SRC), &none(), &none());
        assert_eq!(r, CaseResult::Error(format!("2 observer(s) {state}")));
    }
    let mut v = golden("verified.json");
    set_path(&mut v, &["report"], Value::Str("other".into()));
    assert!(matches!(
        check(&Expect::Verified, &v, Path::new(SRC), &none(), &none()).0,
        CaseResult::Error(_)
    ));
    let mut v = golden("verified.json");
    set_path(&mut v, &["observers", "selected"], Value::Int(0));
    assert_eq!(
        check(&Expect::Verified, &v, Path::new(SRC), &none(), &none()).0,
        CaseResult::Error("no observer selected".into())
    );
}

// ---------------------------------------------------------------- the command

const KILN_ROWS: &str = "[tiny_project]\nurl = \"https://example.org/tiny.git\"\ntag = \"v1.0\"\n";
const CORPORA: &str = "[tiny]\nkiln = \"tiny_project\"\ncxx_std = 17\nbuild_type = \"Release\"\n\
     c_compiler = \"cc\"\ncxx_compiler = \"c++\"\nconfigure = []\ngenerate = [\"gen\"]\n";

/// The fake `slip`. `parse` copies the template store into `--store` and
/// prints the plan and its map run; `verify` writes `report.<round>.json`
/// to `--report`, prints the run of its round, and exits with
/// `verify.<round>.status` (default 1); `declare` writes `--out`; `units`
/// prints `units.pre` or `units.post`. An `act.<command>` file is run as
/// a shell script with the arguments (to edit a tree through a link).
/// Each call is logged.
const FAKE: &str = r#"#!/bin/sh
[ "$1" = --probe ] && exit 0
D=@DIR@
cmd=$1
shift
echo "$cmd $*" >> "$D/log"
[ -f "$D/act.$cmd" ] && sh "$D/act.$cmd" "$@"
store=; report=; round=0; out=; post=0
while [ $# -gt 0 ]; do
  case "$1" in
    --store) store=$2; shift ;;
    --report) report=$2; shift ;;
    --round) round=$2; shift ;;
    --out) out=$2; shift ;;
    --post-image) post=1 ;;
  esac
  shift
done
case "$cmd" in
  --version) echo "slip 0.0.0 (kiln 1.3.0)"; exit 0 ;;
  plan) mkdir -p "$store"; echo "plan $(cat "$D/plan")"; echo "observers 3"; exit 0 ;;
  parse)
    cp -R "$D/t/." "$store/" || exit 9
    echo "plan $(cat "$D/plan")"
    echo "run $store/$(cat "$D/map.run")"
    exit 0 ;;
  declare) echo decl > "$out"; echo "declared 00"; exit 0 ;;
  verify)
    [ -f "$D/report.$round.json" ] && cp "$D/report.$round.json" "$report"
    echo "run $store/$(cat "$D/verify.$round.run")"
    echo "slip: verify of round $round says this on standard error" >&2
    exit $(cat "$D/verify.$round.status" 2>/dev/null || echo 1) ;;
  units)
    if [ $post = 1 ]; then cat "$D/units.post" 2>/dev/null; else cat "$D/units.pre" 2>/dev/null; fi
    exit 0 ;;
esac
exit 99
"#;

fn d(n: u8) -> Digest {
    Digest([n; 32])
}

fn observer(main: &str, n: u64) -> Observer {
    let command = Cmd {
        directory: b"/build".to_vec(),
        argv: vec![b"c++".to_vec(), b"-c".to_vec(), main.as_bytes().to_vec()],
        env: vec![],
    };
    Observer {
        id: command.observer_id(),
        command,
        language: Language::Cxx,
        main: FileRef { path: main.as_bytes().to_vec(), size: 100 - n, content: d(n as u8) },
        cost_hint: 100 - n,
        origins: vec![Origin { configuration: 0, entry: n }],
    }
}

fn plan(src: &Path) -> Plan {
    let mains: Vec<String> =
        ["b.cpp", "c.cpp", "d.cpp"].iter().map(|m| format!("{}/lib/{m}", src.display())).collect();
    Plan {
        tool_slip: "0.0.0".into(),
        tool_kiln: "1.3.0".into(),
        configurations: vec![Configuration {
            name: "default".into(),
            database: FileRef {
                path: b"/build/compile_commands.json".to_vec(),
                size: 1,
                content: d(9),
            },
        }],
        observers: mains.iter().enumerate().map(|(i, m)| observer(m, i as u64)).collect(),
        excluded: vec![],
    }
}

fn counters(pairs: &[(&str, u64)]) -> Vec<Counter> {
    let mut v: Vec<Counter> =
        pairs.iter().map(|(n, v)| Counter { name: (*n).into(), value: *v }).collect();
    v.sort_by(|a, b| a.name.cmp(&b.name));
    v
}

fn shard(observer: Digest, phase: Phase, round: u64, c: Vec<Counter>) -> Shard {
    Shard {
        observer,
        phase,
        round,
        outcome: Outcome { status: Status::Ok, errors: 0, warnings: 0, message: String::new() },
        inputs: Some(vec![]),
        counters: c,
        digests: None,
        verdict: None,
        edits: None,
        binding: None,
        rule_use: None,
        spellings: None,
        names: None,
        negatives: None,
        found: None,
        rebuilt_use: None,
        ownership: None,
    }
}

fn record(plan: Digest, phase: Phase, round: u64, jobs: Vec<Job>) -> RunRecord {
    RunRecord {
        run: Run {
            tool_slip: "0.0.0".into(),
            tool_kiln: "1.3.0".into(),
            plan,
            phase,
            round,
            started_unix_ns: 1_700_000_000_000_000_000 + round,
            wall_ns: 2_500_000_000 + round * 1_000_000,
            cores: 4,
            workers_max: 2,
            memory_budget: 0,
            peak_rss_sum: 0,
        },
        jobs,
    }
}

/// A fixture: a repository with the `tiny` row and `bench/demo1/`, a
/// configured corpus under `corpora/` with `src/lib`, and the fake `slip`.
struct World {
    f: Fixture,
    plan: Plan,
    plan_id: Digest,
}

impl World {
    fn new(tag: &str) -> World {
        let f = Fixture::empty(tag);
        f.write("repo/bench/corpora.toml", CORPORA);
        f.write("repo/kiln/scripts/oss/projects.toml", KILN_ROWS);
        let c = rows::parse(CORPORA, KILN_ROWS).unwrap().remove(0);
        let layout = Layout::new(&f.path("corpora"), "tiny");
        std::fs::create_dir_all(&layout.build).unwrap();
        f.write("corpora/tiny/src/lib/a.h", "int x;\nvoid f(int);\nint g();\n");
        f.write("corpora/tiny/src/lib/b.cpp", "#include \"a.h\"\n");
        let commit = "c".repeat(40);
        std::fs::write(layout.fetch_stamp(), corpus::fetch_stamp(&c, &commit)).unwrap();
        let cstamp = corpus::configure_stamp(&commit, &["-S".to_owned()]);
        std::fs::write(layout.configure_stamp(), &cstamp).unwrap();
        std::fs::write(layout.generate_stamp(), corpus::generate_stamp(&cstamp, &c.generate))
            .unwrap();
        std::fs::write(layout.database(), "[]\n").unwrap();
        f.write(
            "repo/bench/demo1/demo.toml",
            "corpus = \"tiny\"\nroots = [\"lib\"]\nonly = [\"/lib/\"]\n",
        );
        f.write("repo/bench/demo1/cases/00-control/expect.txt", "verified\n");
        f.write("repo/bench/demo1/cases/00-control/README.md", "The control.\n");
        f.write(
            "repo/bench/demo1/cases/01-change/edit.txt",
            "file lib/a.h\nreplace\nvoid f(int);\nwith\nvoid f(long);\nend\n",
        );
        f.write(
            "repo/bench/demo1/cases/01-change/expect.txt",
            "mismatch\n  changed f(int) @ lib/a.h:3\n  removed ns::old @ lib/b.cpp:5\n",
        );
        f.write("repo/bench/demo1/cases/02-blocked/base", "01-change\n");
        f.write("repo/bench/demo1/cases/02-blocked/declared.txt", "# x\nrelocate a => :: b\n");
        f.write("repo/bench/demo1/cases/02-blocked/observers", "/lib/b.cpp\n\n/lib/c.cpp\n");
        f.write(
            "repo/bench/demo1/cases/02-blocked/edit.txt",
            "file lib/a.h\nreplace\nint g();\nwith\nint g(int = 0);\nend\n",
        );
        f.write(
            "repo/bench/demo1/cases/02-blocked/expect.txt",
            "mismatch\n  changed f(int) @ lib/a.h:3\n  changed h(long) @ lib/a.h:7\n",
        );
        let fake = FAKE.replace("@DIR@", &f.path("fake").display().to_string());
        f.write("bin/slip", &fake);
        std::fs::set_permissions(f.path("bin/slip"), std::fs::Permissions::from_mode(0o755))
            .unwrap();
        common::wait_executable(&f.path("bin/slip"));
        let p = plan(&layout.src);
        let plan_id = slip_model::formats::content_id(&p.to_bytes()).unwrap();
        std::fs::create_dir_all(f.path("fake")).unwrap();
        std::fs::write(f.path("fake/plan"), plan_id.to_string()).unwrap();
        let w = World { f, plan: p, plan_id };
        w.store();
        // The reports of the three rounds: verified, found, partly.
        w.report(0, "verified.json", 0);
        w.report(1, "found.json", 1);
        w.report(2, "partly.json", 1);
        w
    }

    fn src(&self) -> PathBuf {
        self.f.path("corpora/tiny/src")
    }

    /// The template store: the plan, a map run of the three observers
    /// (one with indeterminate units), and verify runs of rounds 0 to 2.
    fn store(&self) {
        let st = Store::open(&self.f.path("fake/t"), Access::Exclusive).unwrap();
        let id = st.publish_plan(&self.plan.to_bytes()).unwrap();
        let rel = |p: PathBuf| p.strip_prefix(st.root()).unwrap().display().to_string();
        let mut jobs = Vec::new();
        for (i, o) in self.plan.observers.iter().enumerate() {
            let ind = if i == 1 { 4 } else { 0 };
            let c = counters(&[
                ("digest.units", 100),
                ("digest.indeterminate", ind),
                ("digest.indeterminate.unsupported", ind),
                ("digest.indeterminate.pending", ind / 2),
                ("digest.cause.unsupported.expr.x", ind),
                ("digest.cause.pending.stmt", ind / 2),
            ]);
            let s = shard(o.id, Phase::Map, 0, c);
            st.publish_shard(&id, Phase::Map, 0, &o.id, &s.to_bytes()).unwrap();
            jobs.push(job(&s, 1 << (20 + i)));
        }
        let r = record(id, Phase::Map, 0, jobs);
        let p = st.publish_run(&id, Phase::Map, 0, r.run.started_unix_ns, &r.to_bytes()).unwrap();
        std::fs::write(self.f.path("fake/map.run"), rel(p)).unwrap();
        for round in 0..3u64 {
            let mut jobs = Vec::new();
            for (i, o) in self.plan.observers.iter().enumerate() {
                let c = counters(&[
                    ("digest.units", 101),
                    ("digest.indeterminate", if i == 1 { 4 } else { 0 }),
                    ("verify.reparsed", u64::from(round > 0) * 2),
                ]);
                let s = shard(o.id, Phase::Verify, round, c);
                st.publish_shard(&id, Phase::Verify, round, &o.id, &s.to_bytes()).unwrap();
                jobs.push(job(&s, 3 << 20));
            }
            let r = record(id, Phase::Verify, round, jobs);
            let p = st
                .publish_run(&id, Phase::Verify, round, r.run.started_unix_ns, &r.to_bytes())
                .unwrap();
            std::fs::write(self.f.path(&format!("fake/verify.{round}.run")), rel(p)).unwrap();
        }
    }

    /// Round `round` answers with the golden `name` and the status.
    fn report(&self, round: u64, name: &str, status: i32) {
        let text =
            golden_text(name, &self.src().display().to_string(), &self.plan_id.to_string(), round);
        std::fs::write(self.f.path(&format!("fake/report.{round}.json")), text).unwrap();
        std::fs::write(self.f.path(&format!("fake/verify.{round}.status")), status.to_string())
            .unwrap();
    }

    fn env(&self) -> Env {
        Env { corpus_dir: Some(self.f.path("corpora").into()), target_dir: None }
    }

    fn run(&self, args: &[&str]) -> (Result<DemoOutcome, Failure>, String) {
        let mut a: Vec<String> = args.iter().map(|s| (*s).to_owned()).collect();
        a.extend(["--slip".to_owned(), self.f.path("bin/slip").display().to_string()]);
        let mut out = Vec::new();
        let r = demo::run(&self.f.path("repo"), &a, &self.env(), &mut out);
        (r, String::from_utf8(out).unwrap())
    }

    fn log(&self) -> String {
        std::fs::read_to_string(self.f.path("fake/log")).unwrap_or_default()
    }

    fn dir(&self) -> PathBuf {
        self.f.path("corpora/tiny")
    }

    fn norm(&self, text: &str) -> String {
        text.replace(&self.dir().display().to_string(), "<corpus>")
            .replace(&self.f.path("repo").display().to_string(), "<repo>")
            .replace(&self.plan_id.to_string(), "<plan>")
    }
}

fn job(s: &Shard, rss: u64) -> Job {
    Job {
        observer: s.observer,
        attempt: 1,
        end: End::Completed,
        shard: Some(slip_model::formats::content_id(&s.to_bytes()).unwrap()),
        queued_ns: 0,
        wall_ns: 1_000_000_000,
        cpu_ns: 1_250_000_000,
        peak_rss: rss,
        estimate: 1 << 20,
    }
}

/// Compares `got` with the golden `name` under `xtask/tests/demo/`; with
/// `SLIP_BLESS=1` in the environment, writes it instead.
fn assert_golden(name: &str, got: &str) {
    let path = Path::new(GOLDEN).join(name);
    if std::env::var_os("SLIP_BLESS").is_some_and(|v| v == "1") {
        std::fs::write(&path, got).unwrap();
        return;
    }
    let want = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(got, want, "{} differs (SLIP_BLESS=1 writes it)", path.display());
}

// §10.3: the whole table of a PARTIAL run (a verified control, a found
// case, a partly case with a blocked line), exit status 4, the `slip`
// commands in order, and `demo1.json`. A second run prints the same.
#[test]
fn s10_3_partial_table() {
    let w = World::new("demo-partial");
    std::fs::write(
        w.f.path("fake/units.pre"),
        "aaaaaaaaaaaaaaaa function declared i h(long)\nbbbbbbbbbbbbbbbb function declared - k()\n",
    )
    .unwrap();
    let (r, out) = w.run(&["1", "-j", "3"]);
    assert_eq!(r, Ok(DemoOutcome::Partial), "{out}");
    assert_golden("partial.txt", &out);
    let src = w.src().display().to_string();
    let tree = |c: &str| w.dir().join("demo1/trees").join(c).join("lib").display().to_string();
    let store = w.dir().join("demo1/store").display().to_string();
    let p = w.plan_id.to_string();
    let db = w.dir().join("build/compile_commands.json").display().to_string();
    let report = |c: &str| w.dir().join(format!("demo1/{c}.json")).display().to_string();
    let decl = w.dir().join("demo1/02-blocked.decl").display().to_string();
    let from = w.f.path("repo/bench/demo1/cases/02-blocked/declared.txt").display().to_string();
    let image = |c: &str| format!("--root {src}/lib --post {}", tree(c));
    let verify = |c: &str, k: u64, extra: &str| {
        format!(
            "verify --plan {p} --store {store} --round {k} {}{extra} --findings 100000 --locate 1024 \
             --report {} --only /lib/ -j 3\n",
            image(c),
            report(c)
        )
    };
    let units = |post: &str| {
        format!(
            "units --plan {p} --store {store}{post} {} --only /lib/b.cpp --only /lib/c.cpp --match h(long)\n",
            image("02-blocked")
        )
    };
    assert_eq!(
        w.log(),
        format!(
            "--version \nplan --db {db} --store {store}\nparse --plan {p} --store {store} --only /lib/ -j 3\n{}{}\
             declare --plan {p} --store {store} --from {from} --out {decl} --owned {src}/lib {} \
             --only /lib/b.cpp --only /lib/c.cpp\n{}{}{}",
            verify("00-control", 0, ""),
            verify("01-change", 1, ""),
            image("02-blocked"),
            verify("02-blocked", 2, &format!(" --declared {decl}")),
            units(""),
            units(" --post-image"),
        )
    );
    // The trees are removed, the reports stay, demo1.json is written.
    assert!(!w.dir().join("demo1/trees").exists());
    assert!(w.dir().join("demo1/01-change.json").is_file());
    let json = std::fs::read_to_string(w.dir().join("demo1/demo1.json")).unwrap();
    assert_golden("partial.json", &w.norm(&json));
    assert!(!json.contains(&w.dir().display().to_string()), "no absolute corpus path");

    // A second run prints the same table and writes the same document.
    let (r, again) = w.run(&["1", "-j", "3"]);
    assert_eq!((r, again), (Ok(DemoOutcome::Partial), out));
    assert_eq!(std::fs::read_to_string(w.dir().join("demo1/demo1.json")).unwrap(), json);
}

// §10.3: `demo 1: ok` (exit status 0) over `--case` selections whose
// cases are all verified or found; `--keep` keeps the trees; a case's
// base tree is made though the base is not run.
#[test]
fn s10_3_ok() {
    let w = World::new("demo-ok");
    let (r, out) = w.run(&["1", "--case", "00-control", "--case", "01-change", "--keep"]);
    assert_eq!(r, Ok(DemoOutcome::Ok), "{out}");
    assert!(out.ends_with("demo 1: ok\n"), "{out}");
    assert!(w.dir().join("demo1/trees/01-change/lib/a.h").is_file());
    // 02 alone, its round 2, answered found: its base 01's tree is made, 01 is not run.
    w.report(2, "found.json", 1);
    std::fs::write(w.f.path("fake/log"), "").unwrap();
    let (r, out) = w.run(&["1", "--case", "02-blocked"]);
    assert!(matches!(r, Ok(DemoOutcome::Failed)), "{out}"); // its lines are not those of found.json
    assert!(!w.log().contains("--round 1 "), "{}", w.log());
    assert!(out.contains("02-blocked  MISSED"), "{out}");
}

// §10.3: a DIFFERENT, MISSED or ERROR case fails the demo (exit status 1)
// and is named in the last line.
#[test]
fn s10_3_failed() {
    let w = World::new("demo-failed");
    w.report(0, "different.json", 1);
    w.report(1, "not-parsed.json", 1);
    w.report(2, "missed.json", 1);
    let (r, out) = w.run(&["1"]);
    assert_eq!(r, Ok(DemoOutcome::Failed), "{out}");
    assert!(
        out.contains("00-control  DIFFERENT observers verified 1 indeterminate_only 1 other 1"),
        "{out}"
    );
    assert!(out.contains("\n  changed f(int) observers 3\n"), "{out}");
    assert!(out.contains("01-change   ERROR     "), "{out}");
    assert!(out.contains("\n  error 1 observer(s) not_parsed\n"), "{out}");
    assert!(out.contains("\n  MISSING  changed h(long) @ lib/a.h:7\n"), "{out}");
    assert!(out.ends_with("demo 1: FAILED (00-control, 01-change, 02-blocked)\n"), "{out}");
    // A `slip verify` that exits 3 is an ERROR of its case, not of the run.
    std::fs::write(w.f.path("fake/verify.0.status"), "3").unwrap();
    let (r, out) = w.run(&["1", "--case", "00-control"]);
    assert_eq!(r, Ok(DemoOutcome::Failed));
    assert!(
        out.contains(
            "00-control  ERROR\n  error `slip verify` exited with status 3 \
             (its standard error: demo1/00-control.log)\n"
        ),
        "{out}"
    );
    // `slip`'s standard error is kept out of the table, in the case's log.
    assert!(!out.contains("standard error\n"), "{out}");
    let log = std::fs::read_to_string(w.f.path("corpora/tiny/demo1/00-control.log")).unwrap();
    assert_eq!(log, "slip: verify of round 0 says this on standard error\n");
    // A report of another round is an error of the case.
    w.report(0, "verified.json", 0);
    let text = std::fs::read_to_string(w.f.path("fake/report.1.json")).unwrap();
    std::fs::write(w.f.path("fake/report.0.json"), text).unwrap();
    let (_, out) = w.run(&["1", "--case", "00-control"]);
    assert!(out.contains("the report is not of plan"), "{out}");
}

// §10.3 step 4: an edit that went through a link to a pristine file is an
// error of the demo.
#[test]
fn an_edit_through_a_link_is_an_error() {
    let w = World::new("demo-link");
    // `slip verify` of round 0 writes into the control's tree, whose files
    // are links to the pristine ones.
    std::fs::write(
        w.f.path("fake/act.verify"),
        "case \"$*\" in *'--round 0 '*) echo oops >> \"$(dirname \"$0\")/../corpora/tiny/demo1/trees/00-control/lib/a.h\" ;; esac\n",
    )
    .unwrap();
    let (r, out) = w.run(&["1", "--case", "00-control", "--case", "01-change"]);
    assert_eq!(r, Ok(DemoOutcome::Failed), "{out}");
    assert!(out.contains("00-control  verified"), "{out}");
    assert!(out.ends_with("demo 1: FAILED (an edit went through a link: lib/a.h)\n"), "{out}");
}

// §10: usage errors, and errors before any case.
#[test]
fn s10_usage_and_errors() {
    let w = World::new("demo-usage");
    for a in [
        &[][..],
        &["2"],
        &["1", "--keep", "--keep"],
        &["1", "-j"],
        &["1", "--case", "nosuch"],
        &["1", "--only", "x"],
    ] {
        let mut out = Vec::new();
        let args: Vec<String> = a.iter().map(|s| (*s).to_owned()).collect();
        let r = demo::run(&w.f.path("repo"), &args, &w.env(), &mut out);
        assert!(matches!(r, Err(Failure::Usage(_))), "{a:?}: {r:?}");
    }
    assert_eq!(w.log(), "");
    // An input that does not read names its file and line.
    w.f.write("repo/bench/demo1/cases/01-change/expect.txt", "mismatch\n  moved x\n");
    let (r, _) = w.run(&["1"]);
    assert_eq!(
        r,
        Err(Failure::Error(
            "bench/demo1/cases/01-change/expect.txt: line 2: `moved` is not a kind (changed, \
             removed, added, derived_changed)"
                .into()
        ))
    );
    w.f.write("repo/bench/demo1/cases/01-change/expect.txt", "verified\n");
    w.f.write("repo/bench/demo1/cases/01-change/edit.txt", "file src/a.h\nreplace\nx\nwith\nend\n");
    let (r, _) = w.run(&["1"]);
    assert_eq!(
        r,
        Err(Failure::Error(
            "bench/demo1/cases/01-change/edit.txt: line 2: `src/a.h` is not under a root (lib)"
                .into()
        ))
    );
    assert_eq!(w.log(), "", "slip is not run on an input error");
    // A corpus that is not configured.
    std::fs::remove_file(w.dir().join("generate.stamp")).unwrap();
    w.f.write(
        "repo/bench/demo1/cases/01-change/edit.txt",
        "file lib/a.h\nreplace\nint x;\nwith\nend\n",
    );
    let (r, _) = w.run(&["1"]);
    assert_eq!(
        r,
        Err(Failure::Error(
            "corpus `tiny` is not configured: run `cargo xtask corpus configure tiny`".into()
        ))
    );
    // Through the executable: a usage error is exit status 2.
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["demo", "2"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.starts_with("xtask demo: there is no demo `2` (only 1)\nusage:"), "{stderr}");
}
