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
    Entity, ErrorKind, Line, LineError, Needs, Resolved, finish, needs, read, resolve,
};
use slip_digest::records::{Record, records};
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
    let r = resolve(
        &read("relocate n => :: fresh\nrelocate n::C => fresh :: =").unwrap(),
        &p.recs,
        &post,
    )
    .unwrap();
    let rules = r.rules.rules();
    assert_eq!(rules.len(), 2);
    let second = rules.iter().find(|x| x.kind == KeyKind::Class).unwrap();
    assert_eq!(second.parent, Parent::Unit(p.n.id()), "the parent mapped: fresh is n");
    // `fresh::C` of the post-image, its parent mapped first, is `n::C`.
    let fresh_mapped = r.rules.map(fresh.clone());
    assert_eq!(fresh_mapped, p.n);
    let c_post = key(KeyKind::Class, Parent::Unit(fresh_mapped.id()), ident("C"), None);
    assert_eq!(r.rules.map(c_post), p.c);
    // The order of the lines matters: the parent first, it is not mapped.
    let r = resolve(
        &read("relocate n::C => fresh :: =\nrelocate n => :: fresh").unwrap(),
        &p.recs,
        &post,
    )
    .unwrap();
    let second = r.rules.rules().iter().find(|x| x.kind == KeyKind::Class).unwrap();
    assert_eq!(second.parent, Parent::Unit(fresh.id()));
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
    assert_eq!(r, Resolved { rules: RuleTable::empty(), changed, removed: vec![p.c.id()] });
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
    // The same entity relocated to two places is two rules, which
    // verification then judges.
    assert!(
        resolve(&read("relocate n::C => :: D\nrelocate n::C => :: E").unwrap(), &p.recs, &[])
            .is_ok()
    );
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
    with_input(source, |i| records(i, rules)).expect("records")
}

fn digest_of(source: &str, rules: &RuleTable) -> Output {
    with_input(source, |i| {
        slip_digest::digest(i, &Options { rules, detail: Detail::All, names: true })
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
    let d = verified(pre, post, "relocate C => :: D\nrelocate C::m(int) => D :: =\n");
    let identity = d.rules().rules().iter().find(|r| r.kind != KeyKind::Class).unwrap();
    assert_eq!((identity.parent, &identity.name), (identity.to_parent, &identity.to_name));
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
    let r = Resolved { rules: RuleTable::empty(), changed: vec![], removed: vec![] };
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
        let with =
            findings(&digest_of(&a, &empty), &digest_of(&b, &declared.rules), &Exemptions::none());
        assert!(!with.iter().copied().any(real), "{case}: {with:?}");
    }
}
