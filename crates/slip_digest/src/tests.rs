//! Unit tests of the walk (WP-024): groups and detail, rules, collisions,
//! counters, names, the row-name guards and the depth bound.

use std::collections::BTreeMap;

use slip_model::Digest;
use slip_model::digests::{GroupClass, GroupSum};
use slip_model::keys::{Key, KeyKind, Name, Parent, Rule, RuleTable};

use crate::units::{Written, mark_collisions};
use crate::{Cause, Detail, Input, Options, Output, tables};

/// Digests `files` (the first is the main file) under `rules` with
/// `detail`.
fn walk_with(files: &[(&str, &str)], rules: &RuleTable, detail: Detail<'_>) -> Output {
    let snippet =
        slip_kiln::snippet::Snippet { files, main: files[0].0, include: &[""], std: "c++20" };
    slip_kiln::snippet::parse(&snippet, |p| {
        assert_eq!(p.errors, 0, "{}", p.diagnostics);
        let path = |f| p.path(f).map(|s: &str| s.as_bytes().to_vec()).unwrap_or_default();
        let input = Input { ast: p.ast, sm: p.sm, subject_path: &path };
        crate::digest(&input, &Options { rules, detail, names: true })
    })
    .expect("the snippet parses")
}

fn walk(files: &[(&str, &str)]) -> Output {
    walk_with(files, &RuleTable::empty(), Detail::All)
}

/// Every unit by name: (class, file, digest, flags).
fn by_name(o: &Output) -> BTreeMap<String, Vec<(GroupClass, String, Digest, u64)>> {
    let names: BTreeMap<Digest, String> = o.names.iter().cloned().collect();
    let mut out: BTreeMap<String, Vec<_>> = BTreeMap::new();
    for g in &o.digests.groups {
        for u in g.units.iter().flatten() {
            let n = names.get(&u.key).cloned().unwrap_or_default();
            out.entry(n).or_default().push((
                g.class,
                String::from_utf8_lossy(&g.file).into_owned(),
                u.digest,
                u.flags,
            ));
        }
    }
    out
}

fn key_of(o: &Output, name: &str) -> Digest {
    o.names
        .iter()
        .find(|(_, n)| n == name)
        .map_or_else(|| panic!("no unit named {name}: {:?}", o.names), |(k, _)| *k)
}

fn counter(o: &Output, name: &str) -> u64 {
    o.counters.iter().find(|c| c.name == name).map_or(0, |c| c.value)
}

const TWO_FILES: &[(&str, &str)] = &[
    (
        "main.cpp",
        "#include \"h.h\"\nnamespace ns { void g(); }\nvoid ns::f(int) {}\nint use = ns::T<int>::v;\n",
    ),
    (
        "h.h",
        "namespace ns { void f(int); template <class X> struct T { static const int v = 1; }; }\n",
    ),
];

// spec/digest.md §11: a declared unit is in the group of the file of its
// first declaration (one unit for a header declaration defined in the
// main file); an instantiation is in the derived group of its template's
// file; the `tu` group holds every namespace.
#[test]
fn groups_by_file_and_class() {
    let o = walk(TWO_FILES);
    let u = by_name(&o);
    let f = &u["ns::f(int)"];
    assert_eq!(f.len(), 1, "one unit for a declaration and its definition: {f:?}");
    assert_eq!((f[0].0, f[0].1.as_str()), (GroupClass::Declared, "h.h"));
    assert_eq!((u["ns::g()"][0].0, u["ns::g()"][0].1.as_str()), (GroupClass::Declared, "main.cpp"));
    let ns = &u["ns"];
    assert_eq!(ns.len(), 1);
    assert_eq!((ns[0].0, ns[0].1.as_str()), (GroupClass::Tu, ""));
    let t = &u["ns::T<int>"];
    assert_eq!((t[0].0, t[0].1.as_str()), (GroupClass::Derived, "h.h"), "{u:?}");
    assert!(u.contains_key("<order>"));
    // Groups sorted by (class, file), and the counters agree.
    let order: Vec<_> = o.digests.groups.iter().map(|g| (g.class, g.file.clone())).collect();
    let mut sorted = order.clone();
    sorted.sort();
    assert_eq!(order, sorted);
    assert_eq!(counter(&o, "digest.groups"), o.digests.groups.len() as u64);
    let units: u64 = o.digests.groups.iter().map(|g| g.count).sum();
    assert_eq!(counter(&o, "digest.units"), units);
    let derived: u64 =
        o.digests.groups.iter().filter(|g| g.class == GroupClass::Derived).map(|g| g.count).sum();
    assert_eq!(counter(&o, "digest.units.derived"), derived);
    assert!(derived >= 1);
}

