//! WP-132, end to end: an identifier piece with a key is checked as a
//! spelling (`spec/api.md` §7.2 rule 6). A transform renames `n::f` to
//! `h` with `identifier("h")` at its declaration and at its reference,
//! declares the rename (`expect_renamed`) and declares the unit of the
//! reference changed (`expect_changed`), so that no digest difference of
//! that unit is reported. In the post-image the reference `h(1)`, inside
//! the class `n::S`, denotes the member `S::h` and not the renamed
//! function: the host gave the piece `n::f`'s key, apply derives the
//! obligation of `spec/verify.md` §10.1 from it at the bytes `h` of the
//! post-image, the check of §10.2 fails it there with the key of `S::h`,
//! and the fold of §10.3 makes it `denotes_other`: a located obligation
//! finding, in a unit declared changed.
//!
//! And the right rename (`RIGHT`, no `S::h`): both references, the
//! unqualified one in `S::k` and the qualified `n::f(2)`, are keyed, apply
//! derives two obligations, and both hold under the run's rules and fail
//! without them (the key is `n::f`'s, the name in the post-image `n::h`).
//!
//! The transform runs in the host over a parsed subject; the edit set is
//! the run's edits as reduce would accept them, with nothing else in it;
//! apply's `build` is the product's; the check and the fold are the
//! trusted base's. What joins the steps here (the shard and the verify
//! job) is not under test.

#![forbid(unsafe_code)]

mod common;

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use slip_apply::{Files, build};
use slip_digest::spelling::{Obligation, Spelling, check_spellings};
use slip_digest::{Input, ReadOnly};
use slip_kiln::session::Cancel;
use slip_kiln::snippet::{Extra, Parsed, Snippet, parse_with_tables};
use slip_model::edits::{Piece, tiers};
use slip_model::editset::{Edit, EditSet, SCHEME, SetState};
use slip_model::hash::content_digest;
use slip_model::keys::RuleTable;
use slip_model::spellings::{Failed, Spellings};
use slip_model::{Digest, FileRef, formats};
use slip_script::driver::{self, Budgets, Outcome};
use slip_script::kiln_subject::KilnSubject;
use slip_script::output::{self, RunOutput};
use slip_verify::spelling::{Fold, Outcome as Checked};

const TRANSFORM: &str = r#"#include <slip/meta.h>

using namespace slip;

consteval void transform() {
  info f = lookup("n::f")[0];
  expect_renamed(f, "h");
  for (info d : declarations_of(f)) replace(name_range_of(d), identifier("h"));
  for (info r : references_to(f)) {
    expect_changed(r, "the call is checked by its spelling");
    replace(name_range_of(r), identifier("h"));
  }
}
"#;

/// The wrong rename: `h(1)` in `S` finds the member `S::h`.
const PRE: &str = "namespace n {
int f(int);
struct S {
  int h(int);
  int k() { return f(1); }
};
}
";

/// The right rename: nothing named `h` captures a reference.
const RIGHT: &str = "namespace n {
int f(int);
struct S {
  int k() { return f(1); }
};
}
int u() { return n::f(2); }
";

const MAIN: &str = "main.cpp";

fn resource() -> PathBuf {
    std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../resource")).unwrap()
}

