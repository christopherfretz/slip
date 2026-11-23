//! Keys, names, rules and rule tables (`spec/digest.md` §3, §3.4;
//! `spec/digest-kinds.md` §3.1, §3.2; the encodings of
//! `spec/landed/model/WP-022.md`). Work package WP-022. Each test names the
//! section it covers.

use slip_model::Digest;
use slip_model::hash::tagged_digest;
use slip_model::keys::{
    DISC_TAG, KEY_TAG, Key, KeyKind, Name, Parent, RULES_TAG, Rule, RuleError, RuleTable,
    UNNAMED_BY_MAX,
};
use slip_model::wire::{Decoder, Encoder, ErrorKind};

// ---------------------------------------------------------------------------
// Helpers

fn d(n: u8) -> Digest {
    Digest([n; 32])
}

fn hex(parts: &[&str]) -> Vec<u8> {
    let text: String = parts.concat().chars().filter(|c| !c.is_whitespace()).collect();
    assert!(text.len().is_multiple_of(2), "odd hex in test");
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap()).collect()
}

fn rep(byte: &str, n: usize) -> String {
    byte.repeat(n)
}

fn id(s: &str) -> Name {
    Name::Identifier(s.as_bytes().to_vec())
}

fn key(kind: KeyKind, parent: Parent, name: Name, disc: Option<Digest>) -> Key {
    Key { kind, parent, name, disc }
}

/// Every form of `Name`, one of each, `unnamed_by` nested twice.
fn every_name() -> Vec<Name> {
    vec![
        id("f"),
        Name::Operator(5),
        Name::Constructor,
        Name::Destructor,
        Name::Conversion(d(0x7e)),
        Name::LiteralOperator(b"_km".to_vec()),
        Name::DeductionGuide,
        Name::Anonymous,
        Name::Content,
        Name::Local(3),
        Name::UnnamedBy(Box::new(Name::UnnamedBy(Box::new(id("T"))))),
        Name::UnnamedOrdinal(2),
        Name::Property(7),
        Name::Order,
        Name::Parameter(1),
    ]
}

/// Decoding `bytes` as one key: the error's kind and offset.
fn key_error(bytes: &[u8]) -> (ErrorKind, usize, &'static str) {
    let e = Key::from_bytes(bytes).expect_err("expected an error");
    (e.kind, e.offset, e.what)
}

fn unnamed_by(depth: usize, inner: Name) -> Name {
    (0..depth).fold(inner, |n, _| Name::UnnamedBy(Box::new(n)))
}

// ---------------------------------------------------------------------------
// Tables (digest-kinds §3.1, §3.2)

// digest-kinds §3.1: every KeyKind has its number, its name, and
// round-trips; 24 (`explicit_instantiation`) reads although it is unused.
#[test]
fn every_key_kind_round_trips() {
    assert_eq!(KeyKind::ALL.len(), 33);
    for (i, kind) in KeyKind::ALL.into_iter().enumerate() {
        assert_eq!(kind.code(), i as u64 + 1);
        let k = key(kind, Parent::Root, id("x"), None);
        let bytes = k.to_bytes();
        assert_eq!(bytes[0], i as u8 + 1, "{kind:?}");
        assert_eq!(Key::from_bytes(&bytes).unwrap(), k);
    }
    assert_eq!(KeyKind::ExplicitInstantiation.code(), 24);
    assert_eq!(KeyKind::ExplicitInstantiation.name(), "explicit_instantiation");
    assert_eq!(KeyKind::UnresolvedIfExists.name(), "unresolved_if_exists");
    assert_eq!(KeyKind::Namespace.name(), "namespace");
}

