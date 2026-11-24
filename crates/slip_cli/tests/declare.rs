//! Declared changes by name (`spec/verify.md` §2.3, "Tests"): the text,
//! what it needs, names resolved in hand-written records (every error, its
//! line, the order of errors), and, over parsed snippets, the loop the
//! command runs: records of both images, `resolve`, the post-image's
//! records under the rules, `finish`, then the post-image digested under
//! the declared rules and compared with the pre-image under the declared
//! exemptions, which finds nothing (where without them it finds the
//! change).
//!
//! The question this file answers for the trusted base: a name resolves to
//! the key the author meant or to an error, never to another key: a name
//! two keys have is `Ambiguous`, a missing one `NoUnit`, a fallback key
//! `Fallback`, and a rule is what verification then checks.

use slip_cli::declare::{
    Entity, ErrorKind, Line, LineError, Needs, Resolved, finish, needs, read, resolve, resolve_with,
};
use slip_digest::records::{Facts, Record, facts, records};
use slip_digest::{Detail, Input, Options, Output};
use slip_model::Digest;
use slip_model::declared::Declared;
use slip_model::digests::{Group, GroupClass};
use slip_model::keys::{Key, KeyKind, Name, Parent, Rule, RuleTable};
use slip_model::verdict::FindingKind;
use slip_verify::{Exemptions, compare_groups};

fn ident(s: &str) -> Name {
    Name::Identifier(s.as_bytes().to_vec())
}

fn ent(kind: Option<KeyKind>, text: &str) -> Entity {
    Entity { kind, text: text.to_owned() }
}

// ---------------------------------------------------------------------------
// The text

// §2.3 "Format": every line form, comments, blank lines, a kind, `=`, the
// global namespace, CRLF ends of line, blanks around the parts.
#[test]
fn read_every_line_form() {
    let text = "# a comment\n\n  \t\nrelocate acme::inner => :: inner\r\nrelocate [function] n::f(int) => n :: g\n\
                relocate C::m(int) =>  D::=\nchanged a\ncreated  [class]  ns::T<int>\nremoved b::c()\n   # indented comment\n";
    let lines = read(text).unwrap();
    let want = vec![
        (
            4,
            Line::Relocate {
                entity: ent(None, "acme::inner"),
                parent: None,
                name: Some("inner".into()),
            },
        ),
        (
            5,
            Line::Relocate {
                entity: ent(Some(KeyKind::Function), "n::f(int)"),
                parent: Some(ent(None, "n")),
                name: Some("g".into()),
            },
        ),
        (
            6,
            Line::Relocate {
                entity: ent(None, "C::m(int)"),
                parent: Some(ent(None, "D")),
                name: None,
            },
        ),
        (7, Line::Changed(ent(None, "a"))),
        (8, Line::Created(ent(Some(KeyKind::Class), "ns::T<int>"))),
        (9, Line::Removed(ent(None, "b::c()"))),
    ];
    assert_eq!(lines, want);
    // WP-111: `kept`.
    assert_eq!(
        read("kept  [class] a::B ").unwrap(),
        [(1, Line::Kept(ent(Some(KeyKind::Class), "a::B")))]
    );
    assert_eq!(read("kept").unwrap_err().kind, ErrorKind::Syntax);
    assert_eq!(read("").unwrap(), vec![]);
    // A parent with `::` in it: the name is after the last one.
    let l = read("relocate x => a::b :: y").unwrap();
    assert_eq!(
        l[0].1,
        Line::Relocate {
            entity: ent(None, "x"),
            parent: Some(ent(None, "a::b")),
            name: Some("y".into())
        }
    );
    // Every kind's name is accepted.
    for k in KeyKind::ALL {
        let l = read(&format!("changed [{}] x", k.name())).unwrap();
        assert_eq!(l[0].1, Line::Changed(ent(Some(k), "x")));
    }
}

// §2.3: every `Syntax` error, with its line; the first one is returned.
#[test]
fn read_syntax_errors() {
    let bad = [
        "rename x",
        "changed",
        "changed   ",
        "relocate",
        "relocate x",
        "relocate x => y",
        "relocate x => :: 1a",
        "relocate x => :: a b",
        "relocate x => :: ",
        "relocate  => :: a",
        "changed [nosuchkind] x",
        "changed [class x",
        "changed [class]",
        "relocate x => [nosuchkind] y :: z",
        "changedx",
    ];
    for b in bad {
        let e = read(&format!("# ok\nchanged a\n{b}\nremoved [nosuchkind] z\n")).unwrap_err();
        assert_eq!((e.line, e.kind), (3, ErrorKind::Syntax), "{b}: {e}");
        assert!(!e.detail.is_empty());
    }
}

// §2.3 the table of parses: `post` for a relocate with a parent, `post_mapped`
// for a `created` line.
#[test]
fn what_lines_need() {
    let n = |t: &str| needs(&read(t).unwrap());
    assert_eq!(n(""), Needs { post: false, post_mapped: false });
    assert_eq!(
        n("relocate a => :: b\nchanged c\nremoved d"),
        Needs { post: false, post_mapped: false }
    );
    assert_eq!(n("relocate a => x :: b"), Needs { post: true, post_mapped: false });
    assert_eq!(n("created a"), Needs { post: false, post_mapped: true });
    assert_eq!(n("created a\nrelocate a => x :: ="), Needs { post: true, post_mapped: true });
    assert_eq!(n("kept a"), Needs { post: false, post_mapped: true });
}

// ---------------------------------------------------------------------------
// Resolution in hand-written records

fn rec(key: &Key, name: &str) -> Record {
    Record {
        id: key.id(),
        key: Some(key.clone()),
        kind: key.kind,
        class: GroupClass::Declared,
        name: name.to_owned(),
        indeterminate: false,
    }
}

fn sorted(mut v: Vec<Record>) -> Vec<Record> {
    v.sort_by(|a, b| (a.id, &a.name).cmp(&(b.id, &b.name)));
    v
}

fn key(kind: KeyKind, parent: Parent, name: Name, disc: Option<Digest>) -> Key {
    Key { kind, parent, name, disc }
}

/// A pre-image: namespaces `n` and `m`, `n::f(int)` (a function and,
/// with the same name, a variable template of another key, as a stand-in
/// for two entities of one name), `n::f(char)`, `n::C` with an operator,
/// a fallback key, and `<order>`.
struct Pre {
    recs: Vec<Record>,
    n: Key,
    f_int: Key,
    f_char: Key,
    c: Key,
}

fn pre() -> Pre {
    let n = key(KeyKind::Namespace, Parent::Root, ident("n"), None);
    let m = key(KeyKind::Namespace, Parent::Root, ident("m"), None);
    let f_int = key(KeyKind::Function, Parent::Unit(n.id()), ident("f"), Some(Digest([1; 32])));
    let f_int_v = key(KeyKind::VariableTemplate, Parent::Unit(n.id()), ident("f"), None);
    let f_char = key(KeyKind::Function, Parent::Unit(n.id()), ident("f"), Some(Digest([2; 32])));
    let c = key(KeyKind::Class, Parent::Unit(n.id()), ident("C"), None);
    let op = key(KeyKind::Function, Parent::Unit(c.id()), Name::Operator(7), Some(Digest([3; 32])));
    let order = key(KeyKind::Order, Parent::Root, Name::Order, None);
    let fallback = Record {
        id: Digest([9; 32]),
        key: None,
        kind: KeyKind::Friend,
        class: GroupClass::Declared,
        name: "n::C::<friend>".into(),
        indeterminate: true,
    };
    let recs = sorted(vec![
        rec(&n, "n"),
        rec(&m, "m"),
        rec(&f_int, "n::f(int)"),
        rec(&f_int_v, "n::f(int)"),
        rec(&f_char, "n::f(char)"),
        rec(&c, "n::C"),
        rec(&op, "n::C::operator+(n::C)"),
        rec(&order, "<order>"),
        fallback,
    ]);
    Pre { recs, n, f_int, f_char, c }
}

fn err(text: &str, pre: &[Record], post: &[Record]) -> LineError {
    resolve(&read(text).unwrap(), pre, post).unwrap_err()
}

