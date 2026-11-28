//! Declared changes and their container, kind `DECL` (`spec/verify.md` §2;
//! `spec/model.md` §2, §5, §10; the encodings, offsets and paths of
//! `spec/landed/model/WP-022.md`). Work package WP-022; version 2 (`kept`,
//! `files`, the reader's checks of `spec/verify.md` §2) is WP-110's. Each
//! test names the section it covers.

use std::fs;
use std::path::Path;

use slip_model::Digest;
use slip_model::container::{HEADER_LEN, TRAILER_LEN, Writer};
use slip_model::declared::{
    Declared, DeclaredError, ExistingLocal, RULES_V2_TAG, RULES_V3_TAG, RULES_V4_TAG,
};
use slip_model::formats::{JsonFormError, content_id, dump, undump};
use slip_model::hash::tagged_digest;
use slip_model::json::{self, Value};
use slip_model::keys::{KeyKind, Name, Parent, RULES_TAG, Rule, RuleError, RuleTable};
use slip_model::transform::Ownership;
use slip_model::wire::{Encoder, ErrorKind};

// ---------------------------------------------------------------------------
// Helpers

fn d(n: u8) -> Digest {
    Digest([n; 32])
}

fn id(s: &str) -> Name {
    Name::Identifier(s.as_bytes().to_vec())
}

fn rule(
    parent: Parent,
    kind: KeyKind,
    name: Name,
    disc: Option<Digest>,
    to: (Parent, Name),
) -> Rule {
    Rule { parent, kind, name, disc, to_parent: to.0, to_name: to.1 }
}

fn sorted(rules: Vec<Rule>) -> Vec<Rule> {
    RuleTable::new(rules).unwrap().rules().to_vec()
}

/// The golden's rules: every Parent form and every Name form, in `name`
/// or `to_name`.
fn golden_rules() -> Vec<Rule> {
    let acme = Parent::Unit(d(0xac));
    let legacy = Parent::Unit(d(0x1e));
    sorted(vec![
        // namespace acme → legacy, at the root (the motivating relocation).
        rule(Parent::Root, KeyKind::Namespace, id("legacy"), None, (Parent::Root, id("acme"))),
        rule(legacy, KeyKind::Function, id("g"), Some(d(0xd1)), (acme, id("f"))),
        rule(acme, KeyKind::Function, Name::Operator(5), Some(d(0xd2)), (acme, Name::Constructor)),
        rule(
            acme,
            KeyKind::Function,
            Name::Destructor,
            Some(d(0xd3)),
            (acme, Name::Conversion(d(0x7e))),
        ),
        rule(
            Parent::CLinkage,
            KeyKind::Function,
            Name::LiteralOperator(b"_km".to_vec()),
            None,
            (Parent::Builtin, Name::DeductionGuide),
        ),
        rule(acme, KeyKind::Namespace, Name::Anonymous, None, (acme, Name::Content)),
        rule(
            acme,
            KeyKind::Class,
            Name::Local(3),
            None,
            (acme, Name::UnnamedBy(Box::new(id("T")))),
        ),
        rule(acme, KeyKind::Class, Name::UnnamedOrdinal(2), None, (acme, Name::Property(7))),
        rule(acme, KeyKind::Order, Name::Order, None, (acme, Name::Parameter(1))),
        rule(acme, KeyKind::Variable, Name::Identifier(vec![0xff, 0x00]), None, (acme, id("ok"))),
    ])
}

/// Files for a value with rules: an owned tree with a read-only subtree.
fn files() -> Ownership {
    Ownership {
        owned: vec![b"/src".to_vec()],
        readonly: vec![b"/src/dep".to_vec()],
        rebuilt: Vec::new(),
    }
}

/// A value with rules is written with its files (verify §2).
fn with_files(x: Declared) -> Declared {
    if x.rules().is_empty() { x } else { x.with_files(Some(files())) }
}

fn golden_declared() -> Declared {
    Declared::new(golden_rules(), vec![d(1), d(2)], vec![d(2)], vec![d(3), d(4)])
        .unwrap()
        .with_kept(vec![d(5), d(6)])
        .unwrap()
        .with_files(Some(files()))
}

fn parse(text: &str) -> Value {
    json::parse(text.as_bytes()).unwrap()
}

/// The payload of the `declared` section, written by the test's own
/// encoder from the spec's layout.
fn raw_payload(rules: &[Rule], lists: [&[Digest]; 3]) -> Vec<u8> {
    let mut e = Encoder::new();
    e.count(rules.len());
    for r in rules {
        r.encode(&mut e);
    }
    for list in lists {
        e.count(list.len());
        for k in list {
            e.digest(k);
        }
    }
    e.into_bytes()
}

/// Where the version 2 fields start in a payload, payload offsets.
#[derive(Default)]
struct At2 {
    kept_at: Vec<usize>,
    files: usize,
    owned: usize,
    owned_at: Vec<usize>,
    readonly_at: Vec<usize>,
    /// Version 3 (WP-118): the `rebuilt` list and its entries.
    rebuilt: usize,
    rebuilt_at: Vec<usize>,
}

/// The payload of a version 2 `declared` section, by the test's encoder:
/// the version 1 fields, `kept`, then `files` as `opt` of two
/// `list<bytes>`.
fn raw_payload_v2(
    rules: &[Rule],
    lists: [&[Digest]; 3],
    kept: &[Digest],
    files: Option<&Ownership>,
) -> (Vec<u8>, At2) {
    let mut e = Encoder::new();
    e.count(rules.len());
    for r in rules {
        r.encode(&mut e);
    }
    for list in lists {
        e.count(list.len());
        for k in list {
            e.digest(k);
        }
    }
    let mut at = At2::default();
    e.count(kept.len());
    for k in kept {
        at.kept_at.push(e.as_bytes().len());
        e.digest(k);
    }
    at.files = e.as_bytes().len();
    match files {
        None => {
            e.u8(0);
        }
        Some(o) => {
            e.u8(1);
            at.owned = e.as_bytes().len();
            for (list, offs) in [(&o.owned, &mut at.owned_at), (&o.readonly, &mut at.readonly_at)] {
                e.count(list.len());
                for p in list {
                    offs.push(e.as_bytes().len());
                    e.bytes(p);
                }
            }
            // Version 3 (WP-118 R5.4): `rebuilt` after `readonly`, written
            // exactly when it is not empty.
            if !o.rebuilt.is_empty() {
                at.rebuilt = e.as_bytes().len();
                e.count(o.rebuilt.len());
                for p in &o.rebuilt {
                    at.rebuilt_at.push(e.as_bytes().len());
                    e.bytes(p);
                }
            }
        }
    }
    (e.into_bytes(), at)
}

/// The version a value is written in: 3 exactly when its files have a
/// rebuilt entry (WP-118 R5.4).
fn version_of(files: Option<&Ownership>) -> u32 {
    if files.is_some_and(|o| !o.rebuilt.is_empty()) { 3 } else { 2 }
}

/// A version 3 file.
fn decl3(payload: &[u8]) -> Vec<u8> {
    file(*b"DECL", 3, &[(1, payload)])
}