// digest-kinds §3.2: every Name form has its number and its "Then", and
// round-trips.
#[test]
fn every_name_form_round_trips() {
    let expected: [(&str, String); 15] = [
        ("identifier", "01 01 66".into()),
        ("operator", "02 05".into()),
        ("constructor", "03".into()),
        ("destructor", "04".into()),
        ("conversion", format!("05 {}", rep("7e", 32))),
        ("literal_operator", "06 03 5f 6b 6d".into()),
        ("deduction_guide", "07".into()),
        ("anonymous", "08".into()),
        ("content", "09".into()),
        ("local", "0a 03".into()),
        ("unnamed_by", "0b 0b 01 01 54".into()),
        ("unnamed_ordinal", "0c 02".into()),
        ("property", "0d 07".into()),
        ("order", "0e".into()),
        ("parameter", "0f 01".into()),
    ];
    for (i, (name, (form, bytes))) in every_name().into_iter().zip(expected).enumerate() {
        assert_eq!(name.code(), i as u64 + 1);
        assert_eq!(name.form(), form);
        let mut e = Encoder::new();
        name.encode(&mut e);
        assert_eq!(e.as_bytes(), hex(&[&bytes]), "{form}");
        let mut dec = Decoder::new(e.as_bytes());
        assert_eq!(Name::decode(&mut dec).unwrap(), name);
        dec.finish().unwrap();
        let k = key(KeyKind::Function, Parent::Unit(d(1)), name.clone(), Some(d(2)));
        assert_eq!(Key::from_bytes(&k.to_bytes()).unwrap(), k);
    }
    // Large values: `uv` of every width.
    for n in [1, 127, 128, u64::MAX] {
        for name in [
            Name::Operator(n),
            Name::Local(n),
            Name::UnnamedOrdinal(n),
            Name::Property(n),
            Name::Parameter(n),
        ] {
            let k = key(KeyKind::Variable, Parent::Root, name, None);
            assert_eq!(Key::from_bytes(&k.to_bytes()).unwrap(), k);
        }
    }
    // Non-UTF-8 spellings are bytes.
    let k = key(KeyKind::Variable, Parent::Root, Name::Identifier(vec![0xff, 0]), None);
    assert_eq!(Key::from_bytes(&k.to_bytes()).unwrap(), k);
}

// digest §3: every Parent form round-trips.
#[test]
fn every_parent_round_trips() {
    for (parent, bytes) in [
        (Parent::Root, "00".to_owned()),
        (Parent::CLinkage, "01".to_owned()),
        (Parent::Builtin, "02".to_owned()),
        (Parent::Unit(d(9)), format!("03 {}", rep("09", 32))),
    ] {
        let mut e = Encoder::new();
        parent.encode(&mut e);
        assert_eq!(e.as_bytes(), hex(&[&bytes]));
        let mut dec = Decoder::new(e.as_bytes());
        assert_eq!(Parent::decode(&mut dec).unwrap(), parent);
        dec.finish().unwrap();
    }
}

// ---------------------------------------------------------------------------
// Golden keys (digest §3, model §3): the bytes and KeyIds of hand-written
// keys. A change of encoding or of the hash fails here with the bytes shown.

/// `key`'s encoding is `bytes` and its id is `expected`, which is
/// `H("slip.key.v1", bytes)`.
fn golden_key(key: &Key, bytes: &[u8], expected: &str) -> Digest {
    assert_eq!(hex_of(&key.to_bytes()), hex_of(bytes), "encoding of {key:?}");
    let id = key.id();
    assert_eq!(id, tagged_digest(KEY_TAG, bytes));
    assert_eq!(id.to_string(), expected, "KeyId of {key:?}");
    assert_eq!(&Key::from_bytes(bytes).unwrap(), key);
    id
}