// §2.3: a relocate to the global namespace, with a new name and with `=`;
// the rule matches the new place and gives back the key.
#[test]
fn relocate_to_the_global_namespace() {
    let p = pre();
    let r = resolve(
        &read("relocate n::C => :: D\nrelocate [function] n::f(char) => :: =").unwrap(),
        &p.recs,
        &[],
    )
    .unwrap();
    let want = vec![
        Rule {
            parent: Parent::Root,
            kind: KeyKind::Class,
            name: ident("D"),
            disc: None,
            to_parent: p.c.parent,
            to_name: p.c.name.clone(),
        },
        Rule {
            parent: Parent::Root,
            kind: KeyKind::Function,
            name: ident("f"),
            disc: p.f_char.disc,
            to_parent: Parent::Unit(p.n.id()),
            to_name: ident("f"),
        },
    ];
    assert_eq!(r.rules, RuleTable::new(want).unwrap());
    assert!(r.changed.is_empty() && r.removed.is_empty());
    // The rule gives back the key: the new place, mapped, is the old key.
    let moved = key(KeyKind::Class, Parent::Root, ident("D"), None);
    assert_eq!(r.rules.map(moved), p.c);
}

// §2.3 `m(P')`: a parent in the post-image, mapped by the rules of the lines
// above: `n` relocated to `fresh`, then `n::C` into `fresh`: the rule's
// parent is `n`'s key (what `fresh` maps to), so the post-image's
// `fresh::C` maps to `n::C`.
#[test]
fn a_parent_mapped_by_the_lines_above() {
    let p = pre();
    let fresh = key(KeyKind::Namespace, Parent::Root, ident("fresh"), None);
    let post = sorted(vec![rec(&fresh, "fresh")]);
    // `n::C => fresh :: =` after the first line would change no key (it
    // moves with `n`): an identity rule, refused (WP-111 review F1); so the
    // class is renamed too.
    let r = resolve(
        &read("relocate n => :: fresh\nrelocate n::C => fresh :: D").unwrap(),
        &p.recs,
        &post,
    )
    .unwrap();
    let rules = r.rules.rules();
    assert_eq!(rules.len(), 2);
    let second = rules.iter().find(|x| x.kind == KeyKind::Class).unwrap();
    assert_eq!(second.parent, Parent::Unit(p.n.id()), "the parent mapped: fresh is n");
    // `fresh::D` of the post-image, its parent mapped first, is `n::C`.
    let fresh_mapped = r.rules.map(fresh.clone());
    assert_eq!(fresh_mapped, p.n);
    let c_post = key(KeyKind::Class, Parent::Unit(fresh_mapped.id()), ident("D"), None);
    assert_eq!(r.rules.map(c_post), p.c);
    // WP-111: the other order is refused (`Order`), at the later line,
    // naming the earlier: the first line's parent would not be mapped and
    // its rule would match nothing.
    let e = err("relocate n::C => fresh :: D\nrelocate n => :: fresh", &p.recs, &post);
    assert_eq!((e.line, e.kind), (2, ErrorKind::Order), "{e}");
    assert_eq!(
        e.detail,
        "the rule of `n` relocates `fresh`, or a namespace or class around it, which line 1 \
         resolved as a new parent without it: this line must come before line 1"
    );
}

// `m` up a chain: a parent whose own parent is relocated, two levels up;
// and a parent whose parent's record is missing is kept as it is.
#[test]
fn a_parent_mapped_up_its_chain() {
    let p = pre();
    let top = key(KeyKind::Namespace, Parent::Root, ident("top"), None);
    let mid = key(KeyKind::Namespace, Parent::Unit(top.id()), ident("mid"), None);
    let post = sorted(vec![rec(&top, "top"), rec(&mid, "top::mid")]);
    let r = resolve(
        &read("relocate m => :: top\nrelocate n::C => top::mid :: =").unwrap(),
        &p.recs,
        &post,
    )
    .unwrap();
    let c_rule = r.rules.rules().iter().find(|x| x.kind == KeyKind::Class).unwrap();
    let m = key(KeyKind::Namespace, Parent::Root, ident("m"), None);
    let mid_mapped = key(KeyKind::Namespace, Parent::Unit(m.id()), ident("mid"), None);
    assert_eq!(c_rule.parent, Parent::Unit(mid_mapped.id()));
    // `top` missing from the post records: `mid` is kept unmapped.
    let post = sorted(vec![rec(&mid, "top::mid")]);
    let r = resolve(
        &read("relocate m => :: top\nrelocate n::C => top::mid :: =").unwrap(),
        &p.recs,
        &post,
    )
    .unwrap();
    let c_rule = r.rules.rules().iter().find(|x| x.kind == KeyKind::Class).unwrap();
    assert_eq!(c_rule.parent, Parent::Unit(mid.id()));
}

// §2.3 `changed`, `removed` resolve in the pre-image; the lists are sorted.
#[test]
fn exemptions_resolve_in_the_pre_image() {
    let p = pre();
    let r = resolve(
        &read("changed n::C\nchanged [function] n::f(char)\nremoved n::C\ncreated nothing")
            .unwrap(),
        &p.recs,
        &[],
    )
    .unwrap();
    let mut changed = vec![p.c.id(), p.f_char.id()];
    changed.sort_unstable();
    assert_eq!(
        r,
        Resolved { rules: RuleTable::empty(), changed, removed: vec![p.c.id()], unwritten: vec![] }
    );
}

// §2.3 `NoUnit`: nothing of that name (or that kind), with the names that
// contain its last component.
#[test]
fn no_unit() {
    let p = pre();
    let e = err("changed n::g(int)", &p.recs, &[]);
    assert_eq!((e.line, e.kind), (1, ErrorKind::NoUnit));
    assert!(e.detail.contains("no unit's name contains `g`"), "{e}");
    let e = err("changed n::f(long)", &p.recs, &[]);
    assert_eq!(e.kind, ErrorKind::NoUnit);
    assert!(e.detail.contains("`n::f(int)`") && e.detail.contains("`n::f(char)`"), "{e}");
    // A kind no unit of that name has.
    let e = err("changed [class] n::f(char)", &p.recs, &[]);
    assert_eq!(e.kind, ErrorKind::NoUnit);
    assert!(e.detail.contains("[class] n::f(char)"), "{e}");
    // The parent of a relocate is looked for in the post-image.
    let e = err("relocate n::C => n :: =", &p.recs, &[]);
    assert_eq!(e.kind, ErrorKind::NoUnit);
    assert!(e.detail.contains("post-images"), "{e}");
    // Names are taken literally, but for the blanks around them.
    assert!(resolve(&read("changed  n::C \t").unwrap(), &p.recs, &[]).is_ok());
    assert_eq!(err("changed n:: C", &p.recs, &[]).kind, ErrorKind::NoUnit);
    // At most eight suggestions.
    let many: Vec<Record> = sorted(
        (0..20)
            .map(|i| {
                let k = key(KeyKind::Variable, Parent::Root, ident(&format!("v{i}")), None);
                rec(&k, &format!("v{i}"))
            })
            .collect(),
    );
    let e = err("changed v", &many, &[]);
    assert_eq!(e.detail.matches("`v").count(), 2 + 8, "the name twice and eight: {e}");
    assert!(e.detail.ends_with('…'));
}

// §2.3 `Ambiguous`: one name, two keys; a kind tells them apart.
#[test]
fn ambiguous_unless_a_kind_tells_apart() {
    let p = pre();
    let e = err("changed n::f(int)", &p.recs, &[]);
    assert_eq!((e.line, e.kind), (1, ErrorKind::Ambiguous));
    assert!(e.detail.contains("function") && e.detail.contains("variable_template"), "{e}");
    let r = resolve(&read("changed [function] n::f(int)").unwrap(), &p.recs, &[]).unwrap();
    assert_eq!(r.changed, vec![p.f_int.id()]);
    // Two records of one key (two names of one unit cannot be: one key with
    // two kinds is a collision, and both are the same key): not ambiguous.
    let k = key(KeyKind::Variable, Parent::Root, ident("x"), None);
    let mut other = rec(&k, "x");
    other.class = GroupClass::Tu;
    let two = sorted(vec![rec(&k, "x"), other]);
    assert_eq!(resolve(&read("changed x").unwrap(), &two, &[]).unwrap().changed, vec![k.id()]);
}