// spec/digest.md §11: the group digests and `tu` are the same whatever the
// detail; detail chooses only which groups carry their unit lists.
#[test]
fn detail_in_its_four_forms() {
    let rules = RuleTable::empty();
    let all = walk_with(TWO_FILES, &rules, Detail::All);
    let none = walk_with(TWO_FILES, &rules, Detail::None);
    let sums: Vec<GroupSum> =
        all.digests.groups.iter().map(slip_model::digests::Group::sum).collect();
    let picked = [(GroupClass::Declared, b"h.h".to_vec())];
    let groups = walk_with(TWO_FILES, &rules, Detail::Groups(&picked));
    let differing = walk_with(TWO_FILES, &rules, Detail::Differing(&sums));
    for o in [&none, &groups, &differing] {
        assert_eq!(o.digests.tu, all.digests.tu);
        let s: Vec<GroupSum> =
            o.digests.groups.iter().map(slip_model::digests::Group::sum).collect();
        assert_eq!(s, sums);
    }
    assert!(all.digests.groups.iter().all(|g| g.units.is_some()));
    assert!(none.digests.groups.iter().all(|g| g.units.is_none()));
    for g in &groups.digests.groups {
        assert_eq!(g.units.is_some(), g.class == GroupClass::Declared && g.file == b"h.h");
    }
    // Differing from its own sums: only the groups with an indeterminate
    // unit carry their lists.
    for g in &differing.digests.groups {
        assert_eq!(g.units.is_some(), g.indeterminate > 0);
    }
    // Differing from no sums: every group.
    let every = walk_with(TWO_FILES, &rules, Detail::Differing(&[]));
    assert!(every.digests.groups.iter().all(|g| g.units.is_some()));
}

fn rename_rule(from: &str, to: &str) -> Rule {
    Rule {
        parent: Parent::Root,
        kind: KeyKind::Namespace,
        name: Name::Identifier(from.as_bytes().to_vec()),
        disc: None,
        to_parent: Parent::Root,
        to_name: Name::Identifier(to.as_bytes().to_vec()),
    }
}

// spec/digest.md §3.4: a namespace relocated by a rule gives every key
// under it as the same source written under the new name.
#[test]
fn rules_relocate_a_namespace() {
    let old = [("main.cpp", "namespace old { struct S { int x; }; void f(S); enum E { A }; }\n")];
    let new = [("main.cpp", "namespace neu { struct S { int x; }; void f(S); enum E { A }; }\n")];
    let rules = RuleTable::new(vec![rename_rule("old", "neu")]).expect("a valid rule");
    let mapped = walk_with(&old, &rules, Detail::All);
    let plain = walk_with(&new, &RuleTable::empty(), Detail::All);
    let keys = |o: &Output| -> Vec<(Digest, Digest)> {
        let mut v: Vec<_> = o
            .digests
            .groups
            .iter()
            .flat_map(|g| g.units.iter().flatten().map(|u| (u.key, u.digest)))
            .collect();
        v.sort();
        v
    };
    assert_eq!(keys(&mapped), keys(&plain));
    // Without the rule, they differ.
    let unmapped = walk_with(&old, &RuleTable::empty(), Detail::All);
    assert_ne!(keys(&unmapped), keys(&plain));
}

