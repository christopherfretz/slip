//! `spec/api.md` §2.2, the transform id (`docs/wp/WP-089.md` "Tests"):
//! equal for equal inputs; different when one byte of an included file
//! changes, when an option changes, when a header's digest changes, and
//! when the target changes. Decisions: `spec/landed/api/WP-089.md`.

mod common;

use common::{file, loading, resource, scratch, with_meta, words};
use slip_kiln::session::{ReadFile, TransformCommand};
use slip_model::Digest;
use slip_model::hash::content_digest;
use slip_script::driver::{TRANSFORM_ID_TAG, host_version, transform_id};
use slip_script::resource::{API_LEVEL, meta_h_digest};

/// The id of `file` loaded with `author`, with its files, triple and
/// predefines.
fn loaded(file: &std::path::Path, author: &[&str]) -> (Digest, Vec<ReadFile>, String, Vec<u8>) {
    loading(&resource(), &words(author), file, |l| {
        let l = l.unwrap_or_else(|e| panic!("{e}"));
        (l.id(), l.files().to_vec(), l.triple().to_owned(), l.predefines().to_vec())
    })
}

/// A host's versions, for the formula's tests.
const H: &str = "slip 0.0.0 (kiln 0.1.0)";

// §2.2: the id is the formula's, over the host's versions (`slip
// --version`'s), the command line given to Kiln, the target and the files
// read; equal inputs give equal ids.
#[test]
fn s2_2_the_id_is_the_formulas_and_equal_for_equal_inputs() {
    let f = file("same", &with_meta("consteval void transform() {}"));
    let (a, files, triple, predefines) = loaded(&f, &["-DX=1"]);
    let (b, ..) = loaded(&f, &["-DX=1"]);
    assert_eq!(a, b);
    let line = TransformCommand::new(resource().join("include"), &words(&["-DX=1"]), f.clone())
        .unwrap()
        .command_line();
    assert_eq!(a, transform_id(API_LEVEL, &host_version(), &line, &triple, &predefines, &files));
    assert_eq!(
        host_version(),
        format!("slip {} (kiln {})", env!("CARGO_PKG_VERSION"), slip_kiln::kiln_version())
    );
    assert_eq!(triple, format!("{}-unknown-linux-gnu", std::env::consts::ARCH));
    assert!(String::from_utf8_lossy(&predefines).contains("#define X 1\n"));
}

// §2.2: one byte of an included file.
#[test]
fn s2_2_one_byte_of_an_included_file() {
    let dir = scratch("inc");
    let part = dir.join("part.h");
    std::fs::write(&part, "constexpr int part = 1;\n").unwrap();
    let f = file(
        "inc",
        &with_meta(&format!("#include \"{}\"\nconsteval void transform() {{}}", part.display())),
    );
    let (a, ..) = loaded(&f, &[]);
    std::fs::write(&part, "constexpr int part = 2;\n").unwrap();
    let (b, ..) = loaded(&f, &[]);
    assert_ne!(a, b);
}

// §2.2: an option.
#[test]
fn s2_2_an_option() {
    let f = file("opt", &with_meta("consteval void transform() {}"));
    let (a, ..) = loaded(&f, &["-DX=1"]);
    let (b, ..) = loaded(&f, &["-DX=2"]);
    let (c, ..) = loaded(&f, &[]);
    assert_ne!(a, b);
    assert_ne!(a, c);
}

// §2.2: a header's digest. A unit that read another `<slip/meta.h>` does
// not load (`transform-header`), so this is the formula over the files of
// a unit that loaded, with that one digest changed.
#[test]
fn s2_2_a_headers_digest() {
    let f = file("hdr", &with_meta("consteval void transform() {}"));
    let (a, mut files, triple, predefines) = loaded(&f, &[]);
    let line =
        TransformCommand::new(resource().join("include"), &[], f.clone()).unwrap().command_line();
    let meta = files.iter_mut().find(|r| r.digest == meta_h_digest()).expect("meta.h was read");
    meta.digest = content_digest(b"another meta.h");
    assert_ne!(a, transform_id(API_LEVEL, &host_version(), &line, &triple, &predefines, &files));
}