// §2.3 `Fallback`: a unit with a fallback key cannot be named.
#[test]
fn a_fallback_key_cannot_be_named() {
    let p = pre();
    for text in [
        "changed n::C::<friend>",
        "relocate n::C::<friend> => :: =",
        "removed [friend] n::C::<friend>",
    ] {
        let e = err(text, &p.recs, &[]);
        assert_eq!(e.kind, ErrorKind::Fallback, "{text}: {e}");
    }
    // One record of a key with a Key and one without (a collision): still
    // a fallback.
    let k = key(KeyKind::Variable, Parent::Root, ident("x"), None);
    let mut none = rec(&k, "x");
    none.key = None;
    none.class = GroupClass::Tu;
    let e = err("changed x", &sorted(vec![rec(&k, "x"), none]), &[]);
    assert_eq!(e.kind, ErrorKind::Fallback);
}

// §2.3 `NotRenamable`: a new identifier for a name that is not one; `=` is
// accepted.
#[test]
fn not_renamable() {
    let p = pre();
    let e = err("relocate n::C::operator+(n::C) => :: plus", &p.recs, &[]);
    assert_eq!((e.line, e.kind), (1, ErrorKind::NotRenamable));
    let r =
        resolve(&read("relocate n::C::operator+(n::C) => :: =").unwrap(), &p.recs, &[]).unwrap();
    assert_eq!(r.rules.rules()[0].name, Name::Operator(7));
    assert_eq!(r.rules.rules()[0].to_name, Name::Operator(7));
}

// §2.3 `Refused`: a `relocate` line whose rule `Rule::refusal` refuses
// (spec/verify.md §2.1), one per refused kind a `relocate` line can make.
// A namespace rule across the unnamed namespace cannot be made by one: the
// unnamed namespace's name is not an identifier, so renaming it is
// `NotRenamable`, and no identifier names it.
#[test]
fn refused_rules() {
    let w = key(KeyKind::ClassTemplate, Parent::Root, ident("W"), Some(Digest([1; 32])));
    let f = key(KeyKind::Function, Parent::Root, ident("f"), Some(Digest([2; 32])));
    let at = |kind, name| key(kind, Parent::Unit(f.id()), name, None);
    let spec =
        key(KeyKind::Specialisation, Parent::Unit(w.id()), ident("W"), Some(Digest([3; 32])));
    let partial = key(
        KeyKind::PartialSpecialisation,
        Parent::Unit(w.id()),
        ident("W"),
        Some(Digest([4; 32])),
    );
    let recs = sorted(vec![
        rec(&w, "W"),
        rec(&f, "f()"),
        rec(&spec, "W<int>"),
        rec(&partial, "W<T *>"),
        rec(&at(KeyKind::Resolved, Name::Property(1)), "f()::<resolved>"),
        rec(&at(KeyKind::DefaultArgument, Name::Parameter(0)), "f()::<default argument 0>"),
    ]);
    for (text, what) in [
        ("relocate W<int> => :: V", "a rule may not match a specialisation"),
        ("relocate W<T *> => :: =", "a rule may not match a specialisation"),
        ("relocate f()::<resolved> => :: =", "a rule may not match a resolved"),
        ("relocate f()::<default argument 0> => :: =", "a rule may not match a resolved"),
    ] {
        let e = err(&format!("# x\n{text}"), &recs, &[]);
        assert_eq!((e.line, e.kind), (2, ErrorKind::Refused), "{text}: {e}");
        assert!(e.detail.contains(what), "{text}: {e}");
    }
    let anon = key(KeyKind::Namespace, Parent::Root, Name::Anonymous, None);
    let recs = sorted(vec![rec(&anon, "(anonymous namespace)")]);
    let e = err("relocate (anonymous namespace) => :: n", &recs, &[]);
    assert_eq!(e.kind, ErrorKind::NotRenamable, "{e}");
    // Relocated to where it is: a rule that changes nothing, refused
    // (WP-111 review, F1).
    let e = err("relocate (anonymous namespace) => :: =", &recs, &[]);
    assert_eq!(e.kind, ErrorKind::Refused, "{e}");
}

// §2.3 `Duplicate`: two relocates whose rules match the same; a key twice
// in one list (at the later line); in two lists is allowed.
#[test]
fn duplicates() {
    let p = pre();
    let e = err("relocate n::C => :: D\n# x\nrelocate n::C => :: D", &p.recs, &[]);
    assert_eq!((e.line, e.kind), (3, ErrorKind::Duplicate));
    // One entity moved twice to one place (two entities: below).
    let e = err(
        "relocate [function] n::f(char) => :: g\nrelocate [function] n::f(char) => :: g",
        &p.recs,
        &[],
    );
    assert_eq!((e.line, e.kind), (2, ErrorKind::Duplicate));
    let e = err("changed n::C\nremoved n::C\nchanged n::C", &p.recs, &[]);
    assert_eq!((e.line, e.kind), (3, ErrorKind::Duplicate));
    let e = err("removed n::C\nremoved n::C", &p.recs, &[]);
    assert_eq!((e.line, e.kind), (2, ErrorKind::Duplicate));
    // The same entity relocated to two places: two rules would produce
    // its key, which no table holds (WP-111 review, F1): the later line is
    // the error, naming the earlier.
    let e = err("relocate n::C => :: D\n\nrelocate n::C => :: E", &p.recs, &[]);
    assert_eq!((e.line, e.kind), (3, ErrorKind::Duplicate), "{e}");
    assert!(e.detail.contains("the rule of line 1 produces"), "{e}");
    // A namespace split in two by two namespace rules, likewise.
    let e = err("relocate n => :: x\nrelocate n => :: y", &p.recs, &[]);
    assert_eq!((e.line, e.kind), (2, ErrorKind::Duplicate), "{e}");
    assert!(e.detail.contains("line 1"), "{e}");
    // A rule that changes nothing is refused (§2.1).
    let e = err("changed n::C\nrelocate n::C => n :: =", &p.recs, &sorted(vec![rec(&p.n, "n")]));
    assert_eq!((e.line, e.kind), (2, ErrorKind::Refused), "{e}");
}

// §2.3: the first error in file order, whatever its kind.
#[test]
fn the_first_error_in_file_order() {
    let p = pre();
    let text =
        "changed n::C\nchanged n::f(int)\nchanged nothing\nchanged n::C::<friend>\nchanged n::C\n";
    let e = err(text, &p.recs, &[]);
    assert_eq!((e.line, e.kind), (2, ErrorKind::Ambiguous));
    let text = "changed n::C\nchanged nothing\nchanged n::f(int)\n";
    assert_eq!(err(text, &p.recs, &[]).line, 2);
    // `created` lines are not looked at by resolve.
    assert!(resolve(&read("created nothing\nchanged n::C").unwrap(), &p.recs, &[]).is_ok());
    // A `Display` with the line.
    assert!(err("changed nothing", &p.recs, &[]).to_string().starts_with("line 1: no unit: "));
}

// §2.3 `created` resolves in `post_mapped` (the post-image under the
// rules), in `finish`; the `Declared` has every list sorted.
#[test]
fn finish_resolves_created_in_the_mapped_post_image() {
    let p = pre();
    let lines =
        read("relocate n::C => :: D\nchanged n::C\ncreated D::fresh\ncreated D::other\n").unwrap();
    let r = resolve(&lines, &p.recs, &[]).unwrap();
    let rules = r.rules.clone();
    let fresh = key(KeyKind::Variable, Parent::Unit(p.c.id()), ident("fresh"), None);
    let other = key(KeyKind::Variable, Parent::Unit(p.c.id()), ident("other"), None);
    let mapped = sorted(vec![rec(&fresh, "D::fresh"), rec(&other, "D::other")]);
    let d = finish(&lines, r.clone(), &mapped).unwrap();
    let mut created = vec![fresh.id(), other.id()];
    created.sort_unstable();
    assert_eq!(d, Declared::new(rules.rules().to_vec(), vec![p.c.id()], created, vec![]).unwrap());
    // Its errors, at their lines.
    let e = finish(&lines, r.clone(), &mapped[..1]).unwrap_err();
    assert_eq!(e.kind, ErrorKind::NoUnit);
    assert_eq!(e.line, if mapped[0].name == "D::fresh" { 4 } else { 3 });
    let twice = read("created D::fresh\ncreated D::fresh").unwrap();
    assert_eq!(finish(&twice, r, &mapped).unwrap_err().line, 2);
}

