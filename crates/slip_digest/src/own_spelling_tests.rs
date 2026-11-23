//! Tests of `own_spelling.rs` (WP-041): what a unit's own spellings hold
//! beyond their profiles.

use std::collections::BTreeMap;
use std::path::Path;

use slip_kiln::kiln::ast::decl::DeclKind;
use slip_kiln::kiln::ast::types::TypeView;
use slip_kiln::kiln::ast::{DeclId, ExprId};
use slip_model::Digest;
use slip_model::keys::RuleTable;

use super::{DIRECT, Fact, LISTS, NO_LIST};
use crate::expr_profile::RAISED;
use crate::expr_profile::tests::{each_input, test_dirs};
use crate::stream::Stream;
use crate::walk::Walk;
use crate::{Cause, Input};

/// One unit of a digest: its key, digest and causes.
#[derive(Clone, Debug, PartialEq, Eq)]
struct U {
    key: Digest,
    digest: Digest,
    causes: Vec<Cause>,
}

/// Every unit of `input`'s digest by name.
fn units_of(input: &Input<'_>) -> BTreeMap<String, U> {
    let rules = RuleTable::empty();
    let o = crate::digest(
        input,
        &crate::Options {
            rules: &rules,
            read_only: crate::ReadOnly::NO_FILE,
            detail: crate::Detail::All,
            names: true,
            rule_use: false,
        },
    );
    let mut digests = BTreeMap::new();
    for g in &o.digests.groups {
        for u in g.units.iter().flatten() {
            digests.insert(u.key, u.digest);
        }
    }
    o.names
        .iter()
        .map(|(k, n)| {
            let causes = o.causes.iter().filter(|(c, _)| c == k).map(|(_, c)| *c).collect();
            (n.clone(), U { key: *k, digest: digests[k], causes })
        })
        .collect()
}

/// Every unit of the digest of `src` (C++26) by name.
fn units(src: &str) -> BTreeMap<String, U> {
    let files = [("main.cpp", src)];
    let snippet = slip_kiln::snippet::Snippet {
        files: &files,
        main: "main.cpp",
        include: &[""],
        std: "c++26",
    };
    slip_kiln::snippet::parse(&snippet, |p| {
        assert_eq!(p.errors, 0, "{src}\n{}", p.diagnostics);
        let path = |_| Vec::new();
        let input = Input { ast: p.ast, sm: p.sm, subject_path: &path };
        units_of(&input)
    })
    .expect("the snippet parses")
}

/// What every shape may use: two `f`, so each spelling finds both.
const PRELUDE: &str = "int f(int);\nlong f(long);\nstruct X {};\n\
    X operator+(X, X);\n\
    template<unsigned long N> struct Z {};\n\
    template<class T, unsigned long N> struct Y1;\n\
    template<class T, unsigned long N> struct Y2;\n";

/// The shapes of rule 2 and the three sites, each a declaration named
/// `@` (`g` or `h`, two entities with one profile and one found set).
const SHAPES: &[(&str, &str)] = &[
    ("trailing-decltype", "template<class T> auto @(T t) -> decltype(f(t));\n"),
    ("array-bound", "template<class T> void @(T t, char (*p)[sizeof(f(T()))]);\n"),
    ("noexcept", "template<class T> void @(T t, void (*p)() noexcept(noexcept(f(T()))));\n"),
    ("nttp-argument", "template<class T> void @(T t, Z<sizeof(f(T()))> z);\n"),
    ("default-argument", "template<class T, class U = decltype(f(T()))> struct @;\n"),
    ("base", "template<class T> struct @ : Z<sizeof(f(T()))> {};\n"),
    ("variable-template", "template<class T> extern decltype(f(T())) @;\n"),
    ("requires", "template<class T> requires (sizeof(f(T())) > 1) void @(T t);\n"),
    ("operator", "template<class T> auto @(T t) -> decltype(t + t);\n"),
    (
        "member-initialiser",
        "template<class T> struct @ { static constexpr int v = sizeof(Z<sizeof(f(T()))>); };\n",
    ),
    ("alias-template", "template<class T> using @ = decltype(f(T()));\n"),
];