fn file(kind: [u8; 4], version: u32, sections: &[(u64, &[u8])]) -> Vec<u8> {
    let mut w = Writer::new(kind, version);
    for (id, p) in sections {
        w.section(*id, p);
    }
    w.finish()
}

/// A version 1 file.
fn decl(payload: &[u8]) -> Vec<u8> {
    file(*b"DECL", 1, &[(1, payload)])
}

/// A version 2 file.
fn decl2(payload: &[u8]) -> Vec<u8> {
    file(*b"DECL", 2, &[(1, payload)])
}

/// The file offset of the payload's first byte, for a payload this long.
fn base(payload: &[u8]) -> usize {
    let mut e = Encoder::new();
    e.count(payload.len());
    HEADER_LEN + 1 + e.as_bytes().len()
}

fn rule_len(r: &Rule) -> usize {
    let mut e = Encoder::new();
    r.encode(&mut e);
    e.as_bytes().len()
}

fn set(v: &mut Value, path: &[&str], new: Value) {
    let mut cur = v;
    for step in path {
        cur = match cur {
            Value::Object(m) => &mut m.iter_mut().find(|(k, _)| k == step).unwrap().1,
            Value::Array(a) => &mut a[step.parse::<usize>().unwrap()],
            _ => panic!("set: no {step}"),
        };
    }
    *cur = new;
}

fn get<'v>(v: &'v Value, path: &[&str]) -> &'v Value {
    let mut cur = v;
    for step in path {
        cur = match cur {
            Value::Object(m) => &m.iter().find(|(k, _)| k == step).unwrap().1,
            Value::Array(a) => &a[step.parse::<usize>().unwrap()],
            _ => panic!("get: no {step}"),
        };
    }
    cur
}

/// `bytes` is rejected with `Malformed` at `offset` and message `what`, and
/// its JSON rendering (built from the same lists by hand) at `path` with
/// the same message.
fn rejected(bytes: &[u8], offset: usize, json: &Value, path: &str) {
    let e = Declared::from_bytes(bytes).expect_err("from_bytes accepted it");
    assert_eq!((e.kind, e.offset), (ErrorKind::Malformed, offset), "{e}");
    assert_eq!(content_id(bytes).unwrap_err(), e);
    assert_eq!(dump(bytes).unwrap_err(), e);
    let j = Declared::from_json(json).expect_err("from_json accepted it");
    assert_eq!(j, JsonFormError { path: path.to_owned(), message: e.what.to_owned() });
    assert_eq!(undump(json).unwrap_err(), j);
}

/// The JSON rendering of the golden value with the `declared` section's
/// member `list` replaced by `value`.
fn json_with(list: &str, value: Value) -> Value {
    let mut v = golden_declared().to_json();
    set(&mut v, &["sections", "declared", list], value);
    v
}

fn json_keys(keys: &[Digest]) -> Value {
    Value::Array(keys.iter().map(|k| Value::Str(k.to_string())).collect())
}

// ---------------------------------------------------------------------------
// Round trips and goldens (verify §2, model §10)

fn golden(name: &str, actual: &[u8]) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden").join(name);
    if std::env::var_os("SLIP_BLESS").is_some() {
        fs::write(&path, actual).unwrap();
        eprintln!("blessed {}", path.display());
        return;
    }
    let expected = fs::read(&path)
        .unwrap_or_else(|e| panic!("{}: {e}; run with SLIP_BLESS=1 to create", path.display()));
    assert!(actual == expected, "{name} differs from its golden; SLIP_BLESS=1 rewrites it");
}

fn round_trip(x: &Declared) {
    x.validate().unwrap();
    let bytes = x.to_bytes();
    let (payload, _) = raw_payload_v2(
        x.rules().rules(),
        [x.changed(), x.created(), x.removed()],
        x.kept(),
        x.files(),
    );
    let version = version_of(x.files());
    assert_eq!(
        bytes,
        file(*b"DECL", version, &[(1, &payload)]),
        "to_bytes agrees with the test's encoder"
    );
    let back = Declared::from_bytes(&bytes).unwrap();
    assert_eq!(&back, x);
    assert_eq!(back.to_bytes(), bytes);
    let text = json::write_pretty(&x.to_json()).unwrap();
    let from_text = Declared::from_json(&parse(&text)).unwrap();
    assert_eq!(&from_text, x);
    assert_eq!(from_text.to_bytes(), bytes, "binary → JSON → binary");
    assert_eq!(dump(&bytes).unwrap(), text);
    assert_eq!(undump(&parse(&text)).unwrap(), bytes);
    assert_eq!(content_id(&bytes).unwrap().as_bytes()[..], bytes[bytes.len() - 32..]);
}

