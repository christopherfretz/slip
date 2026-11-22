//! Declared changes and their container, kind `DECL` (`spec/verify.md` §2;
//! `spec/model.md` §2, §5, §10; the encodings, offsets and paths of
//! `spec/landed/model/WP-022.md`). Work package WP-022. Each test names
//! the section it covers.

use std::fs;
use std::path::Path;

use slip_model::Digest;
use slip_model::container::{HEADER_LEN, TRAILER_LEN, Writer};
use slip_model::declared::{Declared, DeclaredError};
use slip_model::formats::{JsonFormError, content_id, dump, undump};
use slip_model::hash::tagged_digest;
use slip_model::json::{self, Value};
use slip_model::keys::{KeyKind, Name, Parent, RULES_TAG, Rule, RuleError, RuleTable};
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

fn golden_declared() -> Declared {
    Declared::new(golden_rules(), vec![d(1), d(2)], vec![d(2)], vec![d(3), d(4)]).unwrap()
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

fn file(kind: [u8; 4], version: u32, sections: &[(u64, &[u8])]) -> Vec<u8> {
    let mut w = Writer::new(kind, version);
    for (id, p) in sections {
        w.section(*id, p);
    }
    w.finish()
}

fn decl(payload: &[u8]) -> Vec<u8> {
    file(*b"DECL", 1, &[(1, payload)])
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
    let payload = raw_payload(x.rules().rules(), [x.changed(), x.created(), x.removed()]);
    assert_eq!(bytes, decl(&payload), "to_bytes agrees with the test's encoder");
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
}

// verify §2: every KeyKind round-trips in a rule, in both forms.
#[test]
fn every_key_kind_round_trips_in_a_container() {
    let rules = KeyKind::ALL
        .into_iter()
        .map(|kind| rule(Parent::Root, kind, id("x"), None, (Parent::Root, id("y"))))
        .collect();
    let x = Declared::new(sorted(rules), vec![], vec![], vec![]).unwrap();
    round_trip(&x);
    let text = dump(&x.to_bytes()).unwrap();
    for kind in KeyKind::ALL {
        assert!(text.contains(&format!("\"kind\": \"{}\"", kind.name())), "{kind:?}");
    }
}

// verify §2: `none()` is four empty lists, and its rules' id is the id of
// the empty table.
#[test]
fn none_is_four_empty_lists() {
    let x = Declared::none();
    assert!(x.rules().is_empty());
    assert!(x.changed().is_empty() && x.created().is_empty() && x.removed().is_empty());
    assert_eq!(x.rules().id(), RuleTable::empty().id());
    assert_eq!(x.rules().id(), tagged_digest(RULES_TAG, &[0]));
    assert_eq!(Declared::new(vec![], vec![], vec![], vec![]).unwrap(), x);
    round_trip(&x);
    assert_eq!(x.to_bytes(), decl(&[0, 0, 0, 0]));
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

// model §5.3: a wrong version is UnsupportedVersion at byte 12.
#[test]
fn rejects_wrong_version() {
    let payload = raw_payload(&[], [&[], &[], &[]]);
    for version in [0u32, 2] {
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
            r#"{{"kind": "DECL", "version": 1, "sections": {{"declared": {{"rules": [
            {{"parent": "root", "kind": "namespace", "name": {name}, "disc": null,
             "to_parent": "root", "to_name": "order"}}], "changed": [], "created": [], "removed": []}}}}}}"#
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