fn hex_of(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

const ACME: &str = "fc1b29dc5bdbba22f3de4b4b9bf36ce86b96f6d2d579ac3905177bd5aeb2d8f8";
const WIDGET: &str = "ab6b1a272e869b1e55f8bb521e29312a9234947a905dde67d6f6ee8d0fbc081f";
const FUNCTION: &str = "96b13d110ae6b4ae341640e73801742599f28f9a2c71bfef7ef1b92f25f6c4ee";
const UNNAMED: &str = "60ab2eaa4525a35132051fe6cd1a32d4b9b74fb4448d7104673d377f6b71df68";
const CONVERSION: &str = "21928505e9ae97c6345a7523182b6dcf6beba633d5b0f36b63b0fea4921a80cf";
const CONTENT: &str = "b30f53122b125c6854855dfd7bf19e36ef6befa59e63fcf556048b0c0640d790";

// digest §3: a namespace in the root; a class in it; a function in it with
// a discriminator; an unnamed class named by its typedef; a conversion
// function of the class; a static_assert keyed by content.
#[test]
fn golden_key_ids() {
    // namespace acme
    let acme = key(KeyKind::Namespace, Parent::Root, id("acme"), None);
    let acme_id = golden_key(&acme, &hex(&["01", "00", "01 04 61636d65", "00"]), ACME);

    // class acme::widget
    let widget = key(KeyKind::Class, Parent::Unit(acme_id), id("widget"), None);
    let widget_id =
        golden_key(&widget, &hex(&["02", "03", ACME, "01 06 776964676574", "00"]), WIDGET);

    // void acme::f(…), its signature's digest d1…
    let f = key(KeyKind::Function, Parent::Unit(acme_id), id("f"), Some(d(0xd1)));
    golden_key(&f, &hex(&["05", "03", ACME, "01 01 66", "01", &rep("d1", 32)]), FUNCTION);

    // typedef struct { … } T; at namespace scope
    let unnamed = key(KeyKind::Class, Parent::Root, Name::UnnamedBy(Box::new(id("T"))), None);
    golden_key(&unnamed, &hex(&["02", "00", "0b 01 01 54", "00"]), UNNAMED);

    // acme::widget::operator T(), T's type digest 7e…, signature d2…
    let conversion =
        key(KeyKind::Function, Parent::Unit(widget_id), Name::Conversion(d(0x7e)), Some(d(0xd2)));
    golden_key(
        &conversion,
        &hex(&["05", "03", WIDGET, "05", &rep("7e", 32), "01", &rep("d2", 32)]),
        CONVERSION,
    );

    // static_assert(…); at namespace scope, its unit's digest c0…
    let content = key(KeyKind::StaticAssert, Parent::Root, Name::Content, Some(d(0xc0)));
    golden_key(&content, &hex(&["17", "00", "09", "01", &rep("c0", 32)]), CONTENT);
}

// model §3: the tags are the registered ones.
#[test]
fn tags() {
    assert_eq!(KEY_TAG, "slip.key.v1");
    assert_eq!(DISC_TAG, "slip.disc.v1");
    assert_eq!(RULES_TAG, "slip.rules.v1");
}

// digest §3: keys that differ in exactly one of kind, parent, name,
// discriminator have different ids; an identifier spelled like an operator
// is not the operator.
#[test]
fn keys_differing_in_one_field_differ() {
    let base = key(KeyKind::Function, Parent::Unit(d(1)), id("f"), Some(d(2)));
    let variants = [
        Key { kind: KeyKind::Variable, ..base.clone() },
        Key { kind: KeyKind::FunctionTemplate, ..base.clone() },
        Key { parent: Parent::Unit(d(3)), ..base.clone() },
        Key { parent: Parent::Root, ..base.clone() },
        Key { parent: Parent::CLinkage, ..base.clone() },
        Key { parent: Parent::Builtin, ..base.clone() },
        Key { name: id("g"), ..base.clone() },
        Key { name: Name::UnnamedBy(Box::new(id("f"))), ..base.clone() },
        Key { disc: Some(d(4)), ..base.clone() },
        Key { disc: None, ..base.clone() },
    ];
    let mut ids = vec![base.id()];
    ids.extend(variants.iter().map(Key::id));
    let mut sorted = ids.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), ids.len());

    let plus = key(KeyKind::Function, Parent::Root, id("operator+"), None);
    for n in 1..64 {
        let op = key(KeyKind::Function, Parent::Root, Name::Operator(n), None);
        assert_ne!(plus.id(), op.id());
    }
    // Every name form gives a different id.
    let mut ids: Vec<Digest> = every_name()
        .into_iter()
        .map(|n| key(KeyKind::Function, Parent::Root, n, None).id())
        .collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 15);
}

// ---------------------------------------------------------------------------
// Rejected encodings (landed model WP-022; model §2.1, §2.2)

// An unknown KeyKind number: Malformed at its first byte.
#[test]
fn rejects_unknown_key_kind() {
    for kind in ["00", "22", "80 01"] {
        let bytes = hex(&[kind, "00 03 00"]);
        assert_eq!(key_error(&bytes), (ErrorKind::Malformed, 0, "unlisted KeyKind value"));
    }
    // 24 is listed.
    assert_eq!(
        Key::from_bytes(&hex(&["18 00 03 00"])).unwrap().kind,
        KeyKind::ExplicitInstantiation
    );
}

// An unknown Name number: Malformed at its first byte.
#[test]
fn rejects_unknown_name() {
    for name in ["00", "10", "ff 01"] {
        let bytes = hex(&["01 00", name, "00"]);
        assert_eq!(key_error(&bytes), (ErrorKind::Malformed, 2, "unlisted Name value"));
    }
}