fn transform_file() -> PathBuf {
    let dir = common::work_root().join(format!("wp132-identifier-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = std::fs::canonicalize(&dir).unwrap().join("transform.cpp");
    std::fs::write(&path, TRANSFORM).unwrap();
    path
}

/// The transform run in the host over `pre`.
fn run(pre: &'static str) -> RunOutput {
    let file = transform_file();
    let resource = resource();
    std::thread::scope(|s| {
        std::thread::Builder::new()
            .stack_size(driver::STACK + (1 << 20))
            .spawn_scoped(s, || {
                driver::with_transform(&resource, &[], &file, driver::STACK, |loaded| {
                    let mut l = loaded.expect("the transform loads");
                    let files = [(MAIN, pre)];
                    let snippet =
                        Snippet { files: &files, main: MAIN, include: &[""], std: "c++20" };
                    parse_with_tables(&snippet, &Extra::default(), |p: &Parsed<'_>| {
                        assert_eq!(p.errors, 0, "{}", p.diagnostics);
                        let owned = |_: &[u8]| true;
                        let subject = KilnSubject::from_snippet(p, &owned).expect("a subject");
                        let keyed = subject.with_keys(|subject| {
                            l.run(subject, Budgets::default(), &Cancel::new())
                        });
                        match keyed.value.expect("an outcome") {
                            (Outcome::Completed(out), _) => out,
                            (Outcome::Failed(f), _) => panic!("the run failed: {f:?}"),
                        }
                    })
                    .expect("the subject parses")
                })
            })
            .unwrap()
            .join()
            .unwrap()
    })
}

struct Mem(BTreeMap<Vec<u8>, Vec<u8>>);

impl Files for Mem {
    fn read(&self, path: &[u8]) -> io::Result<Vec<u8>> {
        self.0.get(path).cloned().ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
    }

    fn identity(&self, _: &[u8]) -> io::Result<Option<(u64, u64)>> {
        Ok(None)
    }
}

fn piece(p: &output::Piece) -> Piece {
    match p {
        output::Piece::Literal(b) => Piece::Literal(b.clone()),
        output::Piece::Spelled { key, text } => Piece::Spelled { key: *key, text: text.clone() },
        output::Piece::Identifier { text, key } => {
            Piece::Identifier { text: text.clone(), key: *key }
        }
        output::Piece::Copy(_) => panic!("the transform copies nothing"),
    }
}

/// The run's edits as an accepted edit set over the one file.
fn edit_set(out: &RunOutput, path: &[u8], pre: &str) -> EditSet {
    let set = EditSet {
        scheme: SCHEME,
        plan: Digest([1; 32]),
        round: 1,
        transform: out.transform,
        basis: Digest([3; 32]),
        state: SetState::Accepted,
        files: vec![FileRef {
            path: path.to_vec(),
            size: pre.len() as u64,
            content: content_digest(pre.as_bytes()),
        }],
        subjects: vec![],
        rules: vec![],
        edits: out
            .edits
            .iter()
            .map(|e| Edit {
                file: 0,
                lo: u64::from(e.place.lo),
                hi: u64::from(e.place.hi),
                pieces: e.pieces.iter().map(piece).collect(),
                subjects: vec![],
                tiers: tiers::T0,
                macro_site: None,
                notes: vec![],
                observers: 1,
                votes: 1,
                first: vec![],
            })
            .collect(),
        groups: vec![],
        changed: vec![],
        kept: vec![],
        unresolved: vec![],
        refusals: vec![],
        reports: vec![],
        failures: vec![],
        unparsed: vec![],
        counters: vec![],
        instances: Vec::new(),
    };
    set.validate().unwrap_or_else(|e| panic!("the edit set is invalid: {e}"));
    set
}

/// The check of §10.2 in the one observer of `post`, under `rules`.
fn check(post: &str, rules: &RuleTable, obligations: &[Obligation]) -> Vec<Spelling> {
    let files = [(MAIN, post)];
    let snippet = Snippet { files: &files, main: MAIN, include: &[""], std: "c++20" };
    parse_with_tables(&snippet, &Extra::default(), |p: &Parsed<'_>| {
        assert_eq!(p.errors, 0, "{}", p.diagnostics);
        let paths: Vec<Vec<u8>> =
            p.sm.file_ids()
                .map(|f| p.path(f).map(|s: &str| s.as_bytes().to_vec()).unwrap_or_default())
                .collect();
        let input = Input {
            ast: p.ast,
            sm: p.sm,
            subject_path: &|f| paths.get(f.index()).cloned().unwrap_or_default(),
        };
        check_spellings(p.unit(), &input, rules, ReadOnly::NO_FILE, obligations)
    })
    .expect("the post-image parses")
}