// spec/digest.md §3.4: a namespace merged onto an existing one: a unit of
// each side that differs is two units of one key (a collision); equal
// declarations are one unit.
#[test]
fn rules_merge_a_namespace() {
    let src = [(
        "main.cpp",
        "namespace a { struct S { int x; }; void same(); }\nnamespace b { struct S { long y; }; void same(); }\n",
    )];
    let rules = RuleTable::new(vec![rename_rule("a", "b")]).expect("a valid rule");
    let o = walk_with(&src, &rules, Detail::All);
    let u = by_name(&o);
    // `b::same()`: written twice with equal digests (both declarations
    // have the same stream), one unit.
    let same = key_of(&o, "a::same()");
    let n_same = o
        .digests
        .groups
        .iter()
        .flat_map(|g| g.units.iter().flatten())
        .filter(|x| x.key == same)
        .count();
    assert_eq!(n_same, 1, "{u:?}");
    // `S`: two classes of one key: a collision, both indeterminate.
    let s = key_of(&o, "a::S");
    assert_eq!(s, key_of(&o, "b::S"));
    assert!(o.causes.iter().any(|(k, c)| *k == s && *c == Cause::Collision), "{:?}", o.causes);
    assert!(counter(&o, "digest.collisions") >= 1);
    // The namespaces themselves merge without a collision.
    let b = key_of(&o, "b");
    assert!(!o.causes.iter().any(|(k, c)| *k == b && *c == Cause::Collision), "{:?}", o.causes);
}

fn written(key: u8, digest: u8, kind: KeyKind) -> Written {
    Written {
        entity: None,
        key: Digest([key; 32]),
        kind,
        digest: Digest([digest; 32]),
        cause: None,
        class: GroupClass::Declared,
        file: b"f".to_vec(),
    }
}

// spec/digest.md §3.4: two entities of one key are both indeterminate
// with `Collision`; namespaces merge; repeatable declarations with equal
// digests are one entity.
#[test]
fn collisions() {
    let mut w = vec![
        written(1, 1, KeyKind::Class),
        written(1, 2, KeyKind::Class),
        written(2, 1, KeyKind::Namespace),
        written(2, 2, KeyKind::Namespace),
        written(3, 5, KeyKind::UsingDirective),
        written(3, 5, KeyKind::UsingDirective),
        written(4, 7, KeyKind::Function),
        written(4, 7, KeyKind::Function),
        written(5, 1, KeyKind::Variable),
    ];
    assert_eq!(mark_collisions(&mut w), 2);
    let causes: Vec<_> = w.iter().map(|u| u.cause).collect();
    let c = Some(Cause::Collision);
    assert_eq!(causes, vec![c, c, None, None, None, None, c, c, None]);
}

// spec/digest.md §12: the counters, by name, and the split by cause.
#[test]
fn counters() {
    let totals = crate::groups::Totals { units: 7, derived: 2, groups: 3, indeterminate: 4 };
    let k = |b| Digest([b; 32]);
    let causes = [
        (k(1), Cause::Collision),
        (k(2), Cause::Pending(crate::Area::Expr)),
        (k(3), Cause::Pending(crate::Area::Expr)),
        (k(4), Cause::Unsupported("x")),
    ];
    let got = crate::groups::counters(&totals, 1, 99, &causes);
    let got: Vec<(&str, u64)> = got.iter().map(|c| (c.name.as_str(), c.value)).collect();
    assert_eq!(
        got,
        vec![
            ("digest.bytes", 99),
            ("digest.collisions", 1),
            ("digest.groups", 3),
            ("digest.indeterminate", 4),
            ("digest.indeterminate.collision", 1),
            ("digest.indeterminate.pending", 2),
            ("digest.indeterminate.pending.expr", 2),
            ("digest.indeterminate.unsupported", 1),
            ("digest.units", 7),
            ("digest.units.derived", 2),
        ]
    );
}

// verify.md §6 `name`: qualified names, functions with their parameter
// types and qualifiers, specialisations, content-keyed units.
#[test]
fn names() {
    let o = walk(&[(
        "main.cpp",
        "namespace n { namespace { int hidden; }\n struct C { void m(int, const char*) const &; };\n \
         static_assert(true);\n template <class T> T id(T);\n}\n template <class T> struct W {};\n \
         template <> struct W<int> {};\n",
    )]);
    let names: Vec<&str> = o.names.iter().map(|(_, n)| n.as_str()).collect();
    for want in [
        "n",
        "n::(anonymous namespace)::hidden",
        "n::C",
        "n::C::m(int, const char *) const &",
        "n::<static_assert>",
        "n::id(type-parameter-0-0)",
        "W",
        "W<int>",
        "<order>",
    ] {
        assert!(names.contains(&want), "no {want} in {names:?}");
    }
    assert!(
        !names.iter().any(|n| n.contains("__va_list_tag")),
        "a builtin's members are on demand"
    );
    let mut sorted = o.names.clone();
    sorted.sort();
    assert_eq!(sorted, o.names, "names are sorted by key");
}