// A Parent tag above 3: Malformed at the tag.
#[test]
fn rejects_parent_tag_above_3() {
    for tag in ["04", "7f", "80 01"] {
        let bytes = hex(&["01", tag, "03 00"]);
        assert_eq!(key_error(&bytes), (ErrorKind::Malformed, 1, "unlisted Parent value"));
    }
}

// `unnamed_by` nested more than 8 deep: Malformed at the ninth's number.
#[test]
fn rejects_unnamed_by_nested_too_deep() {
    assert_eq!(UNNAMED_BY_MAX, 8);
    let ok = key(KeyKind::Class, Parent::Root, unnamed_by(8, id("T")), None);
    assert_eq!(Key::from_bytes(&ok.to_bytes()).unwrap(), ok);
    assert_eq!(ok.name.validate(), Ok(()));
    let deep = key(KeyKind::Class, Parent::Root, unnamed_by(9, id("T")), None);
    assert_eq!(
        key_error(&deep.to_bytes()),
        (ErrorKind::Malformed, 2 + 8, "unnamed_by nested more than 8 deep")
    );
    assert_eq!(deep.name.validate(), Err("unnamed_by nested more than 8 deep"));
    // Far deeper: the reader stops at the ninth, it does not recurse.
    let bytes = [hex(&["02 00"]), vec![0x0b; 100_000]].concat();
    assert_eq!(key_error(&bytes).1, 10);
}

// An operator number of 0: Malformed at the number.
#[test]
fn rejects_operator_zero() {
    let bytes = hex(&["05 00 02 00 00"]);
    assert_eq!(key_error(&bytes), (ErrorKind::Malformed, 3, "operator number is 0"));
    assert_eq!(Name::Operator(0).validate(), Err("operator number is 0"));
    assert_eq!(
        unnamed_by(1, Name::Operator(0)).validate(),
        Err("operator number is 0"),
        "inside unnamed_by"
    );
}

// The discriminator's opt tag other than 0 or 1: Malformed at the tag.
#[test]
fn rejects_bad_disc_tag() {
    assert_eq!(
        key_error(&hex(&["01 00 03 02"])),
        (ErrorKind::Malformed, 3, "opt tag is not 0 or 1")
    );
}

// Bytes after the key: Malformed at the first one.
#[test]
fn rejects_trailing_bytes() {
    let bytes = hex(&["01 00 03 00 00"]);
    assert_eq!(key_error(&bytes), (ErrorKind::Malformed, 4, "bytes after the end"));
}

// Truncated and too large (model §2.1): a key cut short, an identifier
// longer than the input.
#[test]
fn rejects_truncated() {
    let full = key(KeyKind::Function, Parent::Unit(d(1)), id("f"), Some(d(2))).to_bytes();
    for len in 0..full.len() {
        let (kind, offset, _) = key_error(&full[..len]);
        assert!(matches!(kind, ErrorKind::Truncated | ErrorKind::TooLarge), "{len}: {kind}");
        if kind == ErrorKind::Truncated {
            assert_eq!(offset, len);
        }
    }
    assert_eq!(key_error(&hex(&["01 00 01 05 61"])).0, ErrorKind::TooLarge);
    assert_eq!(key_error(&hex(&["01 00 01 05 61"])).1, 3);
}

// ---------------------------------------------------------------------------
// RuleTable (digest §3.4, verify §2, §2.1)

fn rule(
    parent: Parent,
    kind: KeyKind,
    name: Name,
    disc: Option<Digest>,
    to: (Parent, Name),
) -> Rule {
    Rule { parent, kind, name, disc, to_parent: to.0, to_name: to.1 }
}

/// `acme::f` (kind function, disc d1) is now called `legacy::g`.
fn f_rule() -> Rule {
    rule(
        Parent::Unit(d(0xac)),
        KeyKind::Function,
        id("f"),
        Some(d(0xd1)),
        (Parent::Unit(d(0x1e)), id("g")),
    )
}

fn f_key() -> Key {
    key(KeyKind::Function, Parent::Unit(d(0xac)), id("f"), Some(d(0xd1)))
}

// digest §3.4: with no rule, mapping is the identity.
#[test]
fn rule_table_empty() {
    let t = RuleTable::empty();
    assert!(t.is_empty());
    assert!(t.rules().is_empty());
    assert_eq!(t.map(f_key()), f_key());
    assert_eq!(RuleTable::new(vec![]).unwrap(), t);
    assert_eq!(t.id(), tagged_digest(RULES_TAG, &[0]));
}