/// Order independence (the reason the mark exists): the same two
/// declarations in the other order give the same digests.
#[test]
fn the_order_of_two_spellings_changes_no_digest() {
    for (label, shape) in SHAPES {
        let g = shape.replace('@', "g");
        let h = shape.replace('@', "h");
        let one = units(&format!("{PRELUDE}{g}{h}"));
        let other = units(&format!("{PRELUDE}{h}{g}"));
        assert_eq!(one, other, "{label}");
        for (n, u) in &one {
            assert!(
                !u.causes
                    .iter()
                    .any(|c| matches!(c, Cause::Unaccounted(w) if w.starts_with("spelling/"))),
                "{label}: {n} {:?}",
                u.causes
            );
        }
    }
}

/// The partial specialisation's site: two partial specialisations of two
/// templates in both orders.
#[test]
fn the_order_of_two_partial_specialisations_changes_no_digest() {
    let a = "template<class T> struct Y1<T, sizeof(f(T()))> {};\n";
    let b = "template<class T> struct Y2<T, sizeof(f(T()))> {};\n";
    assert_eq!(units(&format!("{PRELUDE}{a}{b}")), units(&format!("{PRELUDE}{b}{a}")));
}

/// Decision 1: the set is a set: lookup order and duplicates are not in
/// the bytes.
#[test]
fn the_set_is_written_sorted_and_without_duplicates() {
    crate::decl_func::tests::with_walk("int f(int);\nlong f(long);\n", |w| {
        let fs = crate::decl_func::tests::named(w, "f", DeclKind::Function);
        assert_eq!(fs.len(), 2);
        let bytes = |w: &mut Walk<'_>, f: Fact| {
            let mut s = Stream::new();
            w.unit.facts = vec![f];
            w.unit.facts_due = true;
            super::write_unit_list(w, &mut s);
            s.as_bytes().to_vec()
        };
        let one = bytes(w, Fact::Found(vec![fs[0], fs[1]]));
        let other = bytes(w, Fact::Found(vec![fs[1], fs[0], fs[1]]));
        assert_eq!(one, other);
        let fewer = bytes(w, Fact::Found(vec![fs[1]]));
        assert_ne!(one, fewer);
        // The kind tag is in the bytes: a found set and an operator set
        // of the same declarations differ.
        assert_ne!(one, bytes(w, Fact::Oper(vec![fs[0], fs[1]])));
        // A list that is not due writes nothing.
        let mut s = Stream::new();
        w.unit.facts = vec![Fact::Found(Vec::new())];
        w.unit.facts_due = false;
        super::write_unit_list(w, &mut s);
        assert!(s.as_bytes().is_empty());
    });
}