// WP-111 item 4: `kept` resolves as `created` does, in the post-images
// under the file's rules (`post_mapped`), into `Declared.kept`, sorted; a
// name no unit has is `NoUnit` and a second line for one key `Duplicate`,
// at its line; a key both `created` and `kept` is in both lists.
#[test]
fn s2_3_kept_resolves_in_the_mapped_post_image() {
    let p = pre();
    let lines = read(
        "relocate n => :: fresh\nkept fresh::stay\ncreated fresh::add\nkept fresh::other\nkept fresh::add\n",
    )
    .unwrap();
    let r = resolve(&lines, &p.recs, &[]).unwrap();
    let var = |name: &str| key(KeyKind::Variable, Parent::Unit(p.n.id()), ident(name), None);
    let (stay, other, add) = (var("stay"), var("other"), var("add"));
    let mapped = sorted(vec![
        rec(&stay, "fresh::stay"),
        rec(&other, "fresh::other"),
        rec(&add, "fresh::add"),
    ]);
    let d = finish(&lines, r.clone(), &mapped).unwrap();
    let mut kept = vec![stay.id(), other.id(), add.id()];
    kept.sort_unstable();
    assert_eq!(d.kept(), kept);
    assert_eq!(d.created(), [add.id()]);
    let e =
        finish(&read("kept fresh::stay\nkept nothing").unwrap(), r.clone(), &mapped).unwrap_err();
    assert_eq!((e.line, e.kind), (2, ErrorKind::NoUnit), "{e}");
    let e = finish(&read("kept fresh::stay\n\nkept fresh::stay").unwrap(), r, &mapped).unwrap_err();
    assert_eq!((e.line, e.kind), (3, ErrorKind::Duplicate), "{e}");
    assert_eq!(e.detail, "a second `kept` line for `fresh::stay`");
}

// WP-111 item 4, spec/verify.md §2.1 rule 1: a `relocate` line whose entity
// is a dependency entity (the pre-images' `Facts`) and not a namespace is
// `DependencyEntity`, at its line, in file order with the other errors;
// a namespace that is one is accepted (its rule applies in the owned
// view); an exemption of a dependency entity is accepted.
#[test]
fn s2_3_dependency_entity() {
    let p = pre();
    let facts = Facts {
        dependency: [p.c.id(), p.n.id(), p.f_char.id()].into_iter().collect(),
        ..Facts::default()
    };
    let go = |text: &str| resolve_with(&read(text).unwrap(), &p.recs, &[], &facts);
    let e = go("changed n::C\nrelocate n::C => :: D").unwrap_err();
    assert_eq!((e.line, e.kind), (2, ErrorKind::DependencyEntity), "{e}");
    assert!(e.detail.starts_with("`n::C` is a dependency entity"), "{e}");
    let e = go("relocate [function] n::f(char) => n :: g\nchanged nothing").unwrap_err();
    assert_eq!((e.line, e.kind), (1, ErrorKind::DependencyEntity), "{e}");
    assert!(go("relocate n => :: fresh\nchanged n::C\nremoved n::C").is_ok());
    assert!(go("relocate [function] n::f(int) => :: g").is_ok());
    // Without facts (records written by hand), accepted.
    assert!(resolve(&read("relocate n::C => :: D").unwrap(), &p.recs, &[]).is_ok());
}

// §2.3 "An entity with C language linkage" (the coordinator's ruling on
// WP-111): the line is accepted, its parent resolved and checked like any
// other, and the rule keeps the parent `c_linkage`: the two spellings of
// the new parent give one rule and one `rules_id`; a parent that resolves
// to nothing is an error; moving the function between namespaces without
// renaming it gives a rule that maps its key to itself.
#[test]
fn s2_3_c_linkage() {
    let p = pre();
    let c_fn = key(KeyKind::Function, Parent::CLinkage, ident("c_fn"), None);
    let mut recs = p.recs.clone();
    recs.push(rec(&c_fn, "n::c_fn(int)"));
    let post = sorted(vec![rec(&p.n, "n")]);
    let table = |text: &str| resolve(&read(text).unwrap(), &recs, &post).unwrap().rules;
    let want = Rule {
        parent: Parent::CLinkage,
        kind: KeyKind::Function,
        name: ident("c_fn2"),
        disc: None,
        to_parent: Parent::CLinkage,
        to_name: ident("c_fn"),
    };
    let (named, plain) =
        (table("relocate n::c_fn(int) => n :: c_fn2"), table("relocate n::c_fn(int) => :: c_fn2"));
    assert_eq!(named.rules(), std::slice::from_ref(&want));
    assert_eq!((named.rules(), named.id()), (plain.rules(), plain.id()));
    // The parent is resolved in the post-images like any other.
    let e = err("changed n::C\nrelocate n::c_fn(int) => nowhere :: c_fn2", &recs, &post);
    assert_eq!((e.line, e.kind), (2, ErrorKind::NoUnit), "{e}");
    // Moved, not renamed: no key changes, so the line is accepted and
    // writes no rule (WP-111 review, F1: an identity rule is refused, and
    // beside a rename it would hide the rename); `unwritten` names it.
    for text in ["relocate n::c_fn(int) => n :: =", "relocate n::c_fn(int) => :: c_fn"] {
        let r = resolve(&read(&format!("changed n::C\n{text}")).unwrap(), &recs, &post).unwrap();
        assert!(r.rules.is_empty(), "{text}");
        assert_eq!(r.unwritten, [2], "{text}");
    }
    let r = resolve(
        &read("relocate n::c_fn(int) => n :: =\nrelocate n::c_fn(int) => n :: c_fn2").unwrap(),
        &recs,
        &post,
    )
    .unwrap();
    assert_eq!(
        (r.rules.rules(), r.unwritten.as_slice()),
        ([want.clone()].as_slice(), [1].as_slice())
    );
    assert_eq!(r.rules.map(c_fn.clone()), c_fn);
    // Its parent is still resolved.
    let e = err("relocate n::c_fn(int) => nowhere :: =", &recs, &post);
    assert_eq!((e.line, e.kind), (1, ErrorKind::NoUnit), "{e}");
    // A C function's rule looks nothing up: no `Order` with a later line
    // that relocates the namespace it names.
    assert!(
        resolve(
            &read("relocate n::c_fn(int) => n :: c_fn2\nrelocate n => :: m").unwrap(),
            &recs,
            &post
        )
        .is_ok()
    );
}

// The rename of a C function, end to end: verified with the line, a real
// finding without it.
#[test]
fn loop_c_function_renamed() {
    let pre =
        "namespace legacy { extern \"C\" int c_fn(int); }\nint u() { return legacy::c_fn(1); }\n";
    let post =
        "namespace legacy { extern \"C\" int c_fn2(int); }\nint u() { return legacy::c_fn2(1); }\n";
    for text in [
        "relocate legacy::c_fn(int) => legacy :: c_fn2\n",
        "relocate legacy::c_fn(int) => :: c_fn2\n",
    ] {
        let d = verified(pre, post, text);
        assert_eq!(d.rules().rules()[0].parent, Parent::CLinkage, "{text}");
    }
}