// digest §3.4: a rule that matches replaces parent and name and keeps kind
// and discriminator.
#[test]
fn rule_table_match() {
    let t = RuleTable::new(vec![f_rule()]).unwrap();
    assert!(!t.is_empty());
    assert_eq!(
        t.map(f_key()),
        key(KeyKind::Function, Parent::Unit(d(0x1e)), id("g"), Some(d(0xd1)))
    );
    // A rule may give any parent, root included, and any name.
    let to_root = rule(
        Parent::Unit(d(0xac)),
        KeyKind::Namespace,
        id("acme"),
        None,
        (Parent::Root, id("legacy")),
    );
    let t = RuleTable::new(vec![to_root]).unwrap();
    assert_eq!(
        t.map(key(KeyKind::Namespace, Parent::Unit(d(0xac)), id("acme"), None)),
        key(KeyKind::Namespace, Parent::Root, id("legacy"), None)
    );
}

// digest §3.4: a rule matches only its exact (parent, kind, name, disc).
#[test]
fn rule_table_no_match_kind() {
    let t = RuleTable::new(vec![f_rule()]).unwrap();
    let k = Key { kind: KeyKind::FunctionTemplate, ..f_key() };
    assert_eq!(t.map(k.clone()), k);
}

#[test]
fn rule_table_no_match_parent() {
    let t = RuleTable::new(vec![f_rule()]).unwrap();
    for parent in [Parent::Unit(d(0xad)), Parent::Root, Parent::CLinkage, Parent::Builtin] {
        let k = Key { parent, ..f_key() };
        assert_eq!(t.map(k.clone()), k);
    }
}

#[test]
fn rule_table_no_match_name() {
    let t = RuleTable::new(vec![f_rule()]).unwrap();
    for name in [id("F"), id("ff"), Name::UnnamedBy(Box::new(id("f"))), Name::Operator(1)] {
        let k = Key { name, ..f_key() };
        assert_eq!(t.map(k.clone()), k);
    }
}

#[test]
fn rule_table_no_match_disc() {
    let t = RuleTable::new(vec![f_rule()]).unwrap();
    for disc in [None, Some(d(0xd2))] {
        let k = Key { disc, ..f_key() };
        assert_eq!(t.map(k.clone()), k);
    }
    // A rule without a discriminator does not match a key with one.
    let t = RuleTable::new(vec![Rule { disc: None, ..f_rule() }]).unwrap();
    assert_eq!(t.map(f_key()), f_key());
}

// verify §2.1: renaming f to g and g to f is two rules; each key is mapped
// once, and rules do not chain.
#[test]
fn rule_table_swap() {
    let p = Parent::Unit(d(0xac));
    let t = RuleTable::new(vec![
        rule(p, KeyKind::Function, id("f"), None, (p, id("g"))),
        rule(p, KeyKind::Function, id("g"), None, (p, id("f"))),
    ])
    .unwrap();
    let f = key(KeyKind::Function, p, id("f"), None);
    let g = key(KeyKind::Function, p, id("g"), None);
    assert_eq!(t.map(f.clone()), g);
    assert_eq!(t.map(g.clone()), f);
    assert_eq!(t.map(t.map(f.clone())), f, "map(map(k)) is not map(k)");
}

// verify §2: two rules with one (parent, kind, name, disc) are refused,
// whatever they map to and in whatever order they are given.
#[test]
fn rule_table_duplicate_refused() {
    let other = Rule { to_name: id("h"), ..f_rule() };
    let unrelated = Rule { name: id("z"), ..f_rule() };
    assert_eq!(
        RuleTable::new(vec![f_rule(), unrelated.clone(), other.clone()]),
        Err(RuleError::Duplicate { first: 0, second: 2 })
    );
    assert_eq!(
        RuleTable::new(vec![other, unrelated, f_rule()]),
        Err(RuleError::Duplicate { first: 0, second: 2 })
    );
    assert_eq!(
        RuleTable::new(vec![f_rule(), f_rule()]),
        Err(RuleError::Duplicate { first: 0, second: 1 })
    );
    // Names no reader accepts.
    assert_eq!(
        RuleTable::new(vec![f_rule(), Rule { to_name: Name::Operator(0), ..f_rule() }]),
        Err(RuleError::InvalidName { index: 1, what: "operator number is 0" })
    );
    assert_eq!(
        RuleTable::new(vec![Rule { name: unnamed_by(9, id("T")), ..f_rule() }]),
        Err(RuleError::InvalidName { index: 0, what: "unnamed_by nested more than 8 deep" })
    );
}

