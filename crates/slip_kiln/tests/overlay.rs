//! `spec/edits.md` §7.1, `docs/wp/WP-066.md`: `parse_observer_with` with
//! `ParseOptions::overlay` (Kiln's overlay, its WP-2390). Each case parses
//! a tree in a directory under the work root (`common::work_root()`) with an overlay and
//! checks it against the reference: the same command without an overlay
//! over a copy of the tree in which the overlaid files hold the overlay's
//! bytes, the two roots made the same string.
//!
//! The parse changes the process's current directory, so every test here
//! holds [`LOCK`] and uses absolute paths only.

mod common;

use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use slip_kiln::written::written_refs;
use slip_kiln::{ParseOptions, Unit, parse_observer, parse_observer_with};
use slip_model::hash::content_digest;
use slip_model::{Command, Digest, FileRef, Language, Observer, Origin, Phase, Shard, Status};

static LOCK: Mutex<()> = Mutex::new(());

/// One overlay entry: a real path and its replacement bytes.
type Entry = (Vec<u8>, Arc<[u8]>);

/// A tree `a/` under the work root and, for the reference, its copy
/// `b/`; removed on drop.
struct Tree {
    base: PathBuf,
    _lock: MutexGuard<'static, ()>,
}

impl Tree {
    fn new(name: &str) -> Tree {
        let lock = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
        let base =
            common::work_root().join(format!("slip-kiln-overlay-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("a")).unwrap();
        let base = std::fs::canonicalize(&base).unwrap();
        Tree { base, _lock: lock }
    }

    fn a(&self) -> PathBuf {
        self.base.join("a")
    }

    fn b(&self) -> PathBuf {
        self.base.join("b")
    }

    fn write(&self, rel: &str, text: &str) -> PathBuf {
        let p = self.a().join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, text).unwrap();
        p
    }

    fn symlink(&self, rel: &str, target: &str) {
        std::os::unix::fs::symlink(target, self.a().join(rel)).unwrap();
    }

    /// The entry replacing `rel` of `a/` with `text`.
    fn entry(&self, rel: &str, text: &str) -> Entry {
        (self.a().join(rel).as_os_str().as_bytes().to_vec(), Arc::from(text.as_bytes()))
    }

    /// `b/`: a copy of `a/` (symbolic links as links) in which each file
    /// of `replaced` holds the overlay's bytes.
    fn reference_tree(&self, replaced: &[(&str, &str)]) {
        fn copy(from: &Path, to: &Path) {
            std::fs::create_dir_all(to).unwrap();
            for e in std::fs::read_dir(from).unwrap() {
                let e = e.unwrap();
                let (src, dst) = (e.path(), to.join(e.file_name()));
                let kind = e.file_type().unwrap();
                if kind.is_symlink() {
                    std::os::unix::fs::symlink(std::fs::read_link(&src).unwrap(), &dst).unwrap();
                } else if kind.is_dir() {
                    copy(&src, &dst);
                } else {
                    std::fs::copy(&src, &dst).unwrap();
                }
            }
        }
        let _ = std::fs::remove_dir_all(self.b());
        copy(&self.a(), &self.b());
        for (rel, text) in replaced {
            std::fs::write(self.b().join(rel), text).unwrap();
        }
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

fn observer(dir: &Path, argv: &[&str]) -> Observer {
    Observer {
        id: Digest([7; 32]),
        command: Command {
            directory: dir.as_os_str().as_bytes().to_vec(),
            argv: argv.iter().map(|a| a.as_bytes().to_vec()).collect(),
            env: Vec::new(),
        },
        language: Language::Cxx,
        main: FileRef { path: Vec::new(), size: 0, content: Digest([0; 32]) },
        cost_hint: 1,
        origins: vec![Origin { configuration: 0, entry: 0 }],
    }
}

/// `entries` sorted, as the interface asks, and borrowed as it takes them.
fn sorted(entries: &mut [Entry]) -> Vec<(&[u8], Arc<[u8]>)> {
    entries.sort_by(|x, y| x.0.cmp(&y.0));
    entries.iter().map(|(p, b)| (p.as_slice(), Arc::clone(b))).collect()
}

/// The shard of `argv` in `dir` with `overlay`, and what `f` saw.
fn parse_with<R: Send>(
    dir: &Path,
    argv: &[&str],
    overlay: &[(&[u8], Arc<[u8]>)],
    written: bool,
    f: impl FnOnce(&Unit<'_>) -> R + Send,
) -> Result<(Shard, Option<R>), String> {
    let mut options = ParseOptions::default();
    options.overlay = overlay;
    options.written = written;
    parse_observer_with(&observer(dir, argv), Phase::Map, 3, options, f)
}

fn parse(dir: &Path, argv: &[&str], overlay: &[(&[u8], Arc<[u8]>)]) -> Result<Shard, String> {
    parse_with(dir, argv, overlay, false, |_| ()).map(|(s, _)| s)
}

/// `shard` with every path and message under `from` moved under `to`.
fn rerooted(mut shard: Shard, from: &Path, to: &Path) -> Shard {
    let (from, to) = (from.as_os_str().as_bytes(), to.as_os_str().as_bytes());
    for f in shard.inputs.iter_mut().flatten() {
        if let Some(rest) = f.path.strip_prefix(from) {
            f.path = [to, rest].concat();
        }
    }
    let (from, to) = (String::from_utf8_lossy(from), String::from_utf8_lossy(to));
    shard.outcome.message = shard.outcome.message.replace(&*from, &to);
    shard
}

/// Parses `argv` in `a/<dir>` with the overlay `replaced` and checks the
/// shard against the reference (the copy `b/` with the files replaced, no
/// overlay); returns the overlay's shard.
fn checked(tree: &Tree, dir: &str, argv: &[&str], replaced: &[(&str, &str)]) -> Shard {
    let mut entries: Vec<Entry> = replaced.iter().map(|(r, t)| tree.entry(r, t)).collect();
    let overlay = sorted(&mut entries);
    let shard = parse(&tree.a().join(dir), argv, &overlay).expect("the overlaid parse");
    shard.validate().expect("a valid shard");
    tree.reference_tree(replaced);
    let reference = parse(&tree.b().join(dir), argv, &[]).expect("the reference parse");
    assert_eq!(
        rerooted(shard.clone(), &tree.a(), &tree.b()),
        reference,
        "the overlaid parse against the reference"
    );
    shard
}

/// The input of `shard` at `path`.
fn input<'s>(shard: &'s Shard, path: &Path) -> Option<&'s FileRef> {
    let p = path.as_os_str().as_bytes();
    shard.inputs.as_ref().expect("inputs").iter().find(|f| f.path == p)
}

/// `(size, digest)` of `text`.
fn content(text: &str) -> (u64, Digest) {
    (text.len() as u64, content_digest(text.as_bytes()))
}

fn of(f: &FileRef) -> (u64, Digest) {
    (f.size, f.content)
}

#[test]
fn main_file_overlaid() {
    let t = Tree::new("main");
    let main = t.write("main.cpp", "int x = ;\n");
    let post = "int y = 1;\nint z = y;\n";
    let shard = checked(&t, "", &["clang++", "-c", "main.cpp"], &[("main.cpp", post)]);
    assert_eq!(
        shard.outcome.status,
        Status::Ok,
        "the disk's text has an error, the overlay's none"
    );
    assert_eq!(of(input(&shard, &main).unwrap()), content(post));
}

#[test]
fn header_by_two_spellings_and_a_symbolic_link() {
    let t = Tree::new("header");
    let header = t.write("inc/h.h", "extern int disk;\n");
    t.symlink("link.h", "inc/h.h");
    let main = t.write(
        "src/main.cpp",
        "#include \"../inc/h.h\"\n#include \"../inc/./h.h\"\n#include \"../link.h\"\nint m = post;\n",
    );
    let post = "extern int post;\n";
    let shard = checked(&t, "src", &["clang++", "-c", "main.cpp"], &[("inc/h.h", post)]);
    assert_eq!(shard.outcome.status, Status::Ok, "every spelling reads the overlay: {shard:?}");
    // The overlaid file's entry is the overlay's, the others the disk's.
    assert_eq!(of(input(&shard, &header).unwrap()), content(post));
    let disk = std::fs::read_to_string(&main).unwrap();
    assert_eq!(of(input(&shard, &main).unwrap()), content(&disk));
    assert_eq!(shard.inputs.as_ref().unwrap().len(), 2, "one entry for the header: {shard:?}");
}

#[test]
fn overlay_adds_and_removes_an_include() {
    let t = Tree::new("include");
    let main = t.write("main.cpp", "int m;\n");
    let x = t.write("x.h", "int x;\n");
    let y = t.write("y.h", "int y;\n");
    let other = t.write("other.cpp", "#include \"y.h\"\nint o;\n");
    let added = checked(
        &t,
        "",
        &["clang++", "-c", "main.cpp"],
        &[("main.cpp", "#include \"x.h\"\nint m;\n")],
    );
    assert!(input(&added, &x).is_some(), "the added include is an input: {added:?}");
    assert!(input(&added, &main).is_some());
    let removed = checked(&t, "", &["clang++", "-c", "other.cpp"], &[("other.cpp", "int o;\n")]);
    assert!(input(&removed, &y).is_none(), "the removed include is not: {removed:?}");
    assert_eq!(removed.inputs.as_ref().unwrap().len(), 1);
    assert!(input(&removed, &other).is_some());
}

#[test]
fn empty_overlay_is_parse_observer() {
    let t = Tree::new("empty");
    t.write("h.h", "int h;\n");
    t.write("main.cpp", "#include \"h.h\"\nint m = h;\n");
    let o = observer(&t.a(), &["clang++", "-c", "main.cpp"]);
    let plain = parse_observer(&o, Phase::Map, 3).expect("parse_observer");
    let empty = parse(&t.a(), &["clang++", "-c", "main.cpp"], &[]).expect("an empty overlay");
    assert_eq!(plain.to_bytes(), empty.to_bytes(), "byte for byte once encoded");
}

#[test]
fn overlaid_file_differing_on_disk_is_not_a_change() {
    // The disk's file is not what the unit read, and is never compared:
    // an `#embed` resource, which Slip digests itself (Kiln keeps no
    // bytes of it), is digested from the overlay too.
    let t = Tree::new("disk");
    let data = t.write("data.bin", "disk bytes");
    let header = t.write("h.h", "extern int disk;\n");
    t.write(
        "main.cpp",
        "#include \"h.h\"\nconst char d[] = {\n#embed \"data.bin\"\n};\nint m = post;\n",
    );
    let replaced = [("h.h", "extern int post;\n"), ("data.bin", "overlay")];
    let shard = checked(&t, "", &["clang++", "-std=c++26", "-c", "main.cpp"], &replaced);
    assert_eq!(shard.outcome.status, Status::Ok, "{shard:?}");
    assert_eq!(of(input(&shard, &header).unwrap()), content("extern int post;\n"));
    assert_eq!(of(input(&shard, &data).unwrap()), content("overlay"));
}

#[test]
fn overlay_path_that_does_not_exist_is_err() {
    let t = Tree::new("missing");
    t.write("main.cpp", "int m;\n");
    let mut entries = vec![t.entry("gone.h", "int g;\n")];
    let e = parse(&t.a(), &["clang++", "-c", "main.cpp"], &sorted(&mut entries)).unwrap_err();
    assert!(e.contains("gone.h"), "{e}");
}

#[test]
fn relative_entry_is_err() {
    // Second review F7: a relative entry would resolve against the
    // process's directory, not the command's, and could overlay a decoy.
    let t = Tree::new("relative");
    t.write("main.cpp", "int m;\n");
    let entries: Vec<(&[u8], Arc<[u8]>)> = vec![(b"main.cpp", Arc::from(&b"int x;\n"[..]))];
    let e = parse(&t.a(), &["clang++", "-c", "main.cpp"], &entries).unwrap_err();
    assert!(e.contains("main.cpp") && e.contains("not an absolute path"), "{e}");
}

#[test]
fn an_input_reached_by_another_case_is_overlaid() {
    // Second review F8: Kiln matches an entry to a file by identity, and
    // the shard's input keeps the spelling the unit reached it by, so on
    // a case-insensitive volume the input `h.h` is the entry `H.h`'s; it
    // is `Unit::overlaid` that says so, never the path's text.
    let t = Tree::new("case");
    t.write("H.h", "extern int disk;\n");
    t.write("main.cpp", "#include \"h.h\"\nint m = post;\n");
    if std::fs::metadata(t.a().join("h.h")).is_err() {
        eprintln!("skipped: the volume is case-sensitive, so `h.h` names no file");
        return;
    }
    let mut entries = vec![t.entry("H.h", "extern int post;\n")];
    let overlay = sorted(&mut entries);
    let (shard, seen) = parse_with(&t.a(), &["clang++", "-c", "main.cpp"], &overlay, false, |u| {
        let sm = u.sm();
        sm.file_ids()
            .filter_map(|f| u.real_path(f).map(|p| (p.to_vec(), u.overlaid(f))))
            .collect::<Vec<_>>()
    })
    .expect("the parse");
    assert_eq!(shard.outcome.status, Status::Ok, "the overlay's bytes were read: {shard:?}");
    let header = t.a().join("h.h");
    let header = header.as_os_str().as_bytes();
    let seen = seen.expect("a unit");
    assert!(
        seen.iter().any(|(p, overlaid)| p.eq_ignore_ascii_case(header) && *overlaid),
        "the header's buffer is the overlay's: {seen:?}"
    );
    let input = shard.inputs.as_ref().unwrap().iter().find(|f| f.path.eq_ignore_ascii_case(header));
    assert_eq!(of(input.expect("the header is an input")), content("extern int post;\n"));
}

#[test]
fn two_entries_of_one_file_are_err() {
    let t = Tree::new("twice");
    t.write("main.cpp", "#include \"h.h\"\n");
    t.write("h.h", "int h;\n");
    t.symlink("link.h", "h.h");
    // Two names of one file (identity); the overlay is in memory and
    // replaces neither (spec/harness.md §11.4).
    std::fs::hard_link(t.a().join("h.h"), t.a().join("hard.h")).unwrap();
    for other in ["link.h", "hard.h"] {
        let mut entries = vec![t.entry("h.h", "int a;\n"), t.entry(other, "int b;\n")];
        let e = parse(&t.a(), &["clang++", "-c", "main.cpp"], &sorted(&mut entries)).unwrap_err();
        assert!(e.contains("one file"), "{other}: {e}");
    }
}

#[test]
fn overlay_with_written_references() {
    // The table's ranges are offsets in the overlay's bytes.
    let t = Tree::new("written");
    t.write("main.cpp", "int f();\n");
    let post = "int f();\nint g = f();\n";
    let mut entries = vec![t.entry("main.cpp", post)];
    let overlay = sorted(&mut entries);
    let (shard, seen) = parse_with(&t.a(), &["clang++", "-c", "main.cpp"], &overlay, true, |u| {
        let main = u.main_file().expect("a main file");
        assert!(u.overlaid(main), "the main file is the overlay's");
        let sm = u.sm();
        written_refs(u)
            .iter()
            .map(|r| {
                let (_, lo) = sm.offset_of(sm.spelling_loc(r.name.lo)).expect("lo");
                let (_, hi) = sm.offset_of(sm.spelling_loc(r.name.hi)).expect("hi");
                (lo as usize, hi as usize)
            })
            .collect::<Vec<_>>()
    })
    .expect("the parse");
    assert_eq!(shard.outcome.status, Status::Ok);
    let spans = seen.expect("a unit");
    let call = post.rfind("f()").unwrap();
    // On disk the file ends at offset 9: the call is past it.
    assert!(
        spans.contains(&(call, call + 1)),
        "the call's name at its offset in the overlay's bytes: {spans:?}"
    );
}
