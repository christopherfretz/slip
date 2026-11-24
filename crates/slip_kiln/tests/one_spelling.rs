//! WP-147 item 1 (`spec/landed/edits/WP-147.md`): what Kiln does with a
//! header included under another spelling than its directory entry's,
//! and where `slip_kiln` makes such a path a real path (`spec/model.md`
//! §8.4 "Inputs", `Unit::real_path`) and decides ownership on it
//! (`Sites::is_owned`, `spec/edits.md` §2.1).
//!
//! The trees are `slip_kiln::tree::Tree`s under `CARGO_TARGET_TMPDIR`, on
//! the workspace's volume, which finds a name in any case (and in either
//! Unicode normalisation) and keeps the spelling it was asked with. Each
//! test skips, saying so, on a volume that does not. The tests of Kiln's
//! behaviour pass; the tests of the defects WP-147 item 2 fixed assert
//! the right result (the inventory states what they gave before).

use std::path::Path;

use slip_kiln::kiln::source::FileOrigin;
use slip_kiln::sites::Sites;
use slip_kiln::tree::Tree;
use slip_kiln::{ParseOptions, Unit};
use slip_model::transform::Ownership;
use slip_model::{Shard, Status};

fn tree(name: &str, files: &[(&str, &str)]) -> Tree {
    let files: Vec<(&str, &[u8])> = files.iter().map(|(p, t)| (*p, t.as_bytes())).collect();
    Tree::new(Path::new(env!("CARGO_TARGET_TMPDIR")), name, &files).expect("the tree")
}

/// Whether the tree's volume finds `rel` (written) under `other`; says so
/// and returns false when it does not.
fn folds(t: &Tree, other: &str, test: &str) -> bool {
    let found = std::fs::symlink_metadata(t.root().join(other)).is_ok();
    if !found {
        eprintln!(
            "{test}: skipped: {} is on a volume where `{other}` names no file",
            t.root().display()
        );
    }
    found
}

/// One buffer read from disk: Kiln's name for it (`SourceManager::file_name`,
/// the path as first loaded) and `slip_kiln`'s real path.
#[derive(Debug)]
struct Buffer {
    name: String,
    real: Option<String>,
}

fn buffers(u: &Unit<'_>) -> Vec<Buffer> {
    let sm = u.sm();
    sm.file_ids()
        .filter(|&f| sm.file_origin(f) == FileOrigin::Disk)
        .map(|f| Buffer {
            name: sm.file_name(f).to_owned(),
            real: u.real_path(f).map(|p| String::from_utf8_lossy(p).into_owned()),
        })
        .collect()
}

fn parse(t: &Tree, main: &str) -> (Shard, Vec<Buffer>) {
    let obs = t.observer(main, "c++17", &[]);
    let (shard, b) = t.parse(&obs, &ParseOptions::default(), buffers).expect("the parse");
    (shard, b.expect("a parse happened"))
}

/// The buffers other than the main file.
fn headers(b: &[Buffer], main: &str) -> Vec<String> {
    b.iter().filter(|b| !b.name.ends_with(main)).map(|b| b.name.clone()).collect()
}

fn inputs(shard: &Shard) -> Vec<String> {
    let all = shard.inputs.as_ref().expect("inputs");
    all.iter().map(|f| String::from_utf8_lossy(&f.path).into_owned()).collect()
}