/// Every pair-test side that parses.
fn each_pair_input(mut f: impl FnMut(&Path, &Input<'_>)) {
    let mut dirs = Vec::new();
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    for root in ["differ", "equal", "demand", "indeterminate"] {
        test_dirs(&tests.join(root), &mut dirs);
    }
    for dir in &dirs {
        each_input(dir, |input| f(dir, input));
    }
}

/// Decision 2: a list changes only the units that write one, and a unit
/// writes one only when its own spellings hold a non-empty set: over the
/// whole pair-test corpus, the units whose digest a list changes are as
/// many as the lists written. No unit is `Unaccounted("spelling/...")`
/// beyond the ones counted here (rule 5).
#[test]
fn a_list_changes_only_the_units_that_write_one() {
    let mut lists = 0;
    let mut changed = 0;
    let mut sites: BTreeMap<String, Vec<String>> = BTreeMap::new();
    each_pair_input(|dir, input| {
        NO_LIST.with(|c| c.set(true));
        let without = units_of(input);
        NO_LIST.with(|c| c.set(false));
        LISTS.with(|c| c.set(0));
        let with = units_of(input);
        let written = LISTS.with(std::cell::Cell::get);
        let n = with
            .iter()
            .filter(|(name, u)| without.get(*name).is_some_and(|v| v.digest != u.digest))
            .count();
        // A content-keyed unit's key is its digest (`spec/digest.md`
        // §3.3): its list moves it, and the units that hold the key
        // change with it. No other key moves.
        let moved: Vec<&String> = with
            .iter()
            .filter(|(name, u)| without.get(*name).is_none_or(|v| v.key != u.key))
            .map(|(name, _)| name)
            .collect();
        assert_eq!(with.len(), without.len(), "{}", dir.display());
        assert!(
            moved.iter().all(|m| m.contains("<static_assert>")),
            "{}: a key moved: {moved:?}",
            dir.display()
        );
        if moved.is_empty() {
            assert_eq!(n, written, "{}: {n} units changed, {written} lists", dir.display());
        } else {
            assert!(n >= written, "{}: {n} units changed, {written} lists", dir.display());
        }
        lists += written;
        changed += n;
        for (name, u) in &with {
            for c in &u.causes {
                if let Cause::Unaccounted(w) = c
                    && w.starts_with("spelling/")
                {
                    sites.entry((*w).to_owned()).or_default().push(format!(
                        "{} {name}",
                        dir.strip_prefix(env!("CARGO_MANIFEST_DIR")).unwrap_or(dir).display()
                    ));
                }
            }
        }
    });
    eprintln!("{lists} lists, {changed} units changed; rule 5: {sites:#?}");
    assert!(lists > 10, "{lists} lists over the corpus");
    // Rule 5 and its kin end only in `indeterminate/` tests, one at least
    // per site a source reaches (the landed file's table).
    let stray: Vec<&String> =
        sites.values().flatten().filter(|u| !u.starts_with("tests/indeterminate/")).collect();
    assert!(
        stray.is_empty(),
        "units Unaccounted(\"spelling/...\") outside indeterminate/: {stray:#?}"
    );
    for site in REACHED {
        assert!(sites.contains_key(*site), "no pair test reaches {site}");
    }
}

/// Rule 6: `Walk::left_out` is raised at exactly the expressions where the
/// reading yields a non-empty set, on every pair-test snippet: for each
/// expression, walked in profile mode (outside a type) and read
/// (`own_spelling::expr`, outside a type), the same expressions.
#[test]
fn left_out_is_raised_where_the_list_has_an_entry() {
    let mut met = 0usize;
    let mut raised_total = 0usize;
    let mut bad = Vec::new();
    each_pair_input(|dir, input| {
        let rules = RuleTable::empty();
        let mut w = Walk::new(input, &rules);
        for i in 1..=w.ast.exprs.len() {
            let Some(e) = u32::try_from(i).ok().and_then(ExprId::try_new) else { continue };
            RAISED.with(|r| r.borrow_mut().clear());
            // Raised outside a type and outside a key the walk computes
            // on the way (a specialisation's): `type_nesting + in_key == 1`.
            w.in_key += 1;
            crate::expr_dep::profile_expr(&mut w, &mut Stream::new(), e);
            w.in_key -= 1;
            let mut raised: Vec<ExprId> = RAISED
                .with(|r| r.borrow().iter().filter(|(_, n)| *n == 1).map(|(e, _)| *e).collect());
            DIRECT.with(|d| d.borrow_mut().clear());
            let saved = w.take_unit();
            super::expr(&mut w, &mut Stream::new(), e, super::Ctx::Known(false));
            w.unit = saved;
            let mut read: Vec<ExprId> = DIRECT.with(|d| d.borrow().clone());
            raised.sort_unstable();
            read.sort_unstable();
            met += 1;
            raised_total += raised.len();
            if raised != read {
                bad.push(format!("{} {e:?}: raised {raised:?}, read {read:?}", dir.display()));
            }
        }
    });
    eprintln!("{met} expressions, {raised_total} raises");
    assert!(raised_total > 10, "{raised_total} raises");
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// Decision 4: a member template of an instantiation (`A<int>`) has a
/// written type Kiln made by substitution. Its expressions are its own: a
/// dependent expression is rebuilt (`Substituter::needs_rebuild`,
/// `kiln_sema/src/templ/subst/exprs.rs:30`, is true for every
/// instantiation-dependent one, whatever the depth of the parameter it
/// names), a fresh `ExprId` per substitution, and the rebuilt unresolved
/// name keeps the pattern's found set (`Substituter::callee`,
/// `exprs.rs:2341`, `unresolved_decls`), the set lookup found at the
/// definition, which is the member's meaning. So rule 2 applies as it
/// stands: `f(long)`, declared after the definition, is in neither set.
#[test]
fn an_instantiated_member_reads_the_patterns_found_set() {
    let src = "int f(int);\n\
               template<class T> struct A { template<class U> auto m(U u) -> decltype(f(u)); \
               template<class U> auto n(U u) -> decltype(f(u, T())); };\n\
               long f(long);\n\
               template struct A<int>;\n";
    crate::decl_func::tests::with_walk(src, |w| {
        let ret_expr = |w: &Walk<'_>, d: DeclId| -> ExprId {
            let q = w.ast.decls.get(d).ty().expect("a type");
            let TypeView::Function(p) = w.ast.types.view(q.ty()) else { panic!("a function") };
            let mut r = p.ret();
            loop {
                match w.ast.types.view(r.ty()) {
                    TypeView::Decltype { expr, underlying: _ } => return expr,
                    TypeView::Paren(x) => r = x,
                    v => panic!("{v:?}"),
                }
            }
        };
        let found = |w: &Walk<'_>, e: ExprId| -> usize {
            let n = w.ast.exprs.get(e);
            let callee = ExprId::try_new(n.a()).expect("a callee");
            let mut c = callee;
            while w.ast.exprs.get(c).kind()
                != slip_kiln::kiln::ast::expr::ExprKind::UnresolvedLookupExpr
            {
                c = ExprId::try_new(w.ast.exprs.get(c).a()).expect("a child");
            }
            slip_kiln::kiln::ast::ListId::try_new(w.ast.exprs.get(c).b())
                .map_or(0, |l| w.ast.lists.iter::<DeclId>(l).count())
        };
        for name in ["m", "n"] {
            let ds = crate::decl_func::tests::named(w, name, DeclKind::CXXMethod);
            assert_eq!(ds.len(), 2, "{name}: the pattern and the member of A<int>");
            let (a, b) = (ret_expr(w, ds[0]), ret_expr(w, ds[1]));
            assert_ne!(a, b, "{name}: a fresh expression per substitution");
            assert_eq!(found(w, a), 1, "{name}: f(int) alone, at the definition");
            assert_eq!(found(w, b), 1, "{name}: the instantiation keeps the set");
        }
    });
}