fn table_rules() -> Vec<Rule> {
    vec![
        f_rule(),
        rule(Parent::Root, KeyKind::Namespace, id("acme"), None, (Parent::Root, id("legacy"))),
        rule(Parent::Unit(d(0xac)), KeyKind::Class, id("w"), None, (Parent::Root, id("w"))),
        rule(Parent::Unit(d(0x01)), KeyKind::Class, id("w"), None, (Parent::Root, id("v"))),
        rule(Parent::Root, KeyKind::Function, id("f"), Some(d(0xd0)), (Parent::Root, id("g"))),
        rule(Parent::Root, KeyKind::Function, id("aa"), None, (Parent::Root, id("b"))),
        rule(Parent::Root, KeyKind::Function, id("b"), None, (Parent::Root, id("aa"))),
        rule(
            Parent::CLinkage,
            KeyKind::Function,
            Name::Operator(3),
            None,
            (Parent::Builtin, id("x")),
        ),
    ]
}

// verify §2: `rules()` is sorted by the bytes of (parent, kind, name, disc),
// and `id` is H("slip.rules.v1", list<Rule>) over that order, whatever the
// order given.
#[test]
fn rule_table_sorted_and_id_independent_of_order() {
    let rules = table_rules();
    let t = RuleTable::new(rules.clone()).unwrap();
    let matched = |r: &Rule| {
        let mut e = Encoder::new();
        r.parent.encode(&mut e);
        r.kind.encode(&mut e);
        r.name.encode(&mut e);
        match r.disc {
            None => e.u8(0),
            Some(x) => e.u8(1).digest(&x),
        };
        e.into_bytes()
    };
    let got: Vec<Vec<u8>> = t.rules().iter().map(matched).collect();
    assert!(got.windows(2).all(|w| w[0] < w[1]), "rules() ascending by match bytes");
    // By bytes: `b` (length 1) before `aa` (length 2), unlike the derived order.
    let names: Vec<&Name> = t
        .rules()
        .iter()
        .filter(|r| r.parent == Parent::Root && r.kind == KeyKind::Function && r.disc.is_none())
        .map(|r| &r.name)
        .collect();
    assert_eq!(names, [&id("b"), &id("aa")]);

    let mut e = Encoder::new();
    e.count(t.rules().len());
    for r in t.rules() {
        r.encode(&mut e);
    }
    assert_eq!(t.id(), tagged_digest(RULES_TAG, e.as_bytes()));

    for shift in 1..rules.len() {
        let mut other = rules.clone();
        other.rotate_left(shift);
        let u = RuleTable::new(other).unwrap();
        assert_eq!(u.rules(), t.rules());
        assert_eq!(u.id(), t.id());
    }
    let mut reversed = rules.clone();
    reversed.reverse();
    assert_eq!(RuleTable::new(reversed).unwrap().id(), t.id());

    // Different tables, different ids.
    let fewer = RuleTable::new(rules[1..].to_vec()).unwrap();
    assert_ne!(fewer.id(), t.id());
    let mut changed = rules.clone();
    changed[0].to_name = id("h");
    assert_ne!(RuleTable::new(changed).unwrap().id(), t.id());
    assert_ne!(RuleTable::empty().id(), t.id());

    // Each rule maps the key it matches, and only that.
    for r in t.rules() {
        let k = key(r.kind, r.parent, r.name.clone(), r.disc);
        assert_eq!(t.map(k), key(r.kind, r.to_parent, r.to_name.clone(), r.disc));
    }
}

// verify §2: a rule's encoding is parent, kind, name, disc, to_parent,
// to_name.
#[test]
fn rule_bytes() {
    let r = rule(Parent::Root, KeyKind::Namespace, id("b"), None, (Parent::Unit(d(5)), id("a")));
    let mut e = Encoder::new();
    r.encode(&mut e);
    assert_eq!(e.as_bytes(), hex(&["00 01 01 01 62 00 03", &rep("05", 32), "01 01 61"]));
    let mut dec = Decoder::new(e.as_bytes());
    assert_eq!(Rule::decode(&mut dec).unwrap(), r);
    dec.finish().unwrap();
}

// ---------------------------------------------------------------------------
// Rule use (verify §9.1, §9.5; WP-069)