/// What Kiln does with `#include "inc/Foo.h"` for the file `inc/foo.h`
/// (inventory row K1): one buffer, named as the `#include` spelled it
/// (Kiln's header search joins the directory and the written name and
/// names the buffer by the path as first loaded), and no diagnostic of
/// its own: as many warnings as the same unit with the include spelled as
/// on disk (Kiln has the group `nonportable-include-path` and no warning
/// in it; clang warns here, `-Wnonportable-include-path`).
#[test]
fn kiln_include_in_another_case() {
    let t = tree(
        "one-spelling-kiln-case",
        &[("inc/foo.h", "int foo_v;\n"), ("main.cpp", "#include \"inc/Foo.h\"\nint m = foo_v;\n")],
    );
    if !folds(&t, "inc/Foo.h", "kiln_include_in_another_case") {
        return;
    }
    let (shard, b) = parse(&t, "main.cpp");
    assert_eq!(shard.outcome.status, Status::Ok, "{:?}", shard.outcome);
    // The control: the same unit with the include spelled as on disk.
    let c = tree(
        "one-spelling-kiln-case-control",
        &[("inc/foo.h", "int foo_v;\n"), ("main.cpp", "#include \"inc/foo.h\"\nint m = foo_v;\n")],
    );
    let (control, _) = parse(&c, "main.cpp");
    // The ruling's Q2: no counter where nothing was respelled, so that the
    // shard is what it was before WP-147.
    assert_eq!(respelled(&control), None, "{:?}", control.counters);
    assert_eq!(
        shard.outcome.warnings, control.outcome.warnings,
        "no diagnostic of its own: {:?} against {:?}",
        shard.outcome, control.outcome
    );
    let h = headers(&b, "main.cpp");
    assert_eq!(h.len(), 1, "one buffer: {b:?}");
    assert!(h[0].ends_with("/inc/Foo.h"), "named as included: {b:?}");
}

/// One header under two spellings in one unit (row K2): Kiln identifies
/// a file by device and inode (`kiln_source` `FileKey::Unique`), so the
/// two spellings are one buffer, named by the first; `#pragma once` and
/// an include guard keep it to one entry (the unit is `ok`); with
/// neither it is entered twice, exactly as one spelling twice is (the
/// redefinition of `S` is diagnosed in both). The shard has one input
/// for it.
#[test]
fn kiln_one_header_under_two_spellings() {
    let once = "#pragma once\nstruct S {};\n";
    let guard = "#ifndef FOO_H\n#define FOO_H\nstruct S {};\n#endif\n";
    let neither = "struct S {};\n";
    for (kind, header, want) in [
        ("once", once, Status::Ok),
        ("guard", guard, Status::Ok),
        ("neither", neither, Status::Diagnosed),
    ] {
        for (first, second) in [("foo.h", "FOO.H"), ("FOO.H", "foo.h"), ("foo.h", "foo.h")] {
            let main = format!("#include \"inc/{first}\"\n#include \"inc/{second}\"\nS s;\n");
            let t = tree(
                &format!("one-spelling-kiln-{kind}"),
                &[("inc/foo.h", header), ("main.cpp", &main)],
            );
            if !folds(&t, "inc/FOO.H", "kiln_one_header_under_two_spellings") {
                return;
            }
            let (shard, b) = parse(&t, "main.cpp");
            let case = format!("{kind}: {first}, then {second}");
            assert_eq!(shard.outcome.status, want, "{case}: {:?}", shard.outcome);
            let h = headers(&b, "main.cpp");
            assert_eq!(h.len(), 1, "{case}: one buffer: {b:?}");
            assert!(h[0].ends_with(&format!("/inc/{first}")), "{case}: the first spelling: {b:?}");
            let i = inputs(&shard);
            assert_eq!(i.len(), 2, "{case}: one input for the header: {i:?}");
        }
    }
}

/// `spec/model.md` §8.4 "Inputs" (row K3): an input reached through
/// another case is named by its spelling on disk. Before WP-147 the input (and
/// `Unit::real_path`) is `<root>/inc/Foo.h`: `std::fs::canonicalize`
/// keeps the case it was asked with.
#[test]
fn s8_4_input_has_the_spelling_on_disk() {
    let t = tree(
        "one-spelling-input-case",
        &[("inc/foo.h", "int foo_v;\n"), ("main.cpp", "#include \"inc/Foo.h\"\nint m = foo_v;\n")],
    );
    if !folds(&t, "inc/Foo.h", "s8_4_input_has_the_spelling_on_disk") {
        return;
    }
    let (shard, b) = parse(&t, "main.cpp");
    let want = format!("{}/inc/foo.h", t.root().display());
    assert!(inputs(&shard).contains(&want), "{:?}", inputs(&shard));
    let real: Vec<&str> = b.iter().filter_map(|b| b.real.as_deref()).collect();
    assert!(real.contains(&want.as_str()), "Unit::real_path: {real:?}");
    // The ruling's Q2: the shard counts the input respelled.
    assert_eq!(respelled(&shard), Some(1), "{:?}", shard.counters);
}