#[test]
fn wp132_a_keyed_identifier_piece_that_denotes_another_entity_fails_located() {
    let out = run(PRE);
    // The host: no key at the declaration's name, `n::f`'s key at the
    // reference; the rename and the unit of the reference declared.
    let at = |lo: usize| PRE.match_indices('f').map(|(i, _)| i).nth(lo).unwrap() as u32;
    let (decl, call) = (at(0), at(1));
    let pieces: Vec<(u32, &[output::Piece])> =
        out.edits.iter().map(|e| (e.place.lo, e.pieces.as_slice())).collect();
    assert_eq!(pieces.len(), 2, "{pieces:?}");
    assert_eq!(pieces[0].0, decl);
    assert_eq!(pieces[0].1, [output::Piece::Identifier { text: b"h".to_vec(), key: None }]);
    assert_eq!(pieces[1].0, call);
    let [output::Piece::Identifier { text, key: Some(f_key) }] = pieces[1].1 else {
        panic!("the reference's piece has no key: {:?}", pieces[1].1);
    };
    assert_eq!(text, b"h");
    assert_eq!(out.renamed.len(), 1, "the rule of the rename");
    assert_eq!(out.changed.len(), 1, "the unit of the reference is declared changed");
    assert!(out.edits.iter().all(|e| e.place.file.path == MAIN.as_bytes()));
    // The edit set names files by canonical absolute paths.
    let path = b"/src/main.cpp".to_vec();
    // Apply: the obligation, from the keyed identifier piece only, at the
    // bytes `h` of the call in the post-image.
    let set = edit_set(&out, &path, PRE);
    let id = formats::content_id(&set.to_bytes()).unwrap();
    let mem = Mem(BTreeMap::from([(path.clone(), PRE.as_bytes().to_vec())]));
    let built = build(&set, id, &mem).expect("apply builds");
    let post = String::from_utf8(built.blobs[0].1.clone()).unwrap();
    assert_eq!(post, PRE.replace("f(", "h("));
    assert_eq!(built.overlay.spellings.len(), 1, "{:?}", built.overlay.spellings);
    let ob = built.overlay.spellings[0];
    assert_eq!((ob.file, ob.lo, ob.hi, ob.key), (0, u64::from(call), u64::from(call) + 1, *f_key));
    assert_eq!(&post[call as usize..=call as usize], "h");
    // The check, in the post-image under the run's rules: the name there
    // denotes `S::h`.
    let rules = RuleTable::new(out.renamed.clone()).expect("a rule table");
    let got = check(
        &post,
        &rules,
        &[Obligation { file: MAIN.as_bytes().to_vec(), lo: call, hi: call + 1, key: ob.key }],
    );
    let [Spelling::Fails { found: Some(found), found_name }] = got.as_slice() else {
        panic!("the obligation does not fail: {got:?}");
    };
    assert_ne!(found, f_key);
    assert!(found_name.contains('h'), "{found_name}");
    // The fold: `denotes_other` for obligation 0, in the overlay's file at
    // the call's bytes.
    let o = &built.overlay;
    let inputs = [FileRef {
        path: o.files[0].path.clone(),
        size: o.files[0].post.size,
        content: o.files[0].post.content,
    }];
    let section = Spellings {
        seen: 1,
        held: vec![],
        failed: vec![Failed { index: 0, found: Some(*found), name: found_name.clone() }],
    };
    let mut fold = Fold::new(o);
    fold.add(0, &inputs, &section).expect("the section is placed");
    assert_eq!(
        fold.finish(),
        [Checked::DenotesOther {
            observers: 1,
            first: 0,
            found: Some(*found),
            name: found_name.clone()
        }]
    );
}

#[test]
fn wp132_a_right_rename_with_identifier_pieces_holds() {
    let out = run(RIGHT);
    let keys: Vec<Option<Digest>> = out
        .edits
        .iter()
        .map(|e| match e.pieces.as_slice() {
            [output::Piece::Identifier { text, key }] if text == b"h" => *key,
            other => panic!("not one identifier piece `h`: {other:?}"),
        })
        .collect();
    // The declaration's name, then the two references, both keyed alike.
    assert_eq!(keys.len(), 3, "{:?}", out.edits);
    assert_eq!(keys[0], None);
    assert!(keys[1].is_some() && keys[1] == keys[2], "{keys:?}");
    let path = b"/src/main.cpp".to_vec();
    let set = edit_set(&out, &path, RIGHT);
    let id = formats::content_id(&set.to_bytes()).unwrap();
    let mem = Mem(BTreeMap::from([(path.clone(), RIGHT.as_bytes().to_vec())]));
    let built = build(&set, id, &mem).expect("apply builds");
    let post = String::from_utf8(built.blobs[0].1.clone()).unwrap();
    assert_eq!(post, RIGHT.replace("f(", "h("));
    let obligations: Vec<Obligation> = built
        .overlay
        .spellings
        .iter()
        .map(|ob| Obligation {
            file: MAIN.as_bytes().to_vec(),
            lo: u32::try_from(ob.lo).unwrap(),
            hi: u32::try_from(ob.hi).unwrap(),
            key: ob.key,
        })
        .collect();
    assert_eq!(obligations.len(), 2, "{:?}", built.overlay.spellings);
    assert!(obligations.iter().all(|o| &post[o.lo as usize..o.hi as usize] == "h"));
    let rules = RuleTable::new(out.renamed.clone()).expect("a rule table");
    assert_eq!(check(&post, &rules, &obligations), [Spelling::Holds, Spelling::Holds]);
    // Without the rule the key is `n::f`'s and the name denotes `n::h`.
    let none = RuleTable::new(vec![]).expect("no rules");
    let got = check(&post, &none, &obligations);
    assert!(got.iter().all(|s| !matches!(s, Spelling::Holds)), "{got:?}");
}