/// Every table and key of the rule tests above, with the rule index
/// `map_traced` must report.
fn traced_cases() -> Vec<(RuleTable, Key, Option<usize>)> {
    let mut out = Vec::new();
    let one = RuleTable::new(vec![f_rule()]).unwrap();
    out.push((RuleTable::empty(), f_key(), None));
    out.push((one.clone(), f_key(), Some(0)));
    out.push((one.clone(), Key { kind: KeyKind::FunctionTemplate, ..f_key() }, None));
    for parent in [Parent::Unit(d(0xad)), Parent::Root, Parent::CLinkage, Parent::Builtin] {
        out.push((one.clone(), Key { parent, ..f_key() }, None));
    }
    for name in [id("F"), id("ff"), Name::UnnamedBy(Box::new(id("f"))), Name::Operator(1)] {
        out.push((one.clone(), Key { name, ..f_key() }, None));
    }
    for disc in [None, Some(d(0xd2))] {
        out.push((one.clone(), Key { disc, ..f_key() }, None));
    }
    out.push((RuleTable::new(vec![Rule { disc: None, ..f_rule() }]).unwrap(), f_key(), None));
    // The key a rule produces is not matched by it.
    out.push((one.clone(), one.map(f_key()), None));
    let p = Parent::Unit(d(0xac));
    let swap = RuleTable::new(vec![
        rule(p, KeyKind::Function, id("f"), None, (p, id("g"))),
        rule(p, KeyKind::Function, id("g"), None, (p, id("f"))),
    ])
    .unwrap();
    // Sorted by bytes: `f` (0x66) before `g` (0x67).
    out.push((swap.clone(), key(KeyKind::Function, p, id("f"), None), Some(0)));
    out.push((swap.clone(), key(KeyKind::Function, p, id("g"), None), Some(1)));
    let t = RuleTable::new(table_rules()).unwrap();
    for (i, r) in t.rules().iter().enumerate() {
        out.push((t.clone(), key(r.kind, r.parent, r.name.clone(), r.disc), Some(i)));
        // Another rule may match it (`aa` and `b` swap): checked against the rules.
        out.push((t.clone(), key(r.kind, r.to_parent, r.to_name.clone(), r.disc), None));
    }
    out
}

// verify §9.5: `map_traced` gives `map`'s key in every case of the rule
// tests, and names the rule that matched the key's own fields.
#[test]
fn map_traced_agrees_with_map() {
    for (t, k, want) in traced_cases() {
        let (mapped, rule) = t.map_traced(k.clone());
        assert_eq!(mapped, t.map(k.clone()), "{k:?}");
        // A key that is a produced key of `table_rules` may also be matched
        // by another rule of it (`aa` and `b` swap): recompute from the rules.
        let matched = t.rules().iter().position(|r| {
            (r.parent, r.kind, &r.name, r.disc) == (k.parent, k.kind, &k.name, k.disc)
        });
        assert_eq!(rule, matched, "{k:?}");
        if want.is_some() {
            assert_eq!(rule, want, "{k:?}");
        }
        match rule {
            Some(i) => {
                let r = &t.rules()[i];
                assert_eq!(mapped, key(r.kind, r.to_parent, r.to_name.clone(), r.disc));
            }
            None => assert_eq!(mapped, k),
        }
    }
}