// WP-111, "Added after the review of WP-103": a line whose new parent, or a
// namespace or class around it, a later line relocates is refused at the
// later line (`Order`), naming the earlier; in the other order both lines
// are accepted (`a_parent_mapped_by_the_lines_above`,
// `a_parent_mapped_up_its_chain`). A later line that relocates something
// else is accepted.
#[test]
fn s2_3_order_of_lines() {
    let p = pre();
    let top = key(KeyKind::Namespace, Parent::Root, ident("top"), None);
    let mid = key(KeyKind::Namespace, Parent::Unit(top.id()), ident("mid"), None);
    let post = sorted(vec![rec(&top, "top"), rec(&mid, "top::mid")]);
    // An enclosing namespace of the new parent, two lines apart.
    let e =
        err("relocate n::C => top::mid :: =\nchanged n::C\nrelocate m => :: top", &p.recs, &post);
    assert_eq!((e.line, e.kind), (3, ErrorKind::Order), "{e}");
    assert!(e.detail.contains("`top::mid`") && e.detail.contains("before line 1"), "{e}");
    // The parent itself.
    let e = err("relocate n::C => top::mid :: =\nrelocate m => top :: mid", &p.recs, &post);
    assert_eq!((e.line, e.kind), (2, ErrorKind::Order), "{e}");
    for ok in [
        "relocate m => :: top\nrelocate n::C => top::mid :: =",
        "relocate n::C => top::mid :: =\nrelocate [function] n::f(int) => top :: g",
        "relocate n::C => top::mid :: =\nrelocate n => :: top2",
    ] {
        assert!(resolve(&read(ok).unwrap(), &p.recs, &post).is_ok(), "{ok}");
    }
}

/// What [`every_order`] resolves each order with.
type Resolver<'a> = dyn Fn(&[(u64, Line)]) -> Result<RuleTable, LineError> + 'a;

/// Every order of `lines` (renumbered from 1): each is either refused with
/// `Order` or gives `want`'s table; at least one is accepted.
fn every_order(lines: &[(u64, Line)], go: &Resolver) -> Digest {
    let n = lines.len();
    assert!(n <= 6, "{n} lines: too many orders");
    let mut idx: Vec<usize> = (0..n).collect();
    let mut ids = std::collections::BTreeSet::new();
    let (mut accepted, mut refused) = (0, 0);
    // Heap's algorithm, iterative.
    let mut c = vec![0usize; n];
    let mut visit = |idx: &[usize]| {
        let order: Vec<(u64, Line)> =
            idx.iter().enumerate().map(|(i, &j)| (i as u64 + 1, lines[j].1.clone())).collect();
        match go(&order) {
            Ok(t) => {
                ids.insert(t.id());
                accepted += 1;
            }
            Err(e) => {
                assert_eq!(e.kind, ErrorKind::Order, "{order:?}: {e}");
                refused += 1;
            }
        }
    };
    visit(&idx);
    let mut i = 0;
    while i < n {
        if c[i] < i {
            if i % 2 == 0 {
                idx.swap(0, i);
            } else {
                idx.swap(c[i], i);
            }
            visit(&idx);
            c[i] += 1;
            i = 0;
        } else {
            c[i] = 0;
            i += 1;
        }
    }
    assert!(accepted > 0, "no order accepted");
    assert_eq!(ids.len(), 1, "{accepted} accepted orders, {refused} refused, {} tables", ids.len());
    ids.into_iter().next().unwrap()
}

// WP-111, "Added after the review of WP-103", and spec/verify.md §2.3: every
// order of a file's lines that `declare` accepts gives one rule table (one
// `rules_id`), the others are `Order`. Over hand-written records; the
// rules suite's cases are permuted in `the_rules_suite_resolves_alike`.
#[test]
fn s2_3_every_accepted_order_gives_one_table() {
    let p = pre();
    let fresh = key(KeyKind::Namespace, Parent::Root, ident("fresh"), None);
    let deep = key(KeyKind::Namespace, Parent::Unit(fresh.id()), ident("deep"), None);
    let top = key(KeyKind::Namespace, Parent::Root, ident("top"), None);
    let mid = key(KeyKind::Namespace, Parent::Unit(top.id()), ident("mid"), None);
    let post = sorted(vec![
        rec(&fresh, "fresh"),
        rec(&deep, "fresh::deep"),
        rec(&top, "top"),
        rec(&mid, "top::mid"),
    ]);
    for text in [
        "relocate n => :: fresh\nrelocate n::C => fresh::deep :: =\nchanged n::C",
        "relocate m => :: top\nrelocate n::C => top::mid :: =\nrelocate [function] n::f(char) => top::mid :: h\nrelocate n => :: fresh",
        "relocate n => :: fresh\nrelocate m => fresh :: deep\nrelocate n::C => fresh::deep :: D",
        "relocate [function] n::f(int) => top :: g\nrelocate n::C => top::mid :: =\nremoved n::C",
    ] {
        let lines = read(text).unwrap();
        let go = |l: &[(u64, Line)]| resolve(l, &p.recs, &post).map(|r| r.rules);
        let id = every_order(&lines, &go);
        // `rules_id` of the `Declared` is a function of the table.
        let r = resolve(&lines, &p.recs, &post).unwrap_or_else(|e| panic!("{text}: {e}"));
        let d = finish(&lines, r, &[]).unwrap();
        assert_eq!(d.rules().id(), id, "{text}");
    }
}

// ---------------------------------------------------------------------------
// The loop over parsed snippets