/// Rule 4 with a cause (N3): an alias whose own aliased type as written
/// is indeterminate (a fold over `+`, with an `operator+` declared, in an
/// instantiation-dependent argument: rule 7's profile) is an `Alias` entry
/// of its user, whose own type holds only the argument's value.
#[test]
fn an_alias_whose_spelling_is_indeterminate_is_listed() {
    let src = format!(
        "{PRELUDE}template<class... T> using R = Z<sizeof(sizeof((T() + ...)))>;\n\
         template<class... T> void u(R<T...> r);\n"
    );
    crate::decl_func::tests::with_walk(&src, |w| {
        let alias_r = crate::decl_func::tests::named(w, "R", DeclKind::TypeAliasTemplate)[0];
        let pattern = w.ast.template_info(alias_r).and_then(|i| i.pattern).expect("R's pattern");
        let under = w.ast.decls.typedef(pattern).expect("R").underlying;
        w.unit = crate::walk::UnitState::default();
        let mut s = Stream::new();
        super::left_out_of_type(w, &mut s, under, None);
        assert_eq!(s.cause(), Some(Cause::Unaccounted("spelling/fold-operator")));
        let d = crate::decl_func::tests::named(w, "u", DeclKind::Function)[0];
        let q = w.ast.decls.get(d).ty().expect("a type");
        w.unit = crate::walk::UnitState::default();
        w.alias_due.clear();
        super::left_out_of_type(w, &mut Stream::new(), q, None);
        assert_eq!(w.unit.facts, vec![Fact::Alias(w.ast.first(pattern))]);
        assert!(w.unit.facts_due);
    });
}

