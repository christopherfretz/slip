//! Unit tests of the walk (WP-024): groups and detail, rules, collisions,
//! counters, names, the row-name guards and the depth bound.

use std::collections::BTreeMap;

use slip_model::Digest;
use slip_model::digests::{GroupClass, GroupSum};
use slip_model::keys::{Key, KeyKind, Name, Parent, Rule, RuleTable};

use crate::units::{Written, mark_collisions, mark_part_collisions};
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
        crate::digest(
            &input,
            &Options {
                rules,
                read_only: crate::ReadOnly::NO_FILE,
                detail,
                names: true,
                rule_use: false,
            },
        )
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
        read: None,
        key: Digest([key; 32]),
        kind,
        digest: Digest([digest; 32]),
        cause: None,
        class: GroupClass::Declared,
        file: b"f".to_vec(),
        resolved: None,
        default_argument: None,
        owner: None,
        collided: false,
        view: None,
        denotes: None,
        emitted: false,
    }
}

/// A part (`KeyKind::Resolved`) of the unit keyed `owner`, of entity `e`.
fn part(key: u8, digest: u8, owner: u8, e: u32) -> Written {
    let mut u = written(key, digest, KeyKind::Resolved);
    u.owner = Some(Digest([owner; 32]));
    u.entity = <slip_kiln::kiln::ast::DeclId as slip_kiln::kiln::ast::RawId>::from_raw(e);
    u
}

// spec/digest.md §3.4, §12 (WP-045 third look): every part of a colliding
// owner is marked, partner or not; a run of parts is counted unless every
// two of its parts are of two different owners whose shared key collides.
#[test]
fn part_collisions() {
    let mut w = vec![
        // Owners 1 (entities 1 and 2) collide.
        written(1, 1, KeyKind::Function),
        written(1, 2, KeyKind::Function),
        // Their parts share key 10: the owners' collision, not counted.
        part(10, 1, 1, 1),
        part(10, 2, 1, 2),
        // A part of owner 1 with no partner: marked all the same.
        part(11, 3, 1, 2),
        // A part of that part: marked too.
        part(12, 4, 11, 2),
        // Owner 2 does not collide; two of its parts hash alike: counted.
        written(2, 5, KeyKind::Function),
        part(20, 6, 2, 3),
        part(20, 7, 2, 3),
        // Two parts of one colliding owner's entity hash alike: counted.
        part(13, 8, 1, 1),
        part(13, 9, 1, 1),
        // A part of a non-colliding owner, alone: not marked.
        part(21, 10, 2, 3),
    ];
    assert_eq!(mark_collisions(&mut w), 1);
    assert_eq!(mark_part_collisions(&mut w), 2);
    let marked: Vec<bool> = w.iter().map(|u| u.cause == Some(Cause::Collision)).collect();
    assert_eq!(
        marked,
        vec![true, true, true, true, true, true, false, true, true, true, true, false]
    );
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
    let totals = crate::groups::Totals {
        units: 7,
        derived: 2,
        derived_emitted: 1,
        groups: 3,
        indeterminate: 4,
    };
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
            ("digest.units.derived.emitted", 1),
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
        "template <class> type-parameter-0-0 n::id(type-parameter-0-0)",
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
            "0-reserved",
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
    assert!(!o.causes.iter().any(|(id, _)| *id == k.id()), "the order unit is written (WP-033)");
}