/// Parses `source` as `main.cpp` and gives `f` the input on a stack the
/// walk can use.
fn with_input<R: Send>(source: &str, f: impl FnOnce(&Input<'_>) -> R + Send) -> R {
    let files = [("main.cpp", source)];
    let snippet = slip_kiln::snippet::Snippet {
        files: &files,
        main: "main.cpp",
        include: &[""],
        std: "c++20",
    };
    let extra = slip_kiln::snippet::Extra { args: &[], target: None };
    let (r, errors) = slip_kiln::snippet::parse_with(&snippet, &extra, |p| {
        let paths: Vec<Vec<u8>> =
            p.sm.file_ids()
                .map(|f| p.path(f).map(|s: &str| s.as_bytes().to_vec()).unwrap_or_default())
                .collect();
        let (ast, sm) = (p.ast, p.sm);
        let r = std::thread::scope(|scope| {
            std::thread::Builder::new()
                .stack_size(slip_digest::STACK_SIZE)
                .spawn_scoped(scope, || {
                    f(&Input {
                        ast,
                        sm,
                        subject_path: &|file| paths.get(file.index()).cloned().unwrap_or_default(),
                    })
                })
                .expect("a thread for the walk")
                .join()
                .unwrap_or_else(|payload| std::panic::resume_unwind(payload))
        });
        (r, p.errors)
    })
    .expect("the snippet parses");
    assert_eq!(errors, 0, "{source}");
    r
}

fn records_of(source: &str, rules: &RuleTable) -> Vec<Record> {
    with_input(source, |i| records(i, rules, slip_digest::ReadOnly::NO_FILE)).expect("records")
}

fn digest_of(source: &str, rules: &RuleTable) -> Output {
    with_input(source, |i| {
        slip_digest::digest(
            i,
            &Options {
                rules,
                read_only: slip_digest::ReadOnly::NO_FILE,
                detail: Detail::All,
                names: true,
                rule_use: false,
            },
        )
    })
}

fn findings(pre: &Output, post: &Output, ex: &Exemptions) -> Vec<FindingKind> {
    named_findings(pre, post, ex).into_iter().map(|(k, _)| k).collect()
}

/// The findings with the name of their key (in either image).
fn named_findings(pre: &Output, post: &Output, ex: &Exemptions) -> Vec<(FindingKind, String)> {
    let p: Vec<&Group> = pre.digests.groups.iter().collect();
    let q: Vec<&Group> = post.digests.groups.iter().collect();
    let c = compare_groups(&p, &q, ex).expect("groups compare");
    let name = |k: Option<Digest>| {
        let find = |o: &Output| o.names.iter().find(|(n, _)| Some(*n) == k).map(|(_, s)| s.clone());
        find(pre).or_else(|| find(post)).unwrap_or_default()
    };
    c.findings.iter().map(|f| (f.kind, name(f.key))).collect()
}

/// The command's loop: the `Declared` of `text` over `pre` and `post`,
/// then what verification finds with it, and without it.
fn round_trip(pre: &str, post: &str, text: &str) -> (Declared, Vec<FindingKind>, Vec<FindingKind>) {
    let lines = read(text).unwrap();
    let empty = RuleTable::empty();
    let pre_r = records_of(pre, &empty);
    let post_r = if needs(&lines).post { records_of(post, &empty) } else { Vec::new() };
    let resolved = resolve(&lines, &pre_r, &post_r)
        .unwrap_or_else(|e| panic!("{e}\npre {pre_r:#?}\npost {post_r:#?}"));
    let mapped =
        if needs(&lines).post_mapped { records_of(post, &resolved.rules) } else { Vec::new() };
    let declared = finish(&lines, resolved, &mapped).unwrap_or_else(|e| panic!("{e}\n{mapped:#?}"));
    let pre_d = digest_of(pre, &empty);
    let with =
        findings(&pre_d, &digest_of(post, declared.rules()), &Exemptions::from_declared(&declared));
    let without = findings(&pre_d, &digest_of(post, &empty), &Exemptions::none());
    (declared, with, without)
}

fn real(k: FindingKind) -> bool {
    matches!(
        k,
        FindingKind::Changed
            | FindingKind::Removed
            | FindingKind::Added
            | FindingKind::DerivedChanged
    )
}

/// Verified with the declaration, no finding at all; a real finding
/// without it.
fn verified(pre: &str, post: &str, text: &str) -> Declared {
    let (declared, with, without) = round_trip(pre, post, text);
    assert!(with.is_empty(), "with the declaration: {with:?}\n{text}");
    assert!(without.iter().copied().any(real), "without it: {without:?}");
    declared
}

// Function bodies and initialisers are pending areas of the digest
// (indeterminate), so the snippets below refer to the entities moved from
// declarations; `loop_with_bodies` has bodies and leaves only
// indeterminate findings.

// A namespace moved to the global namespace, its old parent kept alive.
#[test]
fn loop_namespace_to_the_global_namespace() {
    let pre = "namespace acme { namespace inner { struct S; void f(S *); } int keep; }\n\
               using T = acme::inner::S;\nvoid g(T *, acme::inner::S *);\n";
    let post = "namespace acme { int keep; }\nnamespace inner { struct S; void f(S *); }\n\
                using T = inner::S;\nvoid g(T *, inner::S *);\n";
    let d = verified(pre, post, "relocate acme::inner => :: inner\n");
    assert_eq!(d.rules().rules().len(), 1);
}

// §2.3, WP-111 review F1: a namespace split in two (`legacy`'s members
// partly to `acme`, partly to `acme2`) is declared per member: it verifies,
// and a member moved to the wrong half is a finding. Two namespace rules
// for it are no table (`duplicates`).
#[test]
fn loop_namespace_split_per_member() {
    let pre = "namespace legacy { struct A; struct B; struct C; }\n\
               void u(legacy::A *, legacy::B *, legacy::C *);\n";
    let post = "namespace acme { struct A; }\nnamespace acme2 { struct B; }\n\
                namespace legacy { struct C; }\n\
                void u(acme::A *, acme2::B *, legacy::C *);\n";
    let text = "relocate legacy::A => acme :: =\nrelocate legacy::B => acme2 :: =\n\
                created acme\ncreated acme2\n";
    let d = verified(pre, post, text);
    assert_eq!(d.rules().rules().len(), 2);
    // The halves swapped: each member is where the other's line says.
    let wrong = "namespace acme { struct B; }\nnamespace acme2 { struct A; }\n\
                 namespace legacy { struct C; }\n\
                 void u(acme2::A *, acme::B *, legacy::C *);\n";
    let (_, with, _) = round_trip(pre, wrong, text);
    assert!(with.iter().copied().any(real), "{with:?}");
}

// A namespace into an existing namespace.
#[test]
fn loop_namespace_into_an_existing_one() {
    let pre = "namespace a { struct X; }\nnamespace b { struct Y; }\nusing Z = a::X;\n";
    let post = "namespace b { struct Y; namespace a { struct X; } }\nusing Z = b::a::X;\n";
    verified(pre, post, "relocate a => b :: a\n");
}

// Into a namespace another line relocates: `old` becomes `fresh`, and
// `x::T` goes into it; `x` stays.
#[test]
fn loop_into_a_relocated_namespace() {
    let pre = "namespace old { struct V; }\nnamespace x { struct T; struct W; }\nusing U = x::T;\nusing R = old::V;\n";
    let post = "namespace fresh { struct V; struct T; }\nnamespace x { struct W; }\nusing U = fresh::T;\nusing R = fresh::V;\n";
    verified(pre, post, "relocate old => :: fresh\nrelocate x::T => fresh :: T\n");
}

// A member function renamed.
#[test]
fn loop_member_function_renamed() {
    let pre = "struct C { void m(int); void k(); };\n";
    let post = "struct C { void n(int); void k(); };\n";
    verified(pre, post, "relocate C::m(int) => C :: n\n");
}

// A class renamed, with a use; its member named under the new parent with
// `=`, which is an identity rule and changes nothing.
#[test]
fn loop_class_renamed() {
    let pre = "struct C { void m(int); };\nvoid use(C *);\n";
    let post = "struct D { void m(int); };\nvoid use(D *);\n";
    verified(pre, post, "relocate C => :: D\n");
    // The member's line beside its class's is a rule that changes nothing
    // (`D::m` maps to `C::m` through the class's rule already): refused
    // (WP-111 review F1; before, accepted as an identity rule).
    let empty = RuleTable::empty();
    let lines = read("relocate C => :: D\nrelocate C::m(int) => D :: =\n").unwrap();
    let e = resolve(&lines, &records_of(pre, &empty), &records_of(post, &empty)).unwrap_err();
    assert_eq!((e.line, e.kind), (2, ErrorKind::Refused), "{e}");
}

// One overload renamed, named with its kind.
#[test]
fn loop_one_overload_renamed() {
    let pre = "namespace n { void f(int); void f(char); }\n";
    let post = "namespace n { void g(int); void f(char); }\n";
    verified(pre, post, "relocate [function] n::f(int) => n :: g\n");
}

// With bodies (pending areas): every real finding is gone, the
// indeterminate ones stay.
#[test]
fn loop_with_bodies() {
    let pre = "struct C { void m(int); };\nvoid use(C c) { c.m(1); }\n";
    let post = "struct D { void n(int); };\nvoid use(D d) { d.n(1); }\n";
    let (_, with, without) =
        round_trip(pre, post, "relocate C => :: D\nrelocate C::m(int) => D :: n\n");
    assert!(!with.iter().copied().any(real), "{with:?}");
    assert!(without.iter().copied().any(real), "{without:?}");
}

// Exemptions: a changed type, a removed and a created variable.
#[test]
fn loop_exemptions() {
    let pre = "int a;\nint b;\nint c;\n";
    let post = "long a;\nint c;\nint d;\n";
    let all = "changed a\nremoved b\ncreated d\nchanged <order>\n";
    let d = verified(pre, post, all);
    assert_eq!((d.changed().len(), d.removed().len(), d.created().len()), (2, 1, 1));
    let (_, _, without) = round_trip(pre, post, all);
    for k in [FindingKind::Changed, FindingKind::Removed, FindingKind::Added] {
        assert!(without.contains(&k), "{without:?}");
    }
    // A wrong exemption leaves the finding.
    for (text, left) in [
        ("changed c\nremoved b\ncreated d\nchanged <order>\n", FindingKind::Changed),
        ("changed a\nremoved c\ncreated d\nchanged <order>\n", FindingKind::Removed),
        ("changed a\nremoved b\nchanged <order>\n", FindingKind::Added),
    ] {
        let (_, with, _) = round_trip(pre, post, text);
        assert!(with.contains(&left), "{text}: {with:?}");
    }
}

// A wrong relocation is not verified: the rule is what verification checks.
#[test]
fn loop_a_wrong_relocation_is_found() {
    let pre = "namespace n { void f(int); void f(char); }\n";
    let post = "namespace n { void g(int); void f(char); }\n";
    let (_, with, _) = round_trip(pre, post, "relocate [function] n::f(char) => n :: g\n");
    assert!(with.iter().copied().any(real), "the wrong overload relocated: {with:?}");
}

// A change of initialisation order alone: `<order>` is changed, and
// `changed <order>` exempts it. (The loops above move namespaces and
// classes and need no `changed <order>`: their order unit is equal.)
#[test]
fn loop_order_of_initialisation() {
    let pre = "int x = 1;\nint y = 2;\n";
    let post = "int y = 2;\nint x = 1;\n";
    let empty = RuleTable::empty();
    let (pre_d, post_d) = (digest_of(pre, &empty), digest_of(post, &empty));
    let without = named_findings(&pre_d, &post_d, &Exemptions::none());
    assert!(without.contains(&(FindingKind::Changed, "<order>".to_owned())), "{without:?}");
    assert!(
        without.iter().all(|(k, n)| !real(*k) || n == "<order>"),
        "only the order changed: {without:?}"
    );
    // The initialisers are a pending area (indeterminate); every real
    // finding is gone with the exemption.
    let (d, with, _) = round_trip(pre, post, "changed <order>\n");
    assert_eq!(d.changed().len(), 1);
    assert!(!with.iter().copied().any(real), "{with:?}");
    assert!(with.iter().all(|k| *k == FindingKind::Indeterminate), "{with:?}");
}

// §2.3 `Duplicate`, two entities to one place: two namespaces relocated to
// one name have one `(parent, kind, name, disc)`, so the later line is the
// error; two overloads relocated to one name have two discriminators and
// are two rules.
#[test]
fn two_entities_relocated_to_one_place() {
    let p = pre();
    let e = err("relocate n => :: z\n\nrelocate m => :: z", &p.recs, &[]);
    assert_eq!((e.line, e.kind), (3, ErrorKind::Duplicate), "{e}");
    let r = resolve(
        &read("relocate [function] n::f(int) => :: g\nrelocate [function] n::f(char) => :: g")
            .unwrap(),
        &p.recs,
        &[],
    )
    .unwrap();
    assert_eq!(r.rules.rules().len(), 2);
}

// `resolve` and `finish` take records in any order: shuffled records give
// what sorted ones give, for every line form and for the errors.
#[test]
fn records_in_any_order() {
    let p = pre();
    let fresh = key(KeyKind::Namespace, Parent::Root, ident("fresh"), None);
    let deep = key(KeyKind::Namespace, Parent::Unit(fresh.id()), ident("deep"), None);
    let post = sorted(vec![rec(&fresh, "fresh"), rec(&deep, "fresh::deep")]);
    let texts = [
        "relocate n => :: fresh\nrelocate n::C => fresh::deep :: =\nchanged [function] n::f(char)\nremoved n::C\n",
        "changed n::f(int)\n",
        "changed nothing\n",
        "relocate n::C::<friend> => :: =\n",
    ];
    let mut rng = 0x5eed_0055_0000_005b_u64;
    let mut shuffle = |v: &mut Vec<Record>| {
        for i in (1..v.len()).rev() {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            v.swap(i, (rng % (i as u64 + 1)) as usize);
        }
    };
    for text in texts {
        let lines = read(text).unwrap();
        let want = resolve(&lines, &p.recs, &post);
        for _ in 0..20 {
            let (mut a, mut b) = (p.recs.clone(), post.clone());
            shuffle(&mut a);
            shuffle(&mut b);
            b.reverse();
            assert_eq!(resolve(&lines, &a, &b), want, "{text}");
        }
    }
    let lines = read("created D::fresh\ncreated D::other\n").unwrap();
    let f = key(KeyKind::Variable, Parent::Root, ident("fresh"), None);
    let o = key(KeyKind::Variable, Parent::Root, ident("other"), None);
    let mut mapped = vec![rec(&f, "D::fresh"), rec(&o, "D::other")];
    let r =
        Resolved { rules: RuleTable::empty(), changed: vec![], removed: vec![], unwritten: vec![] };
    let want = finish(&lines, r.clone(), &mapped).unwrap();
    mapped.reverse();
    assert_eq!(finish(&lines, r, &mapped).unwrap(), want);
}

// §2.3: a byte order mark at the start is skipped; elsewhere it is text.
#[test]
fn a_byte_order_mark_is_skipped() {
    let l = read("\u{feff}changed a\n").unwrap();
    assert_eq!(l, vec![(1, Line::Changed(ent(None, "a")))]);
    let l = read("\u{feff}# comment\r\nremoved b").unwrap();
    assert_eq!(l, vec![(2, Line::Removed(ent(None, "b")))]);
    assert_eq!(read("changed a\n\u{feff}changed b").unwrap_err().line, 2);
}

// ---------------------------------------------------------------------------
// WP-103: `declare` and the resolver of `slip_digest`'s `tests/rules.rs`
// agree

/// The resolver of the rules suite, a second implementation of the same
/// names on purpose (`docs/wp/WP-103.md`).
#[path = "../../slip_digest/tests/common/rules.rs"]
#[allow(dead_code, reason = "only `read` and `table` are used here")]
mod digest_rules;

// For cases of `crates/slip_digest/tests/rules/` whose `rules.txt` has
// only `relocate` lines (both resolvers map a line's new parent under the
// lines above it, `spec/verify.md` §2.3), the `RuleTable`
// `declare::resolve` makes from the case's two images is the one the
// suite's resolver makes, by `id()`; and the table verifies. Among them
// pitfall row R16's good tree (`pitfalls/r16-good`) and a member of a
// class template's instantiation (`instantiated/`).
#[test]
fn the_rules_suite_resolves_alike() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../slip_digest/tests/rules");
    for case in [
        "verify/relocated-namespace",
        "parents/mapped-parent",
        "disc/overload-renamed-relocated-parameter",
        "pitfalls/r16-good",
        "instantiated/member-of-class-template-renamed",
    ] {
        let dir = root.join(case);
        let file = |f: &str| {
            std::fs::read_to_string(dir.join(f)).unwrap_or_else(|e| panic!("{case}/{f}: {e}"))
        };
        let (a, b, text) = (file("a.cpp"), file("b.cpp"), file("rules.txt"));
        let empty = RuleTable::empty();
        let (pre, post) = (records_of(&a, &empty), records_of(&b, &empty));
        let lines = read(&text).unwrap_or_else(|e| panic!("{case}: {e}"));
        let declared = resolve(&lines, &pre, &post).unwrap_or_else(|e| panic!("{case}: {e}"));
        let ours = digest_rules::read(&text).unwrap_or_else(|e| panic!("{case}: {e}"));
        let suite =
            digest_rules::table(&ours, &pre, &post).unwrap_or_else(|e| panic!("{case}: {e}"));
        assert_eq!(declared.rules.rules().len(), suite.table.rules().len(), "{case}");
        assert_eq!(declared.rules.id(), suite.table.id(), "{case}");
        // WP-111: every accepted order of the lines gives this table.
        let go = |l: &[(u64, Line)]| resolve(l, &pre, &post).map(|r| r.rules);
        assert_eq!(every_order(&lines, &go), declared.rules.id(), "{case}");
        let with =
            findings(&digest_of(&a, &empty), &digest_of(&b, &declared.rules), &Exemptions::none());
        assert!(!with.iter().copied().any(real), "{case}: {with:?}");
    }
}