/// Rule 4: an alias whose own spelling makes no list due but carries a
/// cause (a canonical operand, rule 5) is an `Alias` entry too: its
/// user is determinate, and the alias's unit is the indeterminate one.
#[test]
fn an_alias_whose_only_spelling_is_a_cause_is_listed() {
    let src = format!(
        "{PRELUDE}template<class T> using R = void (*)(decltype(f(T())) [2]);\n\
         template<class T> void u(T, R<T> r);\n"
    );
    crate::decl_func::tests::with_walk(&src, |w| {
        let alias_r = crate::decl_func::tests::named(w, "R", DeclKind::TypeAliasTemplate)[0];
        let pattern = w.ast.template_info(alias_r).and_then(|i| i.pattern).expect("R's pattern");
        let under = w.ast.decls.typedef(pattern).expect("R").underlying;
        w.unit = crate::walk::UnitState::default();
        let mut s = Stream::new();
        super::left_out_of_type(w, &mut s, under, None);
        assert_eq!(s.cause(), Some(Cause::Unaccounted("spelling/decltype")));
        assert!(!w.unit.facts_due, "{:?}", w.unit.facts);
        let d = crate::decl_func::tests::named(w, "u", DeclKind::Function)[0];
        let q = w.ast.decls.get(d).ty().expect("a type");
        w.unit = crate::walk::UnitState::default();
        w.alias_due.clear();
        let mut s = Stream::new();
        super::left_out_of_type(w, &mut s, q, None);
        assert_eq!(w.unit.facts, vec![Fact::Alias(w.ast.first(pattern))]);
        assert_eq!(s.cause(), None);
    });
}

/// Rule 4: a use through an alias writes the alias's `Ref`, not its set:
/// adding a declaration the alias finds changes the alias's unit and not
/// its user's.
#[test]
fn an_alias_holds_its_set_and_its_user_the_ref() {
    let head = "int f(int);\n";
    let alias = "template<class T> using R = decltype(f(T()));\n";
    let user = "template<class T> void u(T t, R<T> r);\n";
    let more = "long f(long);\n";
    let a = units(&format!("{head}{alias}{more}{user}"));
    let b = units(&format!("{head}{more}{alias}{user}"));
    let changed: Vec<&String> = a
        .iter()
        .filter(|(n, u)| b.get(*n).is_some_and(|v| v.digest != u.digest))
        .map(|(n, _)| n)
        .collect();
    assert_eq!(changed.len(), 1, "{changed:?}");
    assert!(changed[0].starts_with('R'), "{changed:?}");
}

/// The sites of rule 5 (and its kin) a pair test in `indeterminate/_/`
/// ends at (`a_list_changes_only_the_units_that_write_one`); the landed
/// file names the test of each.
const REACHED: &[&str] = &[
    "spelling/decltype",
    "spelling/array-bound",
    "spelling/noexcept",
    "spelling/arg",
    "spelling/vector-size",
    "spelling/addr-space",
    "spelling/pack-index",
    "spelling/bit-int",
    "spelling/profile",
    "spelling/conversion-name",
    "spelling/fold-operator",
    "spelling/full",
    "spelling/using-enum",
];

