//! WP-147 item 1 (`spec/landed/edits/WP-147.md`, row D1): the read-only
//! files the verify job gives the digest walk under a rule table
//! (`spec/verify.md` §2.1 "Rules rename owned entities only",
//! `ReadOnlyFiles::new` over `Input::subject_path`, as
//! `slip_cli::verify_job::post_walk` builds it), for a header that the
//! request makes read-only, included under another case.
//!
//! The tree is a `slip_kiln::tree::Tree` under `CARGO_TARGET_TMPDIR`, on
//! the workspace's volume, which finds a name in any case; the test skips,
//! saying so, on one that does not. The subject paths are the real paths
//! (the image of the tree itself, `Image::identity`).

use std::path::Path;

use slip_digest::ReadOnlyFiles;
use slip_kiln::ParseOptions;
use slip_kiln::tree::Tree;
use slip_model::transform::Ownership;

/// Whether the walk would take the header `src/vendor/x.h`, included as
/// `include` from `src/main.cpp`, for read-only, under a request owning
/// `src/` with `src/vendor` read-only: `None` when the volume does not
/// find the spelling.
fn header_read_only(include: &str) -> Option<Vec<(String, bool)>> {
    let main = format!("#include \"{include}\"\nint m = vendor_v;\n");
    let files: [(&str, &[u8]); 2] =
        [("src/vendor/x.h", b"int vendor_v;\n"), ("src/main.cpp", main.as_bytes())];
    let t = Tree::new(Path::new(env!("CARGO_TARGET_TMPDIR")), "one-spelling-digest", &files)
        .expect("the tree");
    if std::fs::symlink_metadata(t.root().join("src").join(include)).is_err() {
        eprintln!("skipped: {} is on a volume where `{include}` names no file", t.root().display());
        return None;
    }
    let root = t.root().to_str().unwrap();
    let own = Ownership {
        owned: vec![format!("{root}/src").into_bytes()],
        readonly: vec![format!("{root}/src/vendor").into_bytes()],
    };
    let obs = t.observer("src/main.cpp", "c++17", &[]);
    let (_, got) = t
        .parse(&obs, &ParseOptions::default(), |u| {
            slip_digest::map::with_input(u, &|p: &[u8]| p.to_vec(), |input| {
                let ro = ReadOnlyFiles::new(Some(&own), input.sm, input.subject_path);
                input
                    .sm
                    .file_ids()
                    .filter(|&f| input.sm.file_name(f).ends_with("x.h"))
                    .map(|f| (input.sm.file_name(f).to_owned(), ro.read_only(f)))
                    .collect::<Vec<_>>()
            })
            .expect("with_input")
        })
        .expect("the parse");
    Some(got.expect("a parse happened"))
}

/// `spec/verify.md` §2.1 (the control): included by its own spelling, the
/// header is read-only, and no rule matches its entities.
#[test]
fn s2_1_readonly_header_by_its_spelling_is_read_only() {
    let Some(got) = header_read_only("vendor/x.h") else { return };
    assert_eq!(got.len(), 1, "{got:?}");
    assert!(got[0].1, "{got:?}");
}

/// `spec/verify.md` §2.1: included as `Vendor/x.h`, it is read-only all
/// the same. Before WP-147 it was not (`read_only` false): its subject path is
/// `<root>/src/Vendor/x.h`, which `src/vendor` does not cover, so a rule
/// of the run matches a dependency's entity.
#[test]
fn s2_1_readonly_header_through_another_case_is_read_only() {
    let Some(got) = header_read_only("Vendor/x.h") else { return };
    assert_eq!(got.len(), 1, "{got:?}");
    assert!(got[0].1, "read-only under any spelling: {got:?}");
}
