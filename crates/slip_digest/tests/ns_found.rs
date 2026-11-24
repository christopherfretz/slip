//! WP-141 (Q10 of the ruling): what Kiln keeps of a lookup that names a
//! namespace to find members, which R1 (`spec/digest.md` §3.4, "A
//! reference to a namespace") reads.
//!
//! 1. The found set of a namespace-qualified unresolved name holds the
//!    entities, never the using-shadows: a name found through a
//!    using-declaration in the namespace (`namespace acme { using
//!    std::forward; }`) is held as `std::forward` itself, as one found
//!    through a using-directive is. A using-declaration's own shadows name
//!    the entity too. The route (which part of the namespace held the
//!    name) is not kept: the addendum's A1 cannot be built on it as
//!    stated (reported to the supervisor; Kiln's lookup has the shadows,
//!    `kiln_sema` strips them when it builds the expression).
//! 2. The found set is the definition's: an instantiation does not look
//!    the qualified name up again (a later overload is not a candidate).

use slip_kiln::kiln::ast::decl::DeclKind;
use slip_kiln::kiln::ast::expr::ExprKind;
use slip_kiln::kiln::ast::{Ast, DeclId, ExprId, ListId};

/// Runs `f` on the AST of `src` (C++20), which must parse without error.
fn with_ast<R>(src: &str, f: impl FnOnce(&Ast) -> R) -> R {
    let files = [("main.cpp", src)];
    let snippet = slip_kiln::snippet::Snippet {
        files: &files,
        main: "main.cpp",
        include: &[""],
        std: "c++20",
    };
    slip_kiln::snippet::parse(&snippet, |p| {
        assert_eq!(p.errors, 0, "{src}\n{}", p.diagnostics);
        f(p.ast)
    })
    .expect("the snippet parses")
}

/// The found set of every qualified `UnresolvedLookupExpr`, as
/// `(kind, display name of the declaration it stands for)`.
fn found_sets(ast: &Ast) -> Vec<Vec<(DeclKind, String)>> {
    let mut out = Vec::new();
    for i in 1..=ast.exprs.len() {
        let Ok(i) = u32::try_from(i) else { break };
        let e = ExprId::new(i);
        let n = ast.exprs.get(e);
        if n.kind() != ExprKind::UnresolvedLookupExpr || ast.templates.expr_nns.get(e).is_none() {
            continue;
        }
        let found: Vec<DeclId> = ListId::try_new(n.b())
            .map(|l| ast.lists.iter::<DeclId>(l).collect())
            .unwrap_or_default();
        let mut set: Vec<(DeclKind, String)> = found
            .iter()
            .map(|&d| {
                (ast.decls.get(d).kind(), slip_digest::display_name(ast, ast.strip_shadow(d)))
            })
            .collect();
        set.sort_by(|a, b| a.1.cmp(&b.1).then(format!("{:?}", a.0).cmp(&format!("{:?}", b.0))));
        out.push(set);
    }
    out
}

const STD: &str = "namespace std {\n\
    struct string {};\n\
    template <class T> T&& forward(T& t) noexcept;\n\
    template <class T> T&& forward(T&& t) noexcept;\n\
    }\n";

/// Q10 point 1: through an owned re-export `namespace acme { using
/// std::forward; }` the found set holds the entities, not the shadows.
#[test]
fn found_set_through_a_using_declaration_holds_the_entities() {
    let src = format!(
        "{STD}namespace acme {{ using std::forward; }}\n\
         template <class T> T g(T t) {{ return acme::forward<T>(t); }}\n"
    );
    let sets = with_ast(&src, found_sets);
    assert_eq!(sets.len(), 1, "{sets:?}");
    assert_eq!(sets[0].len(), 2, "{sets:?}");
    assert!(
        sets[0].iter().all(|(k, n)| *k == DeclKind::FunctionTemplate && n == "std::forward"),
        "{sets:?}"
    );
}