/// The sources tried for rule 5 through the digest, aimed at a caller
/// that holds only a canonical type, and the `spelling/` causes each
/// reaches; the first thirteen are those of the first round, the rest aim
/// at the sites no pair test reaches (`typeof`, `deduced`, `subst-pack`,
/// `full`). The landed file counts them.
const RULE_5_TRIED: &[(&str, &[&str])] = &[
    ("template<class T> struct S { S(T, char(*)[sizeof(f(T()))]); };\nS s(1, nullptr);\n", &[]),
    ("template<class T> struct S { operator decltype(f(T()))(); };\n", &[]),
    ("template<class T> void k(T t) { using V = decltype(f(t)); V* p; }\n", &[]),
    ("template<class T> void k(T t) { auto p = (char(*)[sizeof(f(T()))])nullptr; }\n", &[]),
    (
        "template<class T> struct B { B(decltype(f(T()))); };\n\
         template<class T> struct D : B<T> { using B<T>::B; };\n",
        &[],
    ),
    (
        "template<class T> struct S { template<class U> auto m(U u) -> decltype(f(u)); };\n\
         template struct S<int>;\n",
        &[],
    ),
    (
        "template<class T> auto g(T t) -> decltype(f(t));\n\
         template<class T> void k(T t) { g(t); decltype(g(t)) *p; }\n",
        &[],
    ),
    ("template<class T> struct S { static constexpr auto v = sizeof(f(T())); char a[v]; };\n", &[]),
    (
        "template<class T> auto g(T t) -> decltype(f(t));\n\
         template<class T> struct S { decltype(g(T())) m; };\n",
        &[],
    ),
    (
        "template<class T> constexpr auto v = sizeof(f(T()));\n\
         template<class T> struct S { char a[v<T>]; };\n",
        &[],
    ),
    ("template<class T> auto g(T t) { return Z<sizeof(f(t))>(); }\n", &[]),
    (
        "template<class T, unsigned long N = sizeof(f(T()))> struct Q {};\n\
         template<class T> void k(Q<T>);\n",
        &[],
    ),
    (
        "template<class T> struct S { void m(char(*)[sizeof(f(T()))]); };\n\
         template<class T> void S<T>::m(char(*)[sizeof(f(T()))]) {}\n",
        &[],
    ),
    // typeof
    ("template<class T> void k(T, void (*)(__typeof__(f(T())) [2]));\n", &[]),
    ("template<class T> void k(T, void (*)(__typeof__(f(T())) [2][3]));\n", &[]),
    ("template<class T> void k(T t) { __typeof__(f(t)) a[2]; }\n", &[]),
    // deduced
    ("template<class T> auto k(T t) { auto x = f(t); return x; }\n", &[]),
    ("template<class T> void k(T t) { auto (*p)(T) = &k<T>; decltype(auto) y = f(t); }\n", &[]),
    ("template<class... T> void k(void (*)(T...[0] [2]), T...);\n", &[]),
    (
        "template<class T> void k(T, void (*)(__underlying_type(decltype(f(T()))) [2]));\n",
        &["spelling/decltype"],
    ),
    // subst-pack
    (
        "template<class... T> struct P { template<class U> void m(U, void (*)(decltype(f(U()) + T())...)); };\n\
         template struct P<int, long>;\n",
        &[],
    ),
    (
        "template<class... T> struct P { template<class... U> void m(void (*)(decltype(f(U())) *...)); };\n\
         template struct P<int>;\n",
        &[],
    ),
    // full
    ("template<class T> void k(T, void (*)(Z<sizeof(f(T()) + 1_x)> [2]));\n", &["spelling/arg"]),
    ("template<class T> void k(T, void (*)(Z<sizeof([] { return 0; }, f(T()))> [2]));\n", &[]),
];