// ---------------------------------------------------------------------------
// WP-111: a function a class template befriends without defining it

const FRIEND_PRE: &str = "template <class T> struct A { friend int f(int); };\nint f(int) { return 1; }\nA<int> a;\nint u() { return f(1); }\n";
const FRIEND_POST: &str = "template <class T> struct A { friend int g(int); };\nint g(int) { return 1; }\nA<int> a;\nint u() { return g(1); }\n";

/// `relocate` lines over `FRIEND_PRE`/`FRIEND_POST` with `facts` (or none):
/// the table, and the real findings of the post-image digested under it.
fn friend_case(text: &str, facts: &Facts) -> (RuleTable, Vec<(FindingKind, String)>) {
    let empty = RuleTable::empty();
    let (pre, post) = (records_of(FRIEND_PRE, &empty), records_of(FRIEND_POST, &empty));
    let r =
        resolve_with(&read(text).unwrap(), &pre, &post, facts).unwrap_or_else(|e| panic!("{e}"));
    let with = named_findings(
        &digest_of(FRIEND_PRE, &empty),
        &digest_of(FRIEND_POST, &r.rules),
        &Exemptions::none(),
    );
    (r.rules, with.into_iter().filter(|(k, _)| real(*k)).collect())
}

// `docs/wp/WP-071.md` "Added after the review of WP-103" (the rules suite's
// `kinds/function-first-declared-as-friend`): `relocate f(int) => :: g`
// alone writes the friend declaration's rule too, and verifies. The pair:
// the same line without the second rule (no facts) is a mismatch, `A`
// changed; the suite's two lines give the same table, in either order.
// A friend defined in the template is no undefined friend (it is followed).
#[test]
fn s2_3_undefined_friend_gets_its_rule() {
    let f = with_input(FRIEND_PRE, |i| facts(i, slip_digest::ReadOnly::NO_FILE));
    assert_eq!(f.undefined_friends.len(), 1, "{f:?}");
    assert!(f.dependency.is_empty());
    let (table, real_findings) = friend_case("relocate f(int) => :: g\n", &f);
    assert_eq!(table.rules().len(), 2, "{table:?}");
    assert_eq!(real_findings, [], "with the friend's rule");
    let (one, without) = friend_case("relocate f(int) => :: g\n", &Facts::default());
    assert_eq!(one.rules().len(), 1);
    assert!(without.iter().any(|(_, n)| n.starts_with('A')), "{without:?}");
    let both = "relocate f(int) => :: g\nrelocate A::friend f(int) => A :: g\n";
    let reversed = "relocate A::friend f(int) => A :: g\nrelocate f(int) => :: g\n";
    for text in [both, reversed] {
        assert_eq!(friend_case(text, &f).0, table, "{text}");
        assert_eq!(friend_case(text, &Facts::default()).0, table, "{text}");
    }
    // `=` where it is: a rule that changes nothing, refused (WP-111 review
    // F1). With the name kept the friend would need no rule.
    let empty = RuleTable::empty();
    let (pre, post) = (records_of(FRIEND_PRE, &empty), records_of(FRIEND_POST, &empty));
    let e = resolve_with(&read("relocate f(int) => :: =\n").unwrap(), &pre, &post, &f).unwrap_err();
    assert_eq!(e.kind, ErrorKind::Refused, "{e}");
    let defined = "template <class T> struct S { friend int f(int) { return 1; } };\nint f(int);\nS<int> s;\nint u() { return f(1); }\n";
    let f = with_input(defined, |i| facts(i, slip_digest::ReadOnly::NO_FILE));
    assert!(f.undefined_friends.is_empty(), "{f:?}");
}