/// `inputs.respelled`, when the shard has it.
fn respelled(shard: &Shard) -> Option<u64> {
    shard.counters.iter().find(|c| c.name == "inputs.respelled").map(|c| c.value)
}

/// The same with another Unicode normalisation (row K4): the file is
/// written `café.h` in NFC (`c3 a9`), included in NFD (`65 cc 81`). The
/// volume finds it (macOS, and this host's mount of a macOS volume,
/// fold normalisation as they fold case), and a byte comparison against
/// the directory's listing tells the two apart. Before WP-147 the input was the
/// NFD spelling.
#[test]
fn s8_4_input_in_another_normalisation_has_the_spelling_on_disk() {
    let (nfc, nfd) = ("caf\u{e9}.h", "cafe\u{301}.h");
    let main = format!("#include \"{nfd}\"\nint m = cafe_v;\n");
    let t = tree("one-spelling-input-nfd", &[(nfc, "int cafe_v;\n"), ("main.cpp", &main)]);
    if !folds(&t, nfd, "s8_4_input_in_another_normalisation_has_the_spelling_on_disk") {
        return;
    }
    let (shard, _) = parse(&t, "main.cpp");
    assert_eq!(shard.outcome.status, Status::Ok, "{:?}", shard.outcome);
    let want = format!("{}/{nfc}", t.root().display());
    assert!(inputs(&shard).contains(&want), "{:?}", inputs(&shard));
}

/// The ownership of the request in WP-147's case: `src/` owned,
/// `src/vendor` read-only.
fn vendor_ownership(t: &Tree) -> Ownership {
    let root = t.root().to_str().unwrap();
    Ownership {
        owned: vec![format!("{root}/src").into_bytes()],
        readonly: vec![format!("{root}/src/vendor").into_bytes()],
    }
}

/// For each buffer of `include`'s header: whether `Sites::is_owned` says
/// it is owned.
fn header_owned(include: &str) -> Option<(Tree, Vec<(String, bool)>)> {
    let main = format!("#include \"{include}\"\nint m = vendor_v;\n");
    let t = tree(
        "one-spelling-sites",
        &[("src/vendor/x.h", "int vendor_v;\n"), ("src/main.cpp", &main)],
    );
    if !folds(&t, &format!("src/{include}"), "header_owned") {
        return None;
    }
    let own = vendor_ownership(&t);
    let obs = t.observer("src/main.cpp", "c++17", &[]);
    let (_, got) = t
        .parse(&obs, &ParseOptions::default(), |u| {
            let sites = Sites::new(u, &own);
            let sm = u.sm();
            sm.file_ids()
                .filter(|&f| sm.file_name(f).ends_with("x.h"))
                .map(|f| (sm.file_name(f).to_owned(), sites.is_owned(f)))
                .collect::<Vec<_>>()
        })
        .expect("the parse");
    Some((t, got.expect("a parse happened")))
}

/// `spec/edits.md` §2.1 (row K5, the control): the read-only header
/// included by its own spelling is not owned.
#[test]
fn s2_1_sites_readonly_header_by_its_spelling() {
    let Some((_t, got)) = header_owned("vendor/x.h") else { return };
    assert_eq!(got.len(), 1, "{got:?}");
    assert!(!got[0].1, "read-only: {got:?}");
}

/// `spec/edits.md` §2.1 (row K5): the read-only header included as
/// `Vendor/x.h` is still read-only. Before WP-147 `Sites::is_owned` was true: its
/// real path is `<root>/src/Vendor/x.h`, which `src/vendor` does not
/// cover, so a rename edits it.
#[test]
fn s2_1_sites_readonly_header_through_another_case() {
    let Some((_t, got)) = header_owned("Vendor/x.h") else { return };
    assert_eq!(got.len(), 1, "{got:?}");
    assert!(!got[0].1, "read-only under any spelling: {got:?}");
}