/// Q10 point 1: with owned overloads beside the re-export, the set holds
/// the re-exported entities and the member, no shadow.
#[test]
fn found_set_holds_entities_and_members() {
    let src = format!(
        "{STD}namespace acme {{ using std::forward; template <class T> T forward(T t, int); }}\n\
         template <class T> T g(T t) {{ return acme::forward<T>(t); }}\n"
    );
    let sets = with_ast(&src, found_sets);
    assert_eq!(sets.len(), 1, "{sets:?}");
    let kinds: Vec<DeclKind> = sets[0].iter().map(|x| x.0).collect();
    assert_eq!(kinds.len(), 3, "{sets:?}");
    assert!(kinds.iter().all(|&k| k == DeclKind::FunctionTemplate), "{sets:?}");
    let names: Vec<&str> = sets[0].iter().map(|x| x.1.as_str()).collect();
    assert_eq!(names, ["acme::forward", "std::forward", "std::forward"], "{sets:?}");
}

/// Q10 point 1: through a using-directive written in the namespace, the
/// set holds the entities, as through a using-declaration.
#[test]
fn found_set_through_a_using_directive_holds_the_entities() {
    let src = format!(
        "{STD}namespace acme {{ using namespace std; }}\n\
         template <class T> T g(T t) {{ return acme::forward<T>(t); }}\n"
    );
    let sets = with_ast(&src, found_sets);
    assert_eq!(sets.len(), 1, "{sets:?}");
    assert_eq!(sets[0].len(), 2, "{sets:?}");
    assert!(sets[0].iter().all(|(k, _)| *k == DeclKind::FunctionTemplate), "{sets:?}");
}

/// Q10 point 1, row 3: the shadows of `using acme::string;`, where `acme`
/// re-exports `std::string`, name `std::string` itself: a
/// using-declaration's shadow does not keep the route.
#[test]
fn a_using_declaration_shadow_names_the_entity() {
    let src = format!("{STD}namespace acme {{ using std::string; }}\nusing acme::string;\n");
    let targets = with_ast(&src, |ast| {
        let mut out = Vec::new();
        for d in ast.decls.ids() {
            if ast.decls.get(d).kind() != DeclKind::Using || ast.decls.get(d).ctx().is_none() {
                continue;
            }
            for s in ast.using_shadows(d) {
                let t = ast.using_target(s).expect("a shadow has a target");
                out.push((
                    slip_digest::display_name(ast, d),
                    ast.decls.get(t).kind(),
                    slip_digest::display_name(ast, t),
                ));
            }
        }
        out
    });
    assert!(!targets.is_empty());
    assert!(targets.iter().all(|(_, k, _)| *k != DeclKind::UsingShadow), "{targets:?}");
}

/// Q10 point 2: the found set is the definition's. `g(char)`, declared
/// after the template, is in no found set and is called by no
/// instantiation: `f('c')` calls `g(int)`.
#[test]
fn the_found_set_is_not_looked_up_again_at_instantiation() {
    let src = "namespace acme { int g(int); int g(long); }\n\
               template <class T> int f(T t) { return acme::g(t); }\n\
               namespace acme { int g(char); }\n\
               int use = f('c');\n";
    with_ast(src, |ast| {
        let late: Vec<DeclId> = ast
            .decls
            .ids()
            .filter(|&d| {
                ast.decls.get(d).kind() == DeclKind::Function
                    && slip_digest::display_name(ast, d) == "acme::g(char)"
            })
            .collect();
        assert_eq!(late.len(), 1);
        let sets = found_sets(ast);
        assert!(!sets.is_empty());
        for s in &sets {
            assert_eq!(s.len(), 2, "{sets:?}");
            assert!(s.iter().all(|(_, n)| n != "acme::g(char)"), "{sets:?}");
        }
        let mut called = Vec::new();
        for i in 1..=ast.exprs.len() {
            let Ok(i) = u32::try_from(i) else { break };
            let n = ast.exprs.get(ExprId::new(i));
            if n.kind() == ExprKind::DeclRefExpr
                && let Some(d) = DeclId::try_new(n.a())
                && ast.decls.get(d).kind() == DeclKind::Function
            {
                called.push(slip_digest::display_name(ast, d));
            }
        }
        assert!(called.iter().any(|n| n == "acme::g(int)"), "{called:?}");
        assert!(called.iter().all(|n| n != "acme::g(char)"), "{called:?}");
    });
}