/// Runs `f` on a walk over `src` (one file, `main.cpp`), with the AST.
fn with_walk<R>(src: &str, f: impl FnOnce(&mut crate::walk::Walk<'_>) -> R) -> R {
    with_walk_std(src, "c++20", f)
}

/// [`with_walk`] in the language mode `std`.
fn with_walk_std<R>(src: &str, std: &str, f: impl FnOnce(&mut crate::walk::Walk<'_>) -> R) -> R {
    let files = [("main.cpp", src)];
    let snippet =
        slip_kiln::snippet::Snippet { files: &files, main: "main.cpp", include: &[""], std };
    let rules = RuleTable::empty();
    slip_kiln::snippet::parse(&snippet, |p| {
        assert_eq!(p.errors, 0, "{}", p.diagnostics);
        let path = |f| p.path(f).map(|s: &str| s.as_bytes().to_vec()).unwrap_or_default();
        let input = Input { ast: p.ast, sm: p.sm, subject_path: &path };
        let mut w = crate::walk::Walk::new(&input, &rules);
        f(&mut w)
    })
    .expect("the snippet parses")
}

/// Every declaration of kind `kind` whose identifier is `name`, in arena
/// order.
fn decls_named(
    w: &crate::walk::Walk<'_>,
    name: &str,
    kind: slip_kiln::kiln::ast::decl::DeclKind,
) -> Vec<slip_kiln::kiln::ast::DeclId> {
    use slip_kiln::kiln::ast::names::DeclNameView;
    let ast = w.ast;
    ast.decls
        .ids()
        .filter(|&d| {
            let n = ast.decls.get(d);
            let ident = match ast.names.view(n.name()) {
                DeclNameView::Identifier(s) => ast.symbols().as_str(s) == name,
                DeclNameView::Empty
                | DeclNameView::Operator(_)
                | DeclNameView::Constructor(_)
                | DeclNameView::Destructor(_)
                | DeclNameView::Conversion(_)
                | DeclNameView::LiteralOperator(_)
                | DeclNameView::DeductionGuide(_) => false,
            };
            n.kind() == kind && ident
        })
        .collect()
}

fn cause_of(o: &Output, name: &str) -> Option<Cause> {
    let k = key_of(o, name);
    o.causes.iter().find(|(x, _)| *x == k).map(|(_, c)| *c)
}

/// digest-kinds §5.3 `ty-exception-spec`, digest §11.2: a function of the
/// deferrable class writes no specification, and is not indeterminate for
/// it: the resolved unit `exception_spec` holds it (WP-046).
#[test]
fn deferrable_absence_is_resolved() {
    let o = walk(&[("main.cpp", "struct S { ~S() noexcept(false); };\n")]);
    let names: Vec<String> = o.names.iter().map(|(_, n)| n.clone()).collect();
    let name = names.iter().find(|n| n.contains("~S") && !n.contains("::<")).expect("~S");
    assert_eq!(cause_of(&o, name), None);
    let resolved = format!("{name}::<exception_spec>");
    assert!(names.contains(&resolved), "{names:?}");
    assert_eq!(cause_of(&o, &resolved), None);
}

/// digest §3.4, digest-kinds §3: the cause of a key is that of every key it
/// is built from; a member of `A<^^int>` carries `A<^^int>`'s cause, a
/// reflection value being `unsupported` (digest-kinds §8 `ValueKind`
/// `Reflection`; WP-024 review C, moved off `Pending(Value)` by WP-028).
#[test]
fn a_key_keeps_its_parents_cause() {
    use slip_kiln::kiln::ast::decl::DeclKind;
    const REFLECTION: Cause = Cause::Unsupported("ValueKind/Reflection");
    let src =
        "template <auto V> struct A { struct In {}; };\nvoid g(A<^^int>::In);\nvoid h(A<^^int>);\n";
    with_walk_std(src, "c++26", |w| {
        for name in ["g", "h"] {
            let f = decls_named(w, name, DeclKind::Function)[0];
            match w.key(f) {
                crate::keys::KeyRes::Key { cause, .. } => {
                    assert_eq!(cause, Some(REFLECTION), "{name}");
                }
                crate::keys::KeyRes::None(c) => panic!("{name}: no key ({c:?})"),
            }
        }
    });
    // Two such keys in one translation unit are one key, and marked.
    let files = [(
        "main.cpp",
        "template <auto V> struct A { struct In {}; };\nvoid g(A<^^int>::In);\nvoid g(A<^^char>::In);\n",
    )];
    let rules = RuleTable::empty();
    let snippet = slip_kiln::snippet::Snippet {
        files: &files,
        main: "main.cpp",
        include: &[""],
        std: "c++26",
    };
    let o = slip_kiln::snippet::parse(&snippet, |p| {
        assert_eq!(p.errors, 0, "{}", p.diagnostics);
        let path = |f| p.path(f).map(|s: &str| s.as_bytes().to_vec()).unwrap_or_default();
        let input = Input { ast: p.ast, sm: p.sm, subject_path: &path };
        crate::digest(
            &input,
            &Options {
                rules: &rules,
                read_only: crate::ReadOnly::NO_FILE,
                detail: Detail::All,
                names: true,
                rule_use: false,
            },
        )
    })
    .expect("the snippet parses");
    let g: Vec<_> = o.names.iter().filter(|(_, n)| n.starts_with("g(")).collect();
    assert!(!g.is_empty());
    for (k, n) in g {
        let c = o.causes.iter().find(|(x, _)| x == k).map(|(_, c)| *c);
        assert!(matches!(c, Some(Cause::Collision | REFLECTION)), "{n}: {c:?}");
    }
    assert!(counter(&o, "digest.collisions") > 0);
}

/// WP-024 review H: a key computed below a detected cycle depends on the
/// path that reached it and is not memoised.
#[test]
fn a_key_below_a_cycle_is_not_memoised() {
    use slip_kiln::kiln::ast::decl::DeclKind;
    with_walk("struct S { int m; };\n", |w| {
        let s = decls_named(w, "S", DeclKind::CXXRecord)[0];
        let m = decls_named(w, "m", DeclKind::Field)[0];
        let s = w.entity(s);
        w.keys_in_progress.insert(s);
        assert!(matches!(w.key(m), crate::keys::KeyRes::None(Some(_))));
        w.keys_in_progress.remove(&s);
        assert!(matches!(w.key(m), crate::keys::KeyRes::Key { .. }));
    });
}

/// WP-024 review G: a key is computed with the unit's state put aside, and
/// a local entity met inside one takes no ordinal.
#[test]
fn a_key_does_not_touch_the_unit() {
    use slip_kiln::kiln::ast::decl::DeclKind;
    with_walk("void f(int p);\n", |w| {
        let f = decls_named(w, "f", DeclKind::Function)[0];
        let p = decls_named(w, "p", DeclKind::ParmVar)[0];
        w.unit.next_local = 7;
        assert!(matches!(w.key(f), crate::keys::KeyRes::Key { .. }));
        assert_eq!(w.unit.next_local, 7);
        w.in_key = 1;
        let mut s = crate::stream::Stream::new();
        w.write_ref(&mut s, p);
        w.in_key = 0;
        assert_eq!(s.cause(), Some(Cause::Unaccounted("key/local-ref")));
        assert_eq!(w.unit.next_local, 7);
    });
}

/// digest-kinds §5.2 (as corrected by WP-024 review F): a function declared
/// at block scope inside `extern "C"` has the `c_linkage` parent, the key
/// of the same function declared at namespace scope.
#[test]
fn a_block_scope_c_function_is_c_linkage() {
    let inner = walk(&[("main.cpp", "extern \"C\" { void f() { void g(); } }\n")]);
    let outer = walk(&[("main.cpp", "extern \"C\" void g();\n")]);
    let gi = o_key(&inner, "g");
    let go = o_key(&outer, "g");
    assert_eq!(gi, go);
}

/// The key of the unit whose name starts with `name` (followed by `(` or
/// the end).
fn o_key(o: &Output, name: &str) -> Digest {
    o.names
        .iter()
        .find(|(_, n)| n == name || n.strip_prefix(name).is_some_and(|r| r.starts_with('(')))
        .map_or_else(|| panic!("no unit named {name}: {:?}", o.names), |(k, _)| *k)
}

/// digest-kinds §2.1: a flag word of more than 64 rows is a shape no table
/// has: the stream is indeterminate, not wrong (WP-024 review J).
#[test]
fn a_flag_word_past_64_rows_is_unaccounted() {
    let mut w = crate::stream::FlagWord::new();
    for _ in 0..64 {
        w = w.bit(true);
    }
    let mut s = crate::stream::Stream::new();
    s.flags(w);
    assert_eq!(s.cause(), None);
    let mut s = crate::stream::Stream::new();
    s.flags(w.bit(false));
    assert_eq!(s.cause(), Some(Cause::Unaccounted("flags/overflow")));
}

/// digest §10 (WP-024 review E): a unit whose key cannot be formed is
/// written under a fallback key (its own kind, parent the nearest keyed
/// entity, name `content`, disc its digest), indeterminate with the key's
/// cause; it does not vanish.
#[test]
fn a_unit_without_a_key_is_kept() {
    use slip_kiln::kiln::ast::decl::DeclKind;
    with_walk("struct S { int m; };\n", |w| {
        let m = w.entity(decls_named(w, "m", DeclKind::Field)[0]);
        let s = w.entity(decls_named(w, "S", DeclKind::CXXRecord)[0]);
        let crate::keys::KeyRes::Key { id: parent, .. } = w.key(s) else { panic!("S has a key") };
        w.keys.insert(m, crate::keys::KeyRes::None(Some(Cause::Unaccounted("key/test"))));
        let u = crate::units::write(w, m, crate::units::UnitClass::Declared).expect("written");
        assert_eq!(u.cause, Some(Cause::Unaccounted("key/test")));
        assert_eq!(u.kind, KeyKind::Field);
        let want = Key {
            kind: KeyKind::Field,
            parent: Parent::Unit(parent),
            name: Name::Content,
            disc: Some(u.digest),
        };
        assert_eq!(u.key, want.id());
    });
}

/// A view unit's key, its namespace's own key, its digest, its own unit's
/// digest, and its view (`view_units_of`).
type ViewUnit = (Digest, Digest, Digest, Digest, Option<bool>);

/// The view units (`units::view_units`) of `files` (the first is the main
/// file) under `rules`, with every file named in `read_only` read-only:
/// for each, its key, the key and digest of its namespace's own unit and
/// its view; and the rule use.
fn view_units_of(
    files: &[(&str, &str)],
    read_only: &[&str],
    rules: &RuleTable,
) -> (Vec<ViewUnit>, Vec<slip_model::agreement::Use>) {
    let snippet =
        slip_kiln::snippet::Snippet { files, main: files[0].0, include: &[""], std: "c++20" };
    slip_kiln::snippet::parse(&snippet, |p| {
        assert_eq!(p.errors, 0, "{}", p.diagnostics);
        let ro: Vec<slip_kiln::kiln::source::FileId> =
            p.sm.file_ids()
                .filter(|&f| read_only.iter().any(|r| p.sm.file_name(f).ends_with(r)))
                .collect();
        assert_eq!(ro.len(), read_only.len(), "every read-only file is a buffer");
        let pred = move |f: slip_kiln::kiln::source::FileId| ro.contains(&f);
        let path = |f| p.path(f).map(|s: &str| s.as_bytes().to_vec()).unwrap_or_default();
        let input = Input { ast: p.ast, sm: p.sm, subject_path: &path };
        let options = Options {
            rules,
            read_only: crate::ReadOnly::new(&pred),
            detail: Detail::None,
            names: false,
            rule_use: true,
        };
        crate::walk::run_with(&input, &options, |_, written, out| {
            let views = written
                .iter()
                .filter(|u| u.view.is_some())
                .map(|v| {
                    let own = written
                        .iter()
                        .find(|u| u.view.is_none() && u.entity == v.entity)
                        .expect("the namespace's own unit");
                    (v.key, own.key, v.digest, own.digest, v.view)
                })
                .collect();
            (views, out.rule_use)
        })
    })
    .expect("the snippet parses")
}

// WP-140 (`spec/digest.md` §3.4, `units::view_units`): a namespace that a
// read-only file and owned code both open has a unit in the owned view
// when that view's key differs from its own: one, whatever the number of
// owned openings, with the namespace's own digest, and the rule that
// matched its key counts `via` once (mutants: the unit written when the
// keys are equal, which the dedup of `groups::build` would hide from every
// `Output`; `via` counted per opening). Under a rule that matches nothing
// the keys are equal and no unit is written; under the empty table none
// is, and with no read-only file there is no second view.
#[test]
fn wp140_a_view_unit_only_where_its_key_differs() {
    let files = [
        (
            "main.cpp",
            "#include \"dep.h\"\nnamespace acme { int g(int); }\nnamespace acme { int h(int); }\n",
        ),
        ("dep.h", "namespace acme { struct Dep {}; }\n"),
    ];
    let moved = RuleTable::new(vec![rename_rule("acme", "legacy")]).expect("a valid rule");
    let (views, uses) = view_units_of(&files, &["dep.h"], &moved);
    assert_eq!(views.len(), 1, "{views:?}");
    let (key, own, digest, own_digest, view) = views[0];
    assert_ne!(key, own);
    assert_eq!(digest, own_digest, "the namespace's one digest");
    assert_eq!(view, Some(true));
    assert_eq!(uses, [slip_model::agreement::Use { rule: 0, via: 1, direct: 0, kept_via: 0 }]);
    let inert =
        RuleTable::new(vec![rename_rule("slip_wp140_none", "legacy")]).expect("a valid rule");
    let (views, uses) = view_units_of(&files, &["dep.h"], &inert);
    assert_eq!((views.len(), uses.len()), (0, 0), "{views:?}");
    let (views, _) = view_units_of(&files, &["dep.h"], &RuleTable::empty());
    assert!(views.is_empty());
    let (views, _) = view_units_of(&files, &[], &moved);
    assert!(views.is_empty(), "every file owned: one view");
}