// §2.1 rule 3, §2.2 the target term: only the machine's default target can
// be made at this pin (the author may not give `-target`), so the term's
// bytes are tested: another triple, or other predefines, give another id.
#[test]
fn s2_2_the_target() {
    let files = [ReadFile { path: b"/t.cpp".to_vec(), digest: content_digest(b"t") }];
    let line = words(&["-std=c++26", "/t.cpp"]);
    let x86 = transform_id(
        API_LEVEL,
        H,
        &line,
        "x86_64-unknown-linux-gnu",
        b"#define __x86_64__ 1\n",
        &files,
    );
    let arm_triple = transform_id(
        API_LEVEL,
        H,
        &line,
        "aarch64-unknown-linux-gnu",
        b"#define __x86_64__ 1\n",
        &files,
    );
    let arm_predefines = transform_id(
        API_LEVEL,
        H,
        &line,
        "x86_64-unknown-linux-gnu",
        b"#define __aarch64__ 1\n",
        &files,
    );
    assert_ne!(x86, arm_triple);
    assert_ne!(x86, arm_predefines);
    assert_ne!(arm_triple, arm_predefines);
    // L6 of the review: the host's versions are a term.
    let other_host = transform_id(
        API_LEVEL,
        "slip 0.0.1 (kiln 0.1.0)",
        &line,
        "x86_64-unknown-linux-gnu",
        b"#define __x86_64__ 1\n",
        &files,
    );
    assert_ne!(x86, other_host);
    assert_ne!(
        x86,
        transform_id(
            API_LEVEL + 1,
            H,
            &line,
            "x86_64-unknown-linux-gnu",
            b"#define __x86_64__ 1\n",
            &files
        )
    );
}

// §2.2, the encoding: the record of the landed file under the tag
// `slip.transform.v0`, pinned by value, so that no change of the encoding
// goes unseen (a change is a new tag).
#[test]
fn s2_2_the_encoding_is_pinned() {
    assert_eq!(TRANSFORM_ID_TAG, "slip.transform.v0");
    let files = [ReadFile { path: b"/t.cpp".to_vec(), digest: content_digest(b"t") }];
    let id = transform_id(
        202_611,
        "slip 0.0.0 (kiln 0.1.0)",
        &words(&["-std=c++26", "/t.cpp"]),
        "x86_64-unknown-linux-gnu",
        b"P",
        &files,
    );
    let mut e = slip_model::wire::Encoder::new();
    e.uv(202_611)
        .str("slip 0.0.0 (kiln 0.1.0)")
        .count(2)
        .str("-std=c++26")
        .str("/t.cpp")
        .str("x86_64-unknown-linux-gnu");
    e.digest(&content_digest(b"P")).count(1).bytes(b"/t.cpp").digest(&content_digest(b"t"));
    assert_eq!(id, slip_model::hash::tagged_digest("slip.transform.v0", e.as_bytes()));
    assert_eq!(id.to_string(), PINNED);
}

const PINNED: &str = "744e71c0b72f57e6e4fda365d19fa72faf7e7523c9e7a4af7c4020dfa45e736f";

// §2.2 and `spec/edits.md` §2.1 (WP-147, S1): a file the unit read is
// named by its spelling on disk, whatever case the `#include` wrote it in
// (on a volume that finds it so), and the id is the formula's over that
// name. Every spelling of an include is in the bytes or the command line
// the id hashes, so this is what makes the id one per file read.
#[test]
fn s2_2_a_file_read_is_named_by_its_spelling_on_disk() {
    let dir = scratch("spelled");
    std::fs::create_dir_all(dir.join("inc")).unwrap();
    let part = dir.join("inc/part.h");
    std::fs::write(&part, "constexpr int part = 1;\n").unwrap();
    let asked = dir.join("INC/Part.h");
    if !asked.exists() {
        eprintln!("skipped: {} is on a volume where `INC/Part.h` names no file", dir.display());
        return;
    }
    let f = file(
        "spelled",
        &with_meta(&format!("#include \"{}\"\nconsteval void transform() {{}}", asked.display())),
    );
    let (id, files, triple, predefines) = loaded(&f, &[]);
    let names: Vec<String> =
        files.iter().map(|f| String::from_utf8_lossy(&f.path).into_owned()).collect();
    assert!(names.contains(&part.display().to_string()), "{names:?}");
    assert!(!names.contains(&asked.display().to_string()), "{names:?}");
    let line =
        TransformCommand::new(resource().join("include"), &[], f.clone()).unwrap().command_line();
    assert_eq!(id, transform_id(API_LEVEL, &host_version(), &line, &triple, &predefines, &files));
}