// WP-111 review, mutant C11, and the friend rule of a function in a
// namespace (which matched no friend record before: their names are
// qualified, `n::A::friend n::f(int)`). Renaming `n::f` gives a rule for
// `n::A`'s friend declaration. `m::B`'s unqualified friend befriends
// `m::f`, another function, and gets none: its record's name says so. (C11,
// the class's nearest namespace not checked, survives this test: with the
// record's qualified name matched, an undefined friend named `n::f` is
// one an unqualified friend declaration in a class of `n` made, and
// `m::B`'s qualified friend of `n::f` is no undefined friend:
// `spec/landed/verify/WP-111.md` §6.)
#[test]
fn s2_3_undefined_friend_of_another_namespace() {
    let side = |f: &str| {
        format!(
            "namespace n {{ int {f}(int); template <class T> struct A {{ friend int {f}(int); }}; int {f}(int) {{ return 1; }} A<int> a; }}\n\
             namespace m {{ int f(int); template <class T> struct B {{ friend int f(int); friend int n::{f}(int); }}; int f(int) {{ return 2; }} B<int> b; }}\n"
        )
    };
    let (pre, post) = (side("f"), side("g"));
    let f = with_input(&pre, |i| facts(i, slip_digest::ReadOnly::NO_FILE));
    let empty = RuleTable::empty();
    let pre_r = records_of(&pre, &empty);
    let friends: Vec<&str> = pre_r
        .iter()
        .filter(|r| f.undefined_friends.contains(&r.id))
        .map(|r| r.name.as_str())
        .collect();
    assert!(friends.contains(&"n::A::friend n::f(int)"), "{friends:?}");
    assert!(friends.contains(&"m::B::friend m::f(int)"), "{friends:?}");
    let r = resolve_with(
        &read("relocate n::f(int) => n :: g\n").unwrap(),
        &pre_r,
        &records_of(&post, &empty),
        &f,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let rules = r.rules.rules();
    assert_eq!(rules.len(), 2, "{rules:?}");
    let a = pre_r.iter().find(|r| r.name == "n::A").unwrap();
    assert!(rules.iter().any(|x| x.to_parent == Parent::Unit(a.id)), "{rules:?}");
}

// WP-111 review, mutants C7 and C10: a friend's class whose namespace
// another line merges onto (`x => :: n`: every post-image member of `n`,
// the class too, maps into `x`). The friend rule's parent is the class
// mapped under the lines above (C7: the table built again for it), and a
// later line that maps a level of the class's chain is `Order` (C10: the
// class's levels are probes).
#[test]
fn s2_3_friend_class_in_a_merged_namespace() {
    let pre = "namespace x { int k; }\nnamespace p { }\n\
               namespace n { template <class T> struct A { friend int f(int); }; int f(int) { return 1; } A<int> a; }\n";
    let post_root = "int g(int) { return 1; }\nnamespace p { }\n\
                     namespace n { template <class T> struct A { friend int g(int); }; A<int> a; int k; }\n";
    let post_p = "namespace p { int g(int) { return 1; } }\n\
                  namespace n { template <class T> struct A { friend int g(int); }; A<int> a; int k; }\n";
    let facts_of = with_input(pre, |i| facts(i, slip_digest::ReadOnly::NO_FILE));
    assert_eq!(facts_of.undefined_friends.len(), 1, "{facts_of:?}");
    let empty = RuleTable::empty();
    let pre_r = records_of(pre, &empty);
    let class_a = pre_r.iter().find(|r| r.name == "n::A").unwrap();
    // C7: the merge above, then the rename with no new parent.
    let post_r = records_of(post_root, &empty);
    let resolved = resolve_with(
        &read("relocate x => :: n\nrelocate n::f(int) => :: g\n").unwrap(),
        &pre_r,
        &post_r,
        &facts_of,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let rules = resolved.rules.rules();
    let friend =
        rules.iter().find(|x| x.to_parent == Parent::Unit(class_a.id)).expect("a friend rule");
    let key_of = |name: &str| post_r.iter().find(|x| x.name == name).unwrap().key.clone().unwrap();
    let ns = resolved.rules.map(key_of("n"));
    let class = resolved.rules.map(Key { parent: Parent::Unit(ns.id()), ..key_of("n::A") });
    assert_eq!(friend.parent, Parent::Unit(class.id()), "the class as the merge maps it");
    // C10: the merge below the line that looked the class up.
    let order = resolve_with(
        &read("relocate n::f(int) => p :: g\nrelocate x => :: n\n").unwrap(),
        &pre_r,
        &records_of(post_p, &empty),
        &facts_of,
    )
    .unwrap_err();
    assert_eq!((order.line, order.kind), (2, ErrorKind::Order), "{order}");
}

// `Facts::dependency` over a parse: every entity of a read-only file (here
// every file) is one, an owned file's none; `merge` is the union.
#[test]
fn facts_of_a_parse() {
    let src = "namespace n { struct S {}; int f(int); }\n";
    let all = with_input(src, |i| facts(i, slip_digest::ReadOnly::EVERY_FILE));
    let none = with_input(src, |i| facts(i, slip_digest::ReadOnly::NO_FILE));
    assert!(none.dependency.is_empty());
    let recs = records_of(src, &RuleTable::empty());
    for name in ["n", "n::S", "n::f(int)"] {
        let r = recs.iter().find(|r| r.name == name).unwrap();
        assert!(all.dependency.contains(&r.id), "{name}");
    }
    let mut m = none.clone();
    m.merge(all.clone());
    assert_eq!(m, all);
}