/// The names of a generated table's rows.
fn rows(t: &[(&'static str, tables::Decision)]) -> Vec<&'static str> {
    t.iter().map(|(n, _)| *n).collect()
}

// spec/digest-kinds.md §2.1: the flag words are gathered in row order. The
// walk pushes bits in the order these tables had when it was written; a
// table that changes order fails here, naming the record to review.
#[test]
fn record_row_names_guard_the_flag_words() {
    assert_eq!(rows(tables::quals::ROWS), ["0-const", "0-volatile", "0-restrict"]);
    assert_eq!(
        rows(tables::func_info::ROWS),
        [
            "0-variadic",
            "0-ref-qual",
            "0-no-proto",
            "0-this-const",
            "0-this-volatile",
            "0-this-restrict",
            "0-cc",
            "0-noreturn",
            "0-trailing-return",
            "0-no-caller-saved-registers",
            "0-regparm",
            "0-sme",
            "1"
        ]
    );
    assert_eq!(
        rows(tables::func_proto::ROWS),
        ["ret", "params", "info", "exc", "n_params", "abis"]
    );
    assert_eq!(
        rows(tables::decl_flags::ROWS),
        [
            "0-invalid",
            "0-implicit",
            "0-used",
            "0-referenced",
            "0-has-attrs",
            "0-has-lexical-ctx",
            "0-dependent-ctx",
            "0-injected"
        ]
    );
    assert_eq!(
        rows(tables::decl_misc::ROWS),
        [
            "0-access",
            "0-friend",
            "0-local-extern",
            "0-module-ownership",
            "0-name-independent",
            "0-linkage-cache",
            "0-visibility-cache",
            "0-first",
            "0-definition"
        ]
    );
    assert_eq!(
        rows(tables::template_parm_info::ROWS),
        ["depth", "index", "pack", "typename_kw", "constraint", "params", "ttp_kind"]
    );
    assert_eq!(
        rows(&tables::decl_node::ROWS[..9]),
        ["kind", "flags", "misc", "name", "ctx", "next", "loc", "ty-type", "ty-exception-spec"]
    );
    assert_eq!(rows(&tables::template_param_list::ROWS[3..]), ["depth", "params", "requires"]);
    assert_eq!(rows(tables::template_info::ROWS), ["decl", "params", "pattern", "constraint"]);
}

// WP-024 "what bounds the stack": a type nested past `DEPTH_LIMIT` makes
// the unit indeterminate with `Unaccounted("walk/depth")` instead of
// overflowing the stack, and the walk still ends.
#[test]
fn depth_is_bounded() {
    // One declarator, so that no memoised level shortens the walk.
    let src = format!("int {}deep;\nint **shallow;\n", "*".repeat(400));
    let o = walk(&[("main.cpp", src.as_str())]);
    let deep = key_of(&o, "deep");
    let cause = o.causes.iter().find(|(k, _)| *k == deep).map(|(_, c)| *c);
    assert_eq!(cause, Some(crate::walk::TOO_DEEP));
    let shallow = key_of(&o, "shallow");
    let cause = o.causes.iter().find(|(k, _)| *k == shallow).map(|(_, c)| *c);
    assert_ne!(cause, Some(crate::walk::TOO_DEEP));
}

// spec/digest.md §1 rule 3: the same input gives the same output.
#[test]
fn a_walk_is_repeatable() {
    assert_eq!(walk(TWO_FILES), walk(TWO_FILES));
}

// spec/digest.md §3.1, §3.4: `Key` ids are those of `slip_model`.
#[test]
fn order_unit_key() {
    let o = walk(&[("main.cpp", "int x;\n")]);
    let k = Key { kind: KeyKind::Order, parent: Parent::Root, name: Name::Order, disc: None };
    assert_eq!(key_of(&o, "<order>"), k.id());
    assert!(o.causes.contains(&(k.id(), Cause::Pending(crate::Area::Order))));
}