/// Rule 5 through the digest: each source tried reaches the causes listed
/// for it, and no other.
#[test]
fn the_sources_tried_for_rule_5_reach_what_is_listed() {
    let mut bad = Vec::new();
    for (c, want) in RULE_5_TRIED {
        let u = units(&format!("{PRELUDE}long operator\"\"_x(unsigned long long);\n{c}"));
        let mut got: Vec<&str> = u
            .values()
            .flat_map(|u| u.causes.iter())
            .filter_map(|c| match c {
                Cause::Unaccounted(w) if w.starts_with("spelling/") => Some(*w),
                _ => None,
            })
            .collect();
        got.sort_unstable();
        got.dedup();
        if got != *want {
            bad.push(format!("{c:?}: {got:?}"));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// Rule 5 at the walk: a canonical type handed to the reading (a caller
/// that holds no written type) is `Unaccounted("spelling/<site>")` when
/// it is marked, and determinate when it is not; the written type of the
/// same declaration is read.
#[test]
fn a_marked_canonical_operand_is_unaccounted() {
    let src = "int f(int);\nlong f(long);\ntemplate<unsigned long N> struct Z {};\n\
               template<class T> auto a(T t) -> decltype(f(t));\n\
               template<class T> void b(T t, char (*p)[sizeof(f(T()))]);\n\
               template<class T> void c(T t, void (*p)() noexcept(noexcept(f(T()))));\n\
               template<class T> void d(T t, Z<sizeof(f(T()))> z);\n\
               template<class T> auto e(T t) -> decltype(t.m);\n";
    crate::decl_func::tests::with_walk(src, |w| {
        for (name, site) in [
            ("a", Some("spelling/decltype")),
            ("b", Some("spelling/array-bound")),
            ("c", Some("spelling/noexcept")),
            ("d", Some("spelling/arg")),
            ("e", None),
        ] {
            let d = crate::decl_func::tests::named(w, name, DeclKind::Function)[0];
            let q = w.ast.decls.get(d).ty().expect("a type");
            let canon = w.ast.types.canonical(q);
            let mut s = Stream::new();
            w.unit = crate::walk::UnitState::default();
            super::left_out_of_type(w, &mut s, canon, None);
            assert_eq!(s.cause(), site.map(Cause::Unaccounted), "{name}");
            let mut s = Stream::new();
            w.unit = crate::walk::UnitState::default();
            super::left_out_of_type(w, &mut s, q, None);
            assert_eq!(s.cause(), None, "{name}: its written type");
            assert_eq!(w.unit.facts_due, site.is_some(), "{name}");
        }
    });
}

/// Rule 4: the user's list holds the alias's `Ref` (where the fact is
/// written), and the alias's own list its set; an alias whose spelling
/// holds no set is not in the list.
#[test]
fn a_use_through_an_alias_lists_the_alias() {
    let src = "int f(int);\n\
               template<class T> using R = decltype(f(T()));\n\
               template<class T> using P = T*;\n\
               template<class T> struct S { using M = decltype(f(T())); void u(M m, P<T> p); };\n\
               template<class T> void u(T t, R<T> r, P<T> p);\n";
    crate::decl_func::tests::with_walk(src, |w| {
        let alias_r = crate::decl_func::tests::named(w, "R", DeclKind::TypeAliasTemplate)[0];
        let pattern = w.ast.template_info(alias_r).and_then(|i| i.pattern).expect("R's pattern");
        let member = crate::decl_func::tests::named(w, "M", DeclKind::TypeAlias)[0];
        for (kind, alias) in [(DeclKind::Function, pattern), (DeclKind::CXXMethod, member)] {
            let d = crate::decl_func::tests::named(w, "u", kind)[0];
            let q = w.ast.decls.get(d).ty().expect("a type");
            w.unit = crate::walk::UnitState::default();
            let mut s = Stream::new();
            // As a type, not as `u`'s own (whose parameters their
            // declarations' units read).
            super::left_out_of_type(w, &mut s, q, None);
            assert_eq!(s.cause(), None);
            assert_eq!(w.unit.facts, vec![Fact::Alias(w.ast.first(alias))], "{kind:?}");
            assert!(w.unit.facts_due);
        }
        // The alias's own unit reads its aliased type.
        let under = w.ast.decls.typedef(member).expect("M").underlying;
        w.unit = crate::walk::UnitState::default();
        super::left_out_of_type(w, &mut Stream::new(), under, None);
        assert!(matches!(w.unit.facts.as_slice(), [Fact::Found(ds)] if ds.len() == 1));
    });
}