// verify §9.1: `produces` names the rule whose (to_parent, kind, to_name,
// disc) is the key; `producers` every one, ascending.
#[test]
fn produces_and_producers() {
    let t = RuleTable::new(table_rules()).unwrap();
    for (i, r) in t.rules().iter().enumerate() {
        let produced = key(r.kind, r.to_parent, r.to_name.clone(), r.disc);
        let all: Vec<usize> = t.producers(&produced).collect();
        let want: Vec<usize> = t
            .rules()
            .iter()
            .enumerate()
            .filter(|(_, x)| {
                (x.to_parent, x.kind, &x.to_name, x.disc)
                    == (produced.parent, produced.kind, &produced.name, produced.disc)
            })
            .map(|(j, _)| j)
            .collect();
        assert_eq!(all, want);
        assert!(all.contains(&i));
        assert_eq!(t.produces(&produced), want.first().copied());
    }
    // The matched key is not a produced key unless a rule produces it.
    assert_eq!(t.produces(&f_key()), None);
    let one = RuleTable::new(vec![f_rule()]).unwrap();
    let g = key(KeyKind::Function, Parent::Unit(d(0x1e)), id("g"), Some(d(0xd1)));
    assert_eq!(one.produces(&g), Some(0));
    for other in [
        Key { disc: None, ..g.clone() },
        Key { kind: KeyKind::Variable, ..g.clone() },
        Key { parent: Parent::Root, ..g.clone() },
        Key { name: id("f"), ..g.clone() },
    ] {
        assert_eq!(one.produces(&other), None, "{other:?}");
    }
    assert_eq!(RuleTable::empty().produces(&g), None);
    // Two namespaces merged onto a third: both rules produce its key.
    let ns = |n: &str| key(KeyKind::Namespace, Parent::Root, id(n), None);
    let merge = RuleTable::new(vec![
        rule(Parent::Root, KeyKind::Namespace, id("b"), None, (Parent::Root, id("legacy"))),
        rule(Parent::Root, KeyKind::Namespace, id("a"), None, (Parent::Root, id("legacy"))),
    ])
    .unwrap();
    assert_eq!(merge.producers(&ns("legacy")).collect::<Vec<_>>(), [0, 1]);
    assert_eq!(merge.produces(&ns("legacy")), Some(0));
    assert_eq!(merge.producers(&ns("a")).count(), 0);
}

/// Every rule set of the rule tests above, in the order given.
fn rule_sets() -> Vec<Vec<Rule>> {
    let p = Parent::Unit(d(0xac));
    vec![
        Vec::new(),
        vec![f_rule()],
        vec![Rule { disc: None, ..f_rule() }],
        vec![
            rule(p, KeyKind::Function, id("g"), None, (p, id("f"))),
            rule(p, KeyKind::Function, id("f"), None, (p, id("g"))),
        ],
        table_rules(),
        {
            let mut r = table_rules();
            r.reverse();
            r
        },
        vec![
            rule(Parent::Root, KeyKind::Namespace, id("b"), None, (Parent::Root, id("legacy"))),
            rule(Parent::Root, KeyKind::Namespace, id("a"), None, (Parent::Root, id("legacy"))),
        ],
    ]
}

/// The rules of `t` whose `(to_parent, kind, to_name, disc)` is `k`'s, by a
/// scan: what `producers` must give.
fn producers_by_scan(t: &RuleTable, k: &Key) -> Vec<usize> {
    t.rules()
        .iter()
        .enumerate()
        .filter(|(_, r)| {
            (r.to_parent, r.kind, &r.to_name, r.disc) == (k.parent, k.kind, &k.name, k.disc)
        })
        .map(|(i, _)| i)
        .collect()
}

// verify §9.1, §9.5: a run's table is built from a declared change
// (`Declared::new`) or read from a container (binary or JSON), never by
// `RuleTable::new`. Each gives the same `producers` for every key the rules
// produce, and the same `map_traced` for every key they match and produce,
// as `RuleTable::new` and as a scan of the rules (WP-069 review, F1).
#[test]
fn declared_and_read_tables_index_as_new() {
    use slip_model::declared::Declared;
    for rules in rule_sets() {
        let new = RuleTable::new(rules.clone()).unwrap();
        let declared = Declared::new(new.rules().to_vec(), vec![], vec![], vec![]).unwrap();
        let binary = Declared::from_bytes(&declared.to_bytes()).unwrap();
        let json = Declared::from_json(&declared.to_json()).unwrap();
        let tables =
            [("declared", declared.rules()), ("binary", binary.rules()), ("json", json.rules())];
        let mut subjects: Vec<Key> = Vec::new();
        for r in new.rules() {
            subjects.push(key(r.kind, r.parent, r.name.clone(), r.disc));
            subjects.push(key(r.kind, r.to_parent, r.to_name.clone(), r.disc));
        }
        subjects.push(f_key());
        for k in &subjects {
            let want = producers_by_scan(&new, k);
            assert_eq!(new.producers(k).collect::<Vec<_>>(), want, "new {k:?}");
            for (how, t) in tables {
                assert_eq!(t.rules(), new.rules(), "{how}");
                assert_eq!(t.producers(k).collect::<Vec<_>>(), want, "{how} {k:?}");
                assert_eq!(t.map_traced(k.clone()), new.map_traced(k.clone()), "{how} {k:?}");
            }
        }
        for (how, t) in tables {
            assert_eq!(t, &new, "{how}");
        }
    }
}