// verify §2, model §10: the binary golden and the JSON golden of declared
// changes with every Parent and Name form.
#[test]
fn golden_declared_binary_and_json() {
    let x = golden_declared();
    round_trip(&x);
    let bytes = x.to_bytes();
    golden("declared.bin", &bytes);
    let text = dump(&bytes).unwrap();
    golden("declared.json", text.as_bytes());
    assert_eq!(Declared::from_bytes(&bytes).unwrap(), x);
    assert_eq!(Declared::from_json(&parse(&text)).unwrap(), x);
    // The JSON forms the landed file names.
    let v = parse(&text);
    let rules = ["sections", "declared", "rules"];
    let r = |i: &str, field: &str| get(&v, &[rules[0], rules[1], rules[2], i, field]).clone();
    assert_eq!(r("0", "parent"), Value::Str("root".into()));
    assert_eq!(r("0", "kind"), Value::Str("namespace".into()));
    assert_eq!(r("0", "name"), parse(r#"{"identifier": "legacy"}"#));
    assert_eq!(r("0", "disc"), Value::Null);
    assert!(text.contains(r#""to_name": "constructor""#));
    assert!(text.contains(r#""unnamed_by": {"#));
    assert!(text.contains(r#""hex": "ff00""#));
    assert!(text.contains(r#""parent": "c_linkage""#));
    assert!(text.contains(r#""to_parent": "builtin""#));
    assert!(text.contains(r#""unit": "acacacac"#));
    assert_eq!(get(&v, &["version"]), &Value::Int(2));
    assert_eq!(
        get(&v, &["sections", "declared", "files"]),
        &parse(r#"{"owned": ["/src"], "readonly": ["/src/dep"]}"#)
    );
    assert_eq!(get(&v, &["sections", "declared", "kept"]), &json_keys(&[d(5), d(6)]));
}

// verify §2: every KeyKind round-trips in a rule, in both forms.
#[test]
fn every_key_kind_round_trips_in_a_container() {
    let rules = KeyKind::ALL
        .into_iter()
        .map(|kind| rule(Parent::Root, kind, id("x"), None, (Parent::Root, id("y"))))
        .collect();
    let x = with_files(Declared::new(sorted(rules), vec![], vec![], vec![]).unwrap());
    round_trip(&x);
    let text = dump(&x.to_bytes()).unwrap();
    for kind in KeyKind::ALL {
        assert!(text.contains(&format!("\"kind\": \"{}\"", kind.name())), "{kind:?}");
    }
}

// verify §2: `none()` is five empty lists and no files, and its
// `rules_id` is the id of the empty table.
#[test]
fn none_is_empty_lists_and_no_files() {
    let x = Declared::none();
    assert!(x.rules().is_empty());
    assert!(x.changed().is_empty() && x.created().is_empty() && x.removed().is_empty());
    assert!(x.kept().is_empty() && x.files().is_none());
    assert_eq!(x.rules().id(), RuleTable::empty().id());
    assert_eq!(x.rules().id(), tagged_digest(RULES_TAG, &[0]));
    assert_eq!(x.rules_id(), x.rules().id());
    assert_eq!(Declared::new(vec![], vec![], vec![], vec![]).unwrap(), x);
    round_trip(&x);
    assert_eq!(x.to_bytes(), decl2(&[0, 0, 0, 0, 0, 0]));
}

// verify §2: a version 1 file without a rule is read as one with `kept`
// empty and `files` absent, in both forms.
#[test]
fn version_1_without_a_rule_is_read() {
    let payload = raw_payload(&[], [&[d(1)], &[d(2)], &[d(3)]]);
    let x = Declared::from_bytes(&decl(&payload)).unwrap();
    let want = Declared::new(vec![], vec![d(1)], vec![d(2)], vec![d(3)]).unwrap();
    assert_eq!(x, want);
    assert!(x.kept().is_empty() && x.files().is_none());
    let text = dump(&decl(&payload)).unwrap();
    assert!(text.contains(r#""version": 1"#) && !text.contains("kept"), "{text}");
    assert_eq!(Declared::from_json(&parse(&text)).unwrap(), want);
    assert_eq!(undump(&parse(&text)).unwrap(), decl(&payload));
    // A version 1 JSON rendering with version 2's members is refused.
    let mut v = want.to_json();
    set(&mut v, &["version"], Value::Int(1));
    assert_eq!(Declared::from_json(&v).unwrap_err().path, "sections.declared.kept");
}

// verify §2 (the reader's checks, row 5): a version 1 file with a rule is
// refused, at the first rule.
#[test]
fn rejects_version_1_with_a_rule() {
    let (a, b) = two_rules();
    let payload = raw_payload(&[a.clone(), b.clone()], [&[], &[], &[]]);
    let mut json = json_with("rules", json_rules(&[a, b]));
    set(&mut json, &["version"], Value::Int(1));
    let Value::Object(m) = &mut json else { unreachable!() };
    let Value::Object(sections) = &mut m[2].1 else { unreachable!() };
    let Value::Object(fields) = &mut sections[0].1 else { unreachable!() };
    fields.truncate(4);
    rejected(&decl(&payload), base(&payload) + 1, &json, "sections.declared.rules[0]");
    assert_eq!(
        Declared::from_bytes(&decl(&payload)).unwrap_err().what,
        "a version 1 container has a rule"
    );
}

// verify §2 (row 6): rules without files, and files without a rule, are
// refused at `files`, in a file and by `validate`.
#[test]
fn rejects_files_absent_with_rules_and_present_without() {
    let (a, _) = two_rules();
    let (payload, at) = raw_payload_v2(std::slice::from_ref(&a), [&[], &[], &[]], &[], None);
    let mut json = json_with("rules", json_rules(std::slice::from_ref(&a)));
    set(&mut json, &["sections", "declared", "files"], Value::Null);
    rejected(&decl2(&payload), base(&payload) + at.files, &json, "sections.declared.files");
    assert_eq!(Declared::from_bytes(&decl2(&payload)).unwrap_err().what, "rules without files");
    let x = Declared::new(vec![a], vec![], vec![], vec![]).unwrap();
    assert!(x.validate().unwrap_err().contains("rules without files"));

    let o = files();
    let (payload, at) = raw_payload_v2(&[], [&[], &[], &[]], &[], Some(&o));
    let json = json_with("rules", Value::Array(vec![]));
    rejected(&decl2(&payload), base(&payload) + at.files, &json, "sections.declared.files");
    assert_eq!(Declared::from_bytes(&decl2(&payload)).unwrap_err().what, "files without a rule");
    assert!(Declared::none().with_files(Some(o)).validate().is_err());
}

// verify §2 (row 3): `kept` not strictly ascending, in a file, by
// `with_kept`; a key of `kept` may be in an exemption list too.
#[test]
fn rejects_kept_unsorted_or_duplicated() {
    let rules = golden_rules();
    let o = files();
    for bad in [[d(1), d(3), d(2)], [d(1), d(2), d(2)]] {
        let (payload, at) = raw_payload_v2(&rules, [&[], &[], &[]], &bad, Some(&o));
        let mut json = json_with("kept", json_keys(&bad));
        for list in ["changed", "created", "removed"] {
            set(&mut json, &["sections", "declared", list], Value::Array(vec![]));
        }
        let offset = base(&payload) + at.kept_at[2];
        rejected(&decl2(&payload), offset, &json, "sections.declared.kept[2]");
        assert_eq!(
            Declared::from_bytes(&decl2(&payload)).unwrap_err().what,
            "kept is not strictly ascending"
        );
        assert_eq!(
            Declared::none().with_kept(bad.to_vec()),
            Err(DeclaredError::NotAscending { list: "kept", index: 2 })
        );
    }
    let x = with_files(
        Declared::new(rules, vec![d(1)], vec![], vec![]).unwrap().with_kept(vec![d(1)]).unwrap(),
    );
    round_trip(&x);
}

/// One bad `files`: (owned, readonly, message, which list, index).
type BadOwnership<'a> = (Vec<Vec<u8>>, Vec<Vec<u8>>, &'a str, u8, usize);

// verify §2 (row 4): `files` is checked as a transform request's ownership
// (edits §2.1), in that order and with those messages.
#[test]
fn rejects_each_bad_ownership() {
    let p = |s: &str| s.as_bytes().to_vec();
    let rules = golden_rules();
    // (owned, readonly, message, which: 0 owned list, 1 owned[i], 2 readonly[i], index)
    let cases: [BadOwnership<'_>; 8] = [
        (vec![], vec![], "owned is empty", 0, 0),
        (vec![p("/a"), p("b")], vec![], "not a canonical absolute path", 1, 1),
        (vec![p("/a/")], vec![], "not a canonical absolute path", 1, 0),
        (vec![p("/b"), p("/a")], vec![], "owned is not strictly ascending", 1, 1),
        (vec![p("/a"), p("/a")], vec![], "owned is not strictly ascending", 1, 1),
        (vec![p("/a")], vec![p("/a/../b")], "not a canonical absolute path", 2, 0),
        (vec![p("/a")], vec![p("/c"), p("/b")], "readonly is not strictly ascending", 2, 1),
        (vec![p("/a"), p("/b")], vec![p("/b")], "a readonly entry is also in owned", 2, 0),
    ];
    for (owned, readonly, what, which, i) in cases {
        let o = Ownership { owned, readonly, rebuilt: Vec::new() };
        let (payload, at) = raw_payload_v2(&rules, [&[], &[], &[]], &[], Some(&o));
        let mut json = golden_declared().to_json();
        for list in ["changed", "created", "removed", "kept"] {
            set(&mut json, &["sections", "declared", list], Value::Array(vec![]));
        }
        let paths = |l: &[Vec<u8>]| {
            Value::Array(
                l.iter().map(|b| Value::Str(String::from_utf8(b.clone()).unwrap())).collect(),
            )
        };
        set(&mut json, &["sections", "declared", "files", "owned"], paths(&o.owned));
        set(&mut json, &["sections", "declared", "files", "readonly"], paths(&o.readonly));
        let (offset, path) = match which {
            0 => (at.owned, "sections.declared.files.owned".to_owned()),
            1 => (at.owned_at[i], format!("sections.declared.files.owned[{i}]")),
            _ => (at.readonly_at[i], format!("sections.declared.files.readonly[{i}]")),
        };
        rejected(&decl2(&payload), base(&payload) + offset, &json, &path);
        assert_eq!(Declared::from_bytes(&decl2(&payload)).unwrap_err().what, what);
        let x = Declared::new(rules.clone(), vec![], vec![], vec![]).unwrap().with_files(Some(o));
        assert!(x.validate().is_err(), "{what}");
    }
}

// verify §2: `rules_id` is `H("slip.rules.v1", rules)` without files and
// `H("slip.rules.v2", rules, files)` with them; files that differ give
// ids that differ.
#[test]
fn rules_id_covers_files() {
    let x = golden_declared();
    let mut e = Encoder::new();
    e.count(x.rules().rules().len());
    for r in x.rules().rules() {
        r.encode(&mut e);
    }
    e.count(1).bytes(b"/src").count(1).bytes(b"/src/dep");
    assert_eq!(x.rules_id(), tagged_digest(RULES_V2_TAG, e.as_bytes()));
    assert_ne!(x.rules_id(), x.rules().id());
    let other = x.clone().with_files(Some(Ownership {
        owned: vec![b"/src".to_vec()],
        readonly: vec![b"/src/dep2".to_vec()],
        rebuilt: Vec::new(),
    }));
    assert_ne!(other.rules_id(), x.rules_id());
    let moved = x.clone().with_files(Some(Ownership {
        owned: vec![b"/src".to_vec(), b"/src/dep".to_vec()],
        readonly: vec![],
        rebuilt: Vec::new(),
    }));
    assert_ne!(moved.rules_id(), x.rules_id());
    // Only the rules and the files: the exemption lists and `kept` are not
    // in it.
    let bare = Declared::new(x.rules().rules().to_vec(), vec![], vec![], vec![])
        .unwrap()
        .with_files(Some(files()));
    assert_eq!(bare.rules_id(), x.rules_id());
    assert_eq!(Declared::none().rules_id(), RuleTable::empty().id());
}

// verify §2: a key may be in more than one exemption list.
#[test]
fn a_key_in_several_lists() {
    let x = Declared::new(vec![], vec![d(1)], vec![d(1)], vec![d(1)]).unwrap();
    round_trip(&x);
}

// model §5 rule 2, §5.4: an unknown ignorable section is skipped by the
// typed readers and kept by dump and undump.
#[test]
fn ignorable_section_kept_by_dump() {
    let payload = raw_payload(&[], [&[d(1)], &[], &[]]);
    let bytes = file(*b"DECL", 1, &[(1, &payload), (64, b"zz")]);
    let x = Declared::from_bytes(&bytes).unwrap();
    assert_eq!(x.changed(), [d(1)]);
    let text = dump(&bytes).unwrap();
    assert!(text.contains("\"#64\""));
    assert_eq!(undump(&parse(&text)).unwrap(), bytes);
    assert_eq!(Declared::from_json(&parse(&text)).unwrap(), x);
}

// ---------------------------------------------------------------------------
// Declared::new (verify §2)

#[test]
fn new_rejects_each_list_unsorted_or_duplicated() {
    for (k, list) in ["changed", "created", "removed"].into_iter().enumerate() {
        for bad in [vec![d(1), d(3), d(2)], vec![d(1), d(2), d(2)]] {
            let mut lists = [vec![], vec![], vec![]];
            lists[k] = bad;
            let [a, b, c] = lists;
            assert_eq!(
                Declared::new(vec![], a, b, c),
                Err(DeclaredError::NotAscending { list, index: 2 })
            );
        }
    }
    let rules = golden_rules();
    let mut swapped = rules.clone();
    swapped.swap(3, 4);
    assert_eq!(
        Declared::new(swapped, vec![], vec![], vec![]),
        Err(DeclaredError::NotAscending { list: "rules", index: 4 })
    );
    let mut twice = rules.clone();
    twice.insert(4, Rule { to_name: id("other"), ..rules[3].clone() });
    assert_eq!(
        Declared::new(twice, vec![], vec![], vec![]),
        Err(DeclaredError::Rules(RuleError::Duplicate { first: 3, second: 4 }))
    );
    let mut invalid = rules;
    invalid[2].to_name = Name::Operator(0);
    assert_eq!(
        Declared::new(invalid, vec![], vec![], vec![]),
        Err(DeclaredError::Rules(RuleError::InvalidName {
            index: 2,
            what: "operator number is 0"
        }))
    );
}

// ---------------------------------------------------------------------------
// Rejected files (landed model WP-022; model §2.1, §5.3, §5.4)

fn two_rules() -> (Rule, Rule) {
    let a = rule(Parent::Root, KeyKind::Namespace, id("a"), None, (Parent::Root, id("p")));
    let b = rule(Parent::Root, KeyKind::Namespace, id("b"), None, (Parent::Root, id("q")));
    (a, b)
}

fn json_rules(rules: &[Rule]) -> Value {
    // The rendering of each rule, taken from a valid value holding it alone.
    Value::Array(
        rules
            .iter()
            .map(|r| {
                let x = Declared::new(vec![r.clone()], vec![], vec![], vec![]).unwrap();
                get(&x.to_json(), &["sections", "declared", "rules", "0"]).clone()
            })
            .collect(),
    )
}

// Rules not strictly ascending: Malformed at the later rule's first byte.
#[test]
fn rejects_rules_unsorted() {
    let (a, b) = two_rules();
    let payload = raw_payload(&[b.clone(), a.clone()], [&[], &[], &[]]);
    let offset = base(&payload) + 1 + rule_len(&b);
    let json = json_with("rules", json_rules(&[b, a]));
    rejected(&decl(&payload), offset, &json, "sections.declared.rules[1]");
    assert_eq!(
        Declared::from_bytes(&decl(&payload)).unwrap_err().what,
        "rules are not strictly ascending"
    );
}

// Two rules with one match: Malformed at the later rule's first byte.
#[test]
fn rejects_two_rules_with_one_match() {
    let (a, b) = two_rules();
    let a2 = Rule { to_name: id("other"), ..a.clone() };
    let rules = [a.clone(), a2, b];
    let payload = raw_payload(&rules, [&[], &[], &[]]);
    let offset = base(&payload) + 1 + rule_len(&a);
    rejected(
        &decl(&payload),
        offset,
        &json_with("rules", json_rules(&rules)),
        "sections.declared.rules[1]",
    );
    assert_eq!(
        Declared::from_bytes(&decl(&payload)).unwrap_err().what,
        "two rules with one (parent, kind, name, disc)"
    );
    // The same rule twice.
    let rules = [a.clone(), a];
    let payload = raw_payload(&rules, [&[], &[], &[]]);
    let offset = base(&payload) + 1 + rule_len(&rules[0]);
    rejected(
        &decl(&payload),
        offset,
        &json_with("rules", json_rules(&rules)),
        "sections.declared.rules[1]",
    );
}

// Each exemption list unsorted, and with a duplicate: Malformed at the
// later element.
#[test]
fn rejects_each_list_unsorted_or_duplicated() {
    for (k, list) in ["changed", "created", "removed"].into_iter().enumerate() {
        for bad in [[d(1), d(3), d(2)], [d(1), d(2), d(2)]] {
            let mut lists: [&[Digest]; 3] = [&[], &[], &[]];
            lists[k] = &bad;
            let payload = raw_payload(&[], lists);
            // rules count, then the lists before this one (empty: one byte
            // each), this list's count, two digests.
            let offset = base(&payload) + 1 + k + 1 + 64;
            let path = format!("sections.declared.{list}[2]");
            rejected(&decl(&payload), offset, &json_with(list, json_keys(&bad)), &path);
            assert_eq!(
                Declared::from_bytes(&decl(&payload)).unwrap_err().what,
                format!("{list} is not strictly ascending")
            );
        }
    }
}

// model §5.4: the encodings of the whole section are read before the order
// of any list is checked.
#[test]
fn encodings_before_order() {
    let (a, b) = two_rules();
    let mut payload = raw_payload(&[b, a], [&[], &[], &[]]);
    payload.push(0);
    let e = Declared::from_bytes(&decl(&payload)).unwrap_err();
    assert_eq!((e.kind, e.what), (ErrorKind::Malformed, "bytes after the end"));
    assert_eq!(e.offset, base(&payload) + payload.len() - 1);
}

// Rejected names and parents inside a file, at the offset of the value.
#[test]
fn rejects_bad_names_in_a_file() {
    // rules: 1, parent root, kind namespace, name operator 0.
    let payload = [&[1u8, 0, 1, 2, 0][..], &[0; 6]].concat();
    let e = Declared::from_bytes(&decl(&payload)).unwrap_err();
    assert_eq!(
        (e.kind, e.offset, e.what),
        (ErrorKind::Malformed, base(&payload) + 4, "operator number is 0")
    );
    let json = parse(
        r#"{"kind": "DECL", "version": 1, "sections": {"declared": {"rules": [
        {"parent": "root", "kind": "namespace", "name": {"operator": 0}, "disc": null,
         "to_parent": "root", "to_name": "order"}], "changed": [], "created": [], "removed": []}}}"#,
    );
    assert_eq!(
        Declared::from_json(&json).unwrap_err(),
        JsonFormError {
            path: "sections.declared.rules[0].name.operator".into(),
            message: "operator number is 0".into()
        }
    );
    // A parent tag of 4.
    let payload = [&[1u8, 4][..], &[0; 8]].concat();
    let e = Declared::from_bytes(&decl(&payload)).unwrap_err();
    assert_eq!(
        (e.kind, e.offset, e.what),
        (ErrorKind::Malformed, base(&payload) + 1, "unlisted Parent value")
    );
}

// model §5 rule 2: an unknown critical section.
#[test]
fn rejects_unknown_section() {
    let payload = raw_payload(&[], [&[], &[], &[]]);
    let bytes = file(*b"DECL", 1, &[(1, &payload), (2, b"x")]);
    let e = Declared::from_bytes(&bytes).unwrap_err();
    assert_eq!((e.kind, e.offset), (ErrorKind::UnknownSection, HEADER_LEN + 1 + 1 + payload.len()));
    let mut v = Declared::none().to_json();
    let Value::Object(members) = &mut v else { unreachable!() };
    let Value::Object(sections) = &mut members[2].1 else { unreachable!() };
    sections.push(("extra".into(), Value::Null));
    assert_eq!(Declared::from_json(&v).unwrap_err().path, "sections.extra");
}

// model §5 rule 6, §5.4: the required section missing.
#[test]
fn rejects_missing_section() {
    let bytes = file(*b"DECL", 1, &[]);
    let e = Declared::from_bytes(&bytes).unwrap_err();
    assert_eq!((e.kind, e.offset), (ErrorKind::Malformed, bytes.len() - TRAILER_LEN - 1));
    let v = parse(r#"{"kind": "DECL", "version": 1, "sections": {}}"#);
    assert_eq!(
        Declared::from_json(&v).unwrap_err(),
        JsonFormError { path: "sections.declared".into(), message: "missing member".into() }
    );
}

// model §5.3: a wrong kind is BadMagic at the kind's first differing byte.
#[test]
fn rejects_wrong_kind() {
    let payload = raw_payload(&[], [&[], &[], &[]]);
    let bytes = file(*b"PLAN", 1, &[(1, &payload)]);
    let e = Declared::from_bytes(&bytes).unwrap_err();
    assert_eq!((e.kind, e.offset), (ErrorKind::BadMagic, 8));
    let mut v = Declared::none().to_json();
    set(&mut v, &["kind"], Value::Str("PLAN".into()));
    assert_eq!(Declared::from_json(&v).unwrap_err().path, "kind");
}

// model §5.3: a wrong version is UnsupportedVersion at byte 12. Version 4
// is read since WP-144 (`declared_v4_*`); 5 is the first unknown one.
#[test]
fn rejects_wrong_version() {
    let payload = raw_payload(&[], [&[], &[], &[]]);
    for version in [0u32, 5] {
        let mut bytes = file(*b"DECL", 1, &[(1, &payload)]);
        bytes[12..16].copy_from_slice(&version.to_le_bytes());
        let e = Declared::from_bytes(&bytes).unwrap_err();
        assert_eq!((e.kind, e.offset), (ErrorKind::UnsupportedVersion, 12));
        let mut v = Declared::none().to_json();
        set(&mut v, &["version"], Value::Int(i128::from(version)));
        assert_eq!(Declared::from_json(&v).unwrap_err().path, "version");
    }
}

// model §10.2: only the writer's rendering of a Name or Parent is accepted.
#[test]
fn rejects_other_json_forms() {
    let with_name = |name: &str| {
        parse(&format!(
            r#"{{"kind": "DECL", "version": 2, "sections": {{"declared": {{"rules": [
            {{"parent": "root", "kind": "namespace", "name": {name}, "disc": null,
             "to_parent": "root", "to_name": "order"}}], "changed": [], "created": [], "removed": [],
             "kept": [], "files": {{"owned": ["/"], "readonly": []}}}}}}}}"#
        ))
    };
    let at = "sections.declared.rules[0].name";
    for (name, path) in [
        (r#""identifier""#, at.to_owned()),
        (r#"{"constructor": null}"#, format!("{at}.constructor")),
        (r#"{"nonsense": 1}"#, format!("{at}.nonsense")),
        (r#""nonsense""#, at.to_owned()),
        (r#"{"identifier": "a", "local": 1}"#, at.to_owned()),
        ("{}", at.to_owned()),
        ("3", at.to_owned()),
        (r#"{"local": 1.0}"#, format!("{at}.local")),
        (r#"{"identifier": {"hex": "61"}}"#, format!("{at}.identifier.hex")),
    ] {
        let e = Declared::from_json(&with_name(name)).expect_err(name);
        assert_eq!(e.path, path, "{name}: {e}");
    }
    // A valid one, for contrast.
    assert!(Declared::from_json(&with_name(r#"{"unnamed_by": {"identifier": "T"}}"#)).is_ok());
    // unnamed_by nested nine deep: at the ninth.
    let mut name = r#"{"identifier": "T"}"#.to_owned();
    for _ in 0..9 {
        name = format!(r#"{{"unnamed_by": {name}}}"#);
    }
    let e = Declared::from_json(&with_name(&name)).unwrap_err();
    let ninth = format!("{at}{}", ".unnamed_by".repeat(8));
    assert_eq!(
        e,
        JsonFormError { path: ninth, message: "unnamed_by nested more than 8 deep".into() }
    );

    let mut v = golden_declared().to_json();
    for (parent, ok) in [
        (r#""unit""#, false),
        (r#"{"unit": "00"}"#, false),
        (r#"{"root": null}"#, false),
        (r#""global""#, false),
        (r#""root""#, true),
    ] {
        set(&mut v, &["sections", "declared", "rules", "0", "parent"], parse(parent));
        assert_eq!(Declared::from_json(&v).is_ok(), ok, "{parent}");
    }
    let mut v = golden_declared().to_json();
    set(&mut v, &["sections", "declared", "rules", "0", "kind"], Value::Str("namespaces".into()));
    assert_eq!(Declared::from_json(&v).unwrap_err().path, "sections.declared.rules[0].kind");
}

// ---------------------------------------------------------------------------
// WP-118: `files.rebuilt`, DECL version 3 (R5.3, R5.4, R5.5)

fn files3() -> Ownership {
    Ownership { rebuilt: vec![b"/src/build".to_vec()], ..files() }
}

fn paths(l: &[Vec<u8>]) -> Value {
    Value::Array(l.iter().map(|b| Value::Str(String::from_utf8(b.clone()).unwrap())).collect())
}

// R5.4: version 3 exactly for a rebuilt list; version 2, byte for byte as
// before, without one (the golden of `golden_declared_binary_and_json`
// is that); `dump`/`undump` keep the version; the JSON member `rebuilt`
// is present exactly in version 3.
#[test]
fn declared_v3_round_trip() {
    let two = golden_declared();
    let three = two.clone().with_files(Some(files3()));
    round_trip(&two);
    round_trip(&three);
    assert_eq!(two.to_bytes()[12..16], 2u32.to_le_bytes());
    assert_eq!(three.to_bytes()[12..16], 3u32.to_le_bytes());
    let files = |x: &Declared| get(&x.to_json(), &["sections", "declared", "files"]).clone();
    let Value::Object(f2) = files(&two) else { panic!("files is an object") };
    let Value::Object(f3) = files(&three) else { panic!("files is an object") };
    assert_eq!(f2.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(), ["owned", "readonly"]);
    assert_eq!(
        f3.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
        ["owned", "readonly", "rebuilt"]
    );
    assert_eq!(get(&three.to_json(), &["version"]), &Value::Int(3));
    assert!(dump(&three.to_bytes()).unwrap().contains("\"rebuilt\": ["));
}

// R5.3: each new row, in order, at its offset and JSON path; R5.4: a
// version 3 file with an empty `rebuilt` list is a second encoding,
// refused at the list; a version 2 JSON with a `rebuilt` member is refused.
#[test]
fn declared_v3_rejects_each_bad_rebuilt() {
    let p = |s: &str| s.as_bytes().to_vec();
    let rules = golden_rules();
    let owned = vec![p("/a")];
    let readonly = vec![p("/a/r")];
    // (rebuilt, message, index)
    let cases: [(Vec<Vec<u8>>, &str, usize); 7] = [
        (vec![p("/a/g"), p("g")], "not a canonical absolute path", 1),
        (vec![p("/a/g/")], "not a canonical absolute path", 0),
        (vec![p("/a/h"), p("/a/g")], "rebuilt is not strictly ascending", 1),
        (vec![p("/a/g"), p("/a/g")], "rebuilt is not strictly ascending", 1),
        (vec![p("/")], "a rebuilt entry is /", 0),
        (vec![p("/"), p("/a")], "a rebuilt entry is /", 0),
        (vec![p("/a")], "a rebuilt entry is also in owned", 0),
    ];
    let later: [(Vec<Vec<u8>>, &str, usize); 2] = [
        (vec![p("/a"), p("/a/r")], "a rebuilt entry is also in owned", 0),
        (vec![p("/a/g"), p("/a/r")], "a rebuilt entry is also in readonly", 1),
    ];
    for (rebuilt, what, i) in cases.into_iter().chain(later) {
        let o = Ownership { owned: owned.clone(), readonly: readonly.clone(), rebuilt };
        let (payload, at) = raw_payload_v2(&rules, [&[], &[], &[]], &[], Some(&o));
        let mut json = golden_declared().with_files(Some(files3())).to_json();
        for list in ["changed", "created", "removed", "kept"] {
            set(&mut json, &["sections", "declared", list], Value::Array(vec![]));
        }
        set(&mut json, &["sections", "declared", "files", "owned"], paths(&o.owned));
        set(&mut json, &["sections", "declared", "files", "readonly"], paths(&o.readonly));
        set(&mut json, &["sections", "declared", "files", "rebuilt"], paths(&o.rebuilt));
        let path = format!("sections.declared.files.rebuilt[{i}]");
        rejected(&decl3(&payload), base(&payload) + at.rebuilt_at[i], &json, &path);
        assert_eq!(Declared::from_bytes(&decl3(&payload)).unwrap_err().what, what);
        let x = Declared::new(rules.clone(), vec![], vec![], vec![]).unwrap().with_files(Some(o));
        assert!(x.validate().is_err(), "{what}");
    }
    // A version 3 file whose list is empty.
    let o = files();
    let (mut payload, _) = raw_payload_v2(&rules, [&[], &[], &[]], &[], Some(&o));
    let list_at = payload.len();
    payload.push(0);
    let mut json = golden_declared().with_files(Some(files3())).to_json();
    for list in ["changed", "created", "removed", "kept"] {
        set(&mut json, &["sections", "declared", list], Value::Array(vec![]));
    }
    set(&mut json, &["sections", "declared", "files", "rebuilt"], Value::Array(vec![]));
    rejected(&decl3(&payload), base(&payload) + list_at, &json, "sections.declared.files.rebuilt");
    assert_eq!(
        Declared::from_bytes(&decl3(&payload)).unwrap_err().what,
        "a version 3 container has no rebuilt entry"
    );
    // M20: a version 3 file with `files` absent (no rules, so a version 2
    // one would be valid): refused at the `files` tag.
    let (payload, at) = raw_payload_v2(&[], [&[], &[], &[]], &[], None);
    let mut json = Declared::none().to_json();
    set(&mut json, &["version"], Value::Int(3));
    rejected(&decl3(&payload), base(&payload) + at.files, &json, "sections.declared.files");
    assert_eq!(
        Declared::from_bytes(&decl3(&payload)).unwrap_err().what,
        "a version 3 container has no rebuilt entry"
    );
    // A version 2 JSON with the member: not version 2's record.
    let mut json = golden_declared().with_files(Some(files3())).to_json();
    set(&mut json, &["version"], Value::Int(2));
    assert!(Declared::from_json(&json).is_err());
}

// R5.5: `rules_id` keeps the v2 tag without a rebuilt list and uses
// `slip.rules.v3` (rules, owned, readonly, rebuilt) with one; three values
// that differ only in the class of one path have three ids.
#[test]
fn rules_id_v3() {
    let x = golden_declared().with_files(Some(files3()));
    let mut e = Encoder::new();
    e.count(x.rules().rules().len());
    for r in x.rules().rules() {
        r.encode(&mut e);
    }
    e.count(1).bytes(b"/src").count(1).bytes(b"/src/dep").count(1).bytes(b"/src/build");
    assert_eq!(x.rules_id(), tagged_digest(RULES_V3_TAG, e.as_bytes()));
    assert_ne!(RULES_V3_TAG, RULES_V2_TAG);
    let with = |owned: &[&str], readonly: &[&str], rebuilt: &[&str]| {
        let l = |v: &[&str]| v.iter().map(|s| s.as_bytes().to_vec()).collect::<Vec<_>>();
        golden_declared()
            .with_files(Some(Ownership {
                owned: l(owned),
                readonly: l(readonly),
                rebuilt: l(rebuilt),
            }))
            .rules_id()
    };
    let ids = [
        with(&["/src", "/src/b"], &[], &[]),
        with(&["/src"], &["/src/b"], &[]),
        with(&["/src"], &[], &["/src/b"]),
    ];
    assert!(ids[0] != ids[1] && ids[1] != ids[2] && ids[0] != ids[2], "{ids:?}");
    // The v2 id is the one it was.
    assert_eq!(golden_declared().rules_id(), with(&["/src"], &["/src/dep"], &[]));
}

// ---------------------------------------------------------------------------
// WP-144: `existing` / `existing_local`, DECL version 4

#[derive(Default)]
struct At4 {
    existing: usize,
    existing_at: Vec<usize>,
    existing_local: usize,
    existing_local_at: Vec<usize>,
    existing_local_file_at: Vec<usize>,
}

fn raw_payload_v4(
    rules: &[Rule],
    lists: [&[Digest]; 3],
    kept: &[Digest],
    files: Option<&Ownership>,
    existing: &[Digest],
    existing_local: &[ExistingLocal],
) -> (Vec<u8>, At4) {
    let mut e = Encoder::new();
    e.count(rules.len());
    for r in rules {
        r.encode(&mut e);
    }
    for list in lists {
        e.count(list.len());
        for k in list {
            e.digest(k);
        }
    }
    e.count(kept.len());
    for k in kept {
        e.digest(k);
    }
    match files {
        None => {
            e.u8(0);
        }
        Some(o) => {
            e.u8(1);
            for list in [&o.owned, &o.readonly, &o.rebuilt] {
                e.count(list.len());
                for p in list {
                    e.bytes(p);
                }
            }
        }
    }
    let mut at = At4 { existing: e.as_bytes().len(), ..At4::default() };
    e.count(existing.len());
    for k in existing {
        at.existing_at.push(e.as_bytes().len());
        e.digest(k);
    }
    at.existing_local = e.as_bytes().len();
    e.count(existing_local.len());
    for entry in existing_local {
        at.existing_local_at.push(e.as_bytes().len());
        e.digest(&entry.key);
        at.existing_local_file_at.push(e.as_bytes().len());
        e.bytes(&entry.file);
    }
    (e.into_bytes(), at)
}

fn decl4(payload: &[u8]) -> Vec<u8> {
    file(*b"DECL", 4, &[(1, payload)])
}

fn json_existing_local(list: &[ExistingLocal]) -> Value {
    Value::Array(
        list.iter()
            .map(|e| {
                Value::Object(vec![
                    ("key".into(), Value::Str(e.key.to_string())),
                    ("file".into(), Value::Str(String::from_utf8(e.file.clone()).unwrap())),
                ])
            })
            .collect(),
    )
}

fn local(k: u8, path: &str) -> ExistingLocal {
    ExistingLocal { key: d(k), file: path.as_bytes().to_vec() }
}

// WP-144: version 4 round-trips with `existing`, `existing_local`, or both,
// with empty or non-empty `rebuilt`; version 2 and 3 keep their exact bytes.
#[test]
fn declared_v4_round_trip() {
    let base_v2 = golden_declared();
    let base_v3 = base_v2.clone().with_files(Some(files3()));
    // Version 2 and 3 unchanged.
    assert_eq!(base_v2.to_bytes()[12..16], 2u32.to_le_bytes());
    assert_eq!(base_v3.to_bytes()[12..16], 3u32.to_le_bytes());

    let cases = [
        base_v2.clone().with_existing(vec![d(10), d(11)], vec![]).unwrap(),
        base_v2.clone().with_existing(vec![], vec![local(10, "/src/a.cpp")]).unwrap(),
        base_v3
            .clone()
            .with_existing(vec![d(10)], vec![local(10, "/src/a.cpp"), local(10, "/src/b.cpp")])
            .unwrap(),
    ];
    for x in &cases {
        x.validate().unwrap();
        let bytes = x.to_bytes();
        assert_eq!(bytes[12..16], 4u32.to_le_bytes());
        let (payload, _) = raw_payload_v4(
            x.rules().rules(),
            [x.changed(), x.created(), x.removed()],
            x.kept(),
            x.files(),
            x.existing(),
            x.existing_local(),
        );
        assert_eq!(bytes, decl4(&payload));
        let back = Declared::from_bytes(&bytes).unwrap();
        assert_eq!(&back, x);
        let text = json::write_pretty(&x.to_json()).unwrap();
        let from_text = Declared::from_json(&parse(&text)).unwrap();
        assert_eq!(&from_text, x);
        assert_eq!(from_text.to_bytes(), bytes);
        assert_eq!(dump(&bytes).unwrap(), text);
        assert_eq!(undump(&parse(&text)).unwrap(), bytes);
        assert_eq!(get(&x.to_json(), &["version"]), &Value::Int(4));
    }
    // Version 4 with empty `rebuilt` is accepted (unlike version 3).
    assert!(cases[0].files().unwrap().rebuilt.is_empty());
    assert!(dump(&cases[0].to_bytes()).unwrap().contains("\"rebuilt\": []"));
}

fn v4_base_json() -> Value {
    let mut v = golden_declared().with_existing(vec![d(1)], vec![]).unwrap().to_json();
    for list in ["changed", "created", "removed", "kept"] {
        set(&mut v, &["sections", "declared", list], Value::Array(vec![]));
    }
    v
}

// WP-144: reader checks between row 4 and row 5 (order and canonical path).
#[test]
fn declared_v4_reader_order_and_paths() {
    let rules = golden_rules();
    let o = files();

    // 1. existing is not strictly ascending.
    for bad in [[d(2), d(1)], [d(2), d(2)]] {
        let (payload, at) = raw_payload_v4(&rules, [&[], &[], &[]], &[], Some(&o), &bad, &[]);
        let mut json = v4_base_json();
        set(&mut json, &["sections", "declared", "existing"], json_keys(&bad));
        rejected(
            &decl4(&payload),
            base(&payload) + at.existing_at[1],
            &json,
            "sections.declared.existing[1]",
        );
        assert_eq!(
            Declared::from_bytes(&decl4(&payload)).unwrap_err().what,
            "existing is not strictly ascending"
        );
        assert_eq!(
            Declared::none().with_existing(bad.to_vec(), vec![]),
            Err(DeclaredError::NotAscending { list: "existing", index: 1 })
        );
    }

    // 2. existing_local is not strictly ascending (by key, then file).
    for bad in [
        [local(2, "/src/a.cpp"), local(1, "/src/b.cpp")],
        [local(1, "/src/b.cpp"), local(1, "/src/a.cpp")],
        [local(1, "/src/a.cpp"), local(1, "/src/a.cpp")],
    ] {
        let (payload, at) = raw_payload_v4(&rules, [&[], &[], &[]], &[], Some(&o), &[], &bad);
        let mut json = v4_base_json();
        set(&mut json, &["sections", "declared", "existing"], Value::Array(vec![]));
        set(&mut json, &["sections", "declared", "existing_local"], json_existing_local(&bad));
        rejected(
            &decl4(&payload),
            base(&payload) + at.existing_local_at[1],
            &json,
            "sections.declared.existing_local[1]",
        );
        assert_eq!(
            Declared::from_bytes(&decl4(&payload)).unwrap_err().what,
            "existing_local is not strictly ascending"
        );
        assert_eq!(
            Declared::none().with_existing(vec![], bad.to_vec()),
            Err(DeclaredError::NotAscending { list: "existing_local", index: 1 })
        );
    }

    // 3. existing_local[i].file not a canonical absolute path.
    for bad_path in ["src/a.cpp", "/src/a.cpp/", "/src/../a.cpp"] {
        let bad = [local(1, "/src/ok.cpp"), local(2, bad_path)];
        let (payload, at) = raw_payload_v4(&rules, [&[], &[], &[]], &[], Some(&o), &[], &bad);
        let mut json = v4_base_json();
        set(&mut json, &["sections", "declared", "existing"], Value::Array(vec![]));
        set(&mut json, &["sections", "declared", "existing_local"], json_existing_local(&bad));
        rejected(
            &decl4(&payload),
            base(&payload) + at.existing_local_file_at[1],
            &json,
            "sections.declared.existing_local[1].file",
        );
        assert_eq!(
            Declared::from_bytes(&decl4(&payload)).unwrap_err().what,
            "not a canonical absolute path"
        );
        let x = golden_declared().with_existing(vec![], bad.to_vec()).unwrap();
        assert!(x.validate().unwrap_err().contains("not a canonical absolute path"));
    }
}

// WP-144: reader checks between row 4 and row 5 (version and namespace rule).
#[test]
fn declared_v4_reader_version_and_rules() {
    let rules = golden_rules();
    let o = files();

    // 4. a version 4 container has no existing entry.
    let (payload, at) = raw_payload_v4(&rules, [&[], &[], &[]], &[], Some(&o), &[], &[]);
    let mut json = v4_base_json();
    set(&mut json, &["sections", "declared", "existing"], Value::Array(vec![]));
    rejected(&decl4(&payload), base(&payload) + at.existing, &json, "sections.declared.existing");
    assert_eq!(
        Declared::from_bytes(&decl4(&payload)).unwrap_err().what,
        "a version 4 container has no existing entry"
    );

    // 5. version 2 and version 3 JSON with existing members refused.
    for vnum in [2i128, 3] {
        let mut json = v4_base_json();
        set(&mut json, &["version"], Value::Int(vnum));
        assert!(Declared::from_json(&json).is_err(), "v{vnum}");
    }

    // 6. existing without a namespace rule (no rules, and non-namespace rule only).
    let fn_rule = vec![rule(
        Parent::Unit(d(0x1e)),
        KeyKind::Function,
        id("g"),
        Some(d(0xd1)),
        (Parent::Unit(d(0xac)), id("f")),
    )];
    let (payload, at) = raw_payload_v4(&fn_rule, [&[], &[], &[]], &[], Some(&o), &[d(9)], &[]);
    let mut json = v4_base_json();
    set(&mut json, &["sections", "declared", "rules"], json_rules(&fn_rule));
    set(&mut json, &["sections", "declared", "existing"], json_keys(&[d(9)]));
    rejected(&decl4(&payload), base(&payload) + at.existing, &json, "sections.declared.existing");
    assert_eq!(
        Declared::from_bytes(&decl4(&payload)).unwrap_err().what,
        "existing without a namespace rule"
    );
    let x = Declared::new(fn_rule, vec![], vec![], vec![])
        .unwrap()
        .with_files(Some(o.clone()))
        .with_existing(vec![], vec![local(9, "/src/a.cpp")])
        .unwrap();
    assert!(x.validate().unwrap_err().contains("existing without a namespace rule"));
}

// WP-144: `rules_id` uses `slip.rules.v4` iff `existing` or `existing_local`
// is non-empty; v1, v2, and v3 ids are unchanged when both are empty.
#[test]
fn rules_id_v4() {
    let x = golden_declared()
        .with_files(Some(files3()))
        .with_existing(vec![d(10)], vec![local(11, "/src/a.cpp")])
        .unwrap();
    let mut e = Encoder::new();
    e.count(x.rules().rules().len());
    for r in x.rules().rules() {
        r.encode(&mut e);
    }
    e.count(1).bytes(b"/src").count(1).bytes(b"/src/dep").count(1).bytes(b"/src/build");
    e.count(1).digest(&d(10));
    e.count(1).digest(&d(11)).bytes(b"/src/a.cpp");
    assert_eq!(x.rules_id(), tagged_digest(RULES_V4_TAG, e.as_bytes()));
    assert_ne!(RULES_V4_TAG, RULES_V3_TAG);
    assert_ne!(RULES_V4_TAG, RULES_V2_TAG);
    // Changing existing or existing_local changes rules_id.
    let y = golden_declared()
        .with_files(Some(files3()))
        .with_existing(vec![d(10)], vec![local(11, "/src/b.cpp")])
        .unwrap();
    assert_ne!(x.rules_id(), y.rules_id());
    let z = golden_declared()
        .with_files(Some(files3()))
        .with_existing(vec![d(12)], vec![local(11, "/src/a.cpp")])
        .unwrap();
    assert_ne!(x.rules_id(), z.rules_id());
}

// WP-144 (S6): binary and JSON goldens for `DECL` version 4 (`declared-v4.bin`,
// `declared-v4.json`), with both `existing` and `existing_local`.
#[test]
fn golden_declared_v4_binary_and_json() {
    let x = golden_declared()
        .with_files(Some(files3()))
        .with_existing(vec![d(10), d(11)], vec![local(12, "/src/a.cpp"), local(12, "/src/b.cpp")])
        .unwrap();
    x.validate().unwrap();
    let bytes = x.to_bytes();
    assert_eq!(bytes[12..16], 4u32.to_le_bytes());
    golden("declared-v4.bin", &bytes);
    let text = dump(&bytes).unwrap();
    golden("declared-v4.json", text.as_bytes());
    assert_eq!(Declared::from_bytes(&bytes).unwrap(), x);
    assert_eq!(Declared::from_json(&parse(&text)).unwrap(), x);
    assert_eq!(undump(&parse(&text)).unwrap(), bytes);
}
