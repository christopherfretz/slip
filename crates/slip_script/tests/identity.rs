//! `spec/api.md` §2.2, the transform id (`docs/wp/WP-089.md` "Tests"):
//! equal for equal inputs; different when one byte of an included file
//! changes, when an option changes, when a header's digest changes, and
//! when the target changes. Decisions: `spec/landed/api/WP-089.md`.

mod common;

use common::{file, loading, resource, scratch, with_meta, words};
use slip_kiln::session::{ReadFile, TransformCommand};
use slip_model::Digest;
use slip_model::hash::content_digest;
use slip_script::driver::{TRANSFORM_ID_TAG, transform_id};
use slip_script::resource::{API_LEVEL, meta_h_digest};

/// The id of `file` loaded with `author`, with its files, triple and
/// predefines.
fn loaded(file: &std::path::Path, author: &[&str]) -> (Digest, Vec<ReadFile>, String, Vec<u8>) {
    loading(&resource(), &words(author), file, |l| {
        let l = l.unwrap_or_else(|e| panic!("{e}"));
        (l.id(), l.files().to_vec(), l.triple().to_owned(), l.predefines().to_vec())
    })
}

// §2.2: the id is the formula's, over the command line given to Kiln, the
// target and the files read; equal inputs give equal ids.
#[test]
fn s2_2_the_id_is_the_formulas_and_equal_for_equal_inputs() {
    let f = file("same", &with_meta("consteval void transform() {}"));
    let (a, files, triple, predefines) = loaded(&f, &["-DX=1"]);
    let (b, ..) = loaded(&f, &["-DX=1"]);
    assert_eq!(a, b);
    let line = TransformCommand::new(resource().join("include"), &words(&["-DX=1"]), f.clone())
        .unwrap()
        .command_line();
    assert_eq!(a, transform_id(API_LEVEL, &line, &triple, &predefines, &files));
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
    assert_ne!(a, transform_id(API_LEVEL, &line, &triple, &predefines, &files));
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
        &line,
        "x86_64-unknown-linux-gnu",
        b"#define __x86_64__ 1\n",
        &files,
    );
    let arm_triple = transform_id(
        API_LEVEL,
        &line,
        "aarch64-unknown-linux-gnu",
        b"#define __x86_64__ 1\n",
        &files,
    );
    let arm_predefines = transform_id(
        API_LEVEL,
        &line,
        "x86_64-unknown-linux-gnu",
        b"#define __aarch64__ 1\n",
        &files,
    );
    assert_ne!(x86, arm_triple);
    assert_ne!(x86, arm_predefines);
    assert_ne!(arm_triple, arm_predefines);
    assert_ne!(
        x86,
        transform_id(
            API_LEVEL + 1,
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
        202611,
        &words(&["-std=c++26", "/t.cpp"]),
        "x86_64-unknown-linux-gnu",
        b"P",
        &files,
    );
    let mut e = slip_model::wire::Encoder::new();
    e.uv(202611).count(2).str("-std=c++26").str("/t.cpp").str("x86_64-unknown-linux-gnu");
    e.digest(&content_digest(b"P")).count(1).bytes(b"/t.cpp").digest(&content_digest(b"t"));
    assert_eq!(id, slip_model::hash::tagged_digest("slip.transform.v0", e.as_bytes()));
    assert_eq!(id.to_string(), PINNED);
}

const PINNED: &str = "320181760d195ecec5338297a6f1f937beb4acbfd154293e54eb7eb3dfab70f3";
