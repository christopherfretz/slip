//! The order unit (`spec/digest.md` §2.1): the keys of the variables with
//! static or thread storage duration defined at namespace or class scope,
//! in the order of their definitions, then the module units the
//! translation unit imports (`spec/digest-kinds.md` §10.7 `imports`).
//!
//! The definitions are found by a walk of the lexical member lists from
//! the translation unit, the walk Kiln's code generation makes
//! (`kiln_codegen::module::emit_members`): into namespaces, linkage
//! specifications, `export` blocks and classes written in source, not
//! into function bodies (whose variables are not at namespace or class
//! scope), templates (whose patterns define nothing) or instantiations.
//! An instantiated variable is not listed: its dynamic initialisation is
//! unordered ([basic.start.dynamic]/1). An explicit specialisation is
//! written in source and is listed.

use std::collections::HashSet;

use slip_kiln::kiln::ast::DeclId;
use slip_kiln::kiln::ast::decl::{DeclFlags, DeclKind, TemplLinkKind};

use crate::Cause;
use crate::stream::Stream;
use crate::walk::Walk;

/// An import of a header unit, whose key is a path: the path rule of
/// digest §9 is not applied to it yet.
const HEADER_IMPORT: Cause = Cause::Unaccounted("UnitInfo/imports");
/// Scopes deeper than this are not walked (an AST is far shallower).
const DEPTH_LIMIT: u32 = 1024;
/// A scope nested past `DEPTH_LIMIT`.
const TOO_DEEP: Cause = Cause::Unaccounted("order/depth");
/// A member of a kind Kiln does not make (`ClassTemplateSpecialization`,
/// `VarTemplateSpecialization`: a specialisation is a `CXXRecord` or a
/// `Var`, `kiln_ast/src/decl.rs:74`, `:117`).
const KIND_UNACCOUNTED: Cause = Cause::Unaccounted("order/kind");

/// The order unit's stream: `count` and the `key` of each variable, then
/// `count` and, for each import of the translation unit's own
/// `UnitInfo` in stored order, its key as `bytes` and `bool` for
/// `export import`.
pub(crate) fn write(w: &mut Walk<'_>, s: &mut Stream) {
    let ast = w.ast;
    let mut vars = Vec::new();
    collect(w, s, ast.tu, 0, &mut vars);
    s.count(vars.len());
    for v in vars {
        w.write_key(s, v);
    }
    imports(w, s);
}

/// The variables defined in the lexical members of `ctx`, in order, and
/// of the scopes among them that are walked.
fn collect(w: &Walk<'_>, s: &mut Stream, ctx: DeclId, depth: u32, out: &mut Vec<DeclId>) {
    // The holding variables of tuple-like bindings, once.
    let holding: HashSet<DeclId> = w.ast.decls.cold.binding_var.iter().map(|(_, &h)| h).collect();
    members(w, s, &holding, ctx, depth, out);
}

/// What the walk does with a member.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Visit {
    /// A scope whose members are walked.
    Enter,
    /// A variable declaration, listed if `listed` says so.
    Var,
    /// Neither.
    Skip,
}

/// `collect` over the members of `ctx`.
fn members(
    w: &Walk<'_>,
    s: &mut Stream,
    holding: &HashSet<DeclId>,
    ctx: DeclId,
    depth: u32,
    out: &mut Vec<DeclId>,
) {
    if depth > DEPTH_LIMIT {
        s.indeterminate(TOO_DEEP);
        return;
    }
    let ast = w.ast;
    for m in ast.decls.members(ctx) {
        match visit(s, ast.decls.get(m).kind()) {
            Visit::Enter => members(w, s, holding, m, depth + 1, out),
            Visit::Var => {
                if listed(w, holding, m) {
                    out.push(m);
                }
            }
            Visit::Skip => {}
        }
    }
}

/// The walk Kiln's code generation makes (`kiln_codegen/src/module.rs:657`):
/// into namespaces, the transparent scopes and classes, not into function
/// bodies (whose variables are not at namespace or class scope) nor
/// templates. A class that is a template's pattern or an instantiation is
/// entered: `listed` leaves out what is declared in it.
fn visit(s: &mut Stream, kind: DeclKind) -> Visit {
    match kind {
        DeclKind::Namespace | DeclKind::LinkageSpec | DeclKind::Export | DeclKind::CXXRecord => {
            Visit::Enter
        }
        DeclKind::Var => Visit::Var,
        DeclKind::ClassTemplateSpecialization | DeclKind::VarTemplateSpecialization => {
            s.indeterminate(KIND_UNACCOUNTED);
            Visit::Skip
        }
        DeclKind::TranslationUnit
        | DeclKind::NamespaceAlias
        | DeclKind::Empty
        | DeclKind::FileScopeAsm
        | DeclKind::UsingDirective
        | DeclKind::Using
        | DeclKind::UsingEnum
        | DeclKind::UsingShadow
        | DeclKind::ConstructorUsingShadow
        | DeclKind::UsingPack
        | DeclKind::UnresolvedUsingValue
        | DeclKind::UnresolvedUsingTypename
        | DeclKind::UnresolvedUsingIfExists
        | DeclKind::Typedef
        | DeclKind::TypeAlias
        | DeclKind::Enum
        | DeclKind::EnumConstant
        | DeclKind::ClassTemplatePartialSpecialization
        | DeclKind::Field
        | DeclKind::IndirectField
        | DeclKind::AccessSpec
        | DeclKind::Friend
        | DeclKind::FriendTemplate
        | DeclKind::Function
        | DeclKind::CXXMethod
        | DeclKind::CXXConstructor
        | DeclKind::CXXDestructor
        | DeclKind::CXXConversion
        | DeclKind::CXXDeductionGuide
        | DeclKind::ParmVar
        | DeclKind::ImplicitParam
        | DeclKind::Binding
        | DeclKind::VarTemplatePartialSpecialization
        | DeclKind::ClassTemplate
        | DeclKind::FunctionTemplate
        | DeclKind::VarTemplate
        | DeclKind::TypeAliasTemplate
        | DeclKind::Concept
        | DeclKind::BuiltinTemplate
        | DeclKind::TemplateTypeParm
        | DeclKind::NonTypeTemplateParm
        | DeclKind::TemplateTemplateParm
        | DeclKind::ImplicitConceptSpecialization
        | DeclKind::RequiresExprBody
        | DeclKind::LifetimeExtendedTemporary
        | DeclKind::TemplateParamObject
        | DeclKind::StaticAssert
        | DeclKind::Label
        | DeclKind::ContractResultName
        | DeclKind::ConstevalBlock => Visit::Skip,
    }
}

/// Whether the variable declaration `m` is listed: a definition
/// (`units::is_var_definition`, tested first: it reads `m` alone), not
/// the holding variable of a tuple-like binding (initialised with the
/// decomposition, which is listed), not in a template, not made by
/// instantiation (`templ::instantiated_declaration`: an explicit specialisation and
/// its members are written in source), and the definition of its entity
/// (§2.3).
fn listed(w: &Walk<'_>, holding: &HashSet<DeclId>, m: DeclId) -> bool {
    if !crate::units::is_var_definition(w, m) {
        return false;
    }
    if w.ast.decls.get(m).has(DeclFlags::IMPLICIT) && holding.contains(&m) {
        return false;
    }
    if templated(w, m) || crate::templ::instantiated_declaration(w, m) {
        return false;
    }
    crate::units::defining_decl(w, w.entity(m)) == Some(m)
}

/// `m` is a template's pattern, a partial specialisation or a
/// declaration in a dependent context.
fn templated(w: &Walk<'_>, m: DeclId) -> bool {
    let ast = w.ast;
    if ast.decls.get(m).has(DeclFlags::DEPENDENT_CTX) {
        return true;
    }
    match ast.templ_link(m).get() {
        Some((TemplLinkKind::Pattern | TemplLinkKind::PartialSpecialization, _)) => true,
        Some((TemplLinkKind::Specialization | TemplLinkKind::MemberOfSpecialization, _)) | None => {
            false
        }
    }
}

/// `UnitInfo` `imports` of the translation unit itself (`units[0]`,
/// §10.7): empty when it has none or ownership is not tracked.
fn imports(w: &Walk<'_>, s: &mut Stream) {
    let modules = &w.ast.modules;
    let list: &[(String, bool)] = if modules.active {
        modules.units.first().map_or(&[], |u| u.imports.as_slice())
    } else {
        &[]
    };
    s.count(list.len());
    for (key, export) in list {
        if key.starts_with('"') {
            s.indeterminate(HEADER_IMPORT);
        }
        s.bytes(key.as_bytes()).bool(*export);
    }
}

#[cfg(test)]
mod tests {
    use slip_kiln::kiln::ast::decl::{DeclFlags, DeclKind};

    use crate::decl_func::tests::{named, with_walk};
    use crate::stream::Stream;

    /// The names of the variables the order unit lists for `src`.
    fn listed(src: &str) -> Vec<String> {
        with_walk(src, |w| {
            let mut out = Vec::new();
            let mut s = Stream::new();
            super::collect(w, &mut s, w.ast.tu, 0, &mut out);
            assert_eq!(s.cause(), None);
            out.into_iter()
                .map(|d| crate::names::entity_name(w.ast, d, slip_model::keys::KeyKind::Variable))
                .collect()
        })
    }

    // digest §2.1: definitions in order, at namespace and class scope,
    // through namespaces, linkage specifications and classes.
    #[test]
    fn definitions_are_listed_in_order() {
        let src = "extern int b;\n\
                   int a = 1;\n\
                   namespace N { int c; extern \"C\" { int d; } }\n\
                   struct S { static int s; static inline int t = 2; static const int k = 3; };\n\
                   int b = 2;\n\
                   int S::s = 4;\n\
                   void f() { static int local = 5; }\n\
                   thread_local int tl;\n";
        assert_eq!(listed(src), ["a", "N::c", "N::d", "S::t", "b", "S::s", "tl"]);
    }

    // digest §2.1: no template's pattern, no instantiation; an explicit
    // specialisation is written in source.
    #[test]
    fn instantiations_are_not_listed() {
        let src = "template <class T> struct A { static int s; static inline int t = 1; };\n\
                   template <class T> int A<T>::s = 0;\n\
                   template <class T> int v = 1;\n\
                   template <> int v<char> = 2;\n\
                   template <> int A<char>::s = 3;\n\
                   template <> struct A<short> { static inline int u = 4; };\n\
                   template struct A<long>;\n\
                   template int v<long>;\n\
                   int use() { return A<int>::s + A<int>::t + v<int>; }\n";
        let got = listed(src);
        assert_eq!(got.len(), 3, "{got:?}");
        assert!(got[0].starts_with('v'), "{got:?}");
        assert!(got[1].ends_with("::s"), "{got:?}");
        assert!(got[2].ends_with("::u"), "{got:?}");
    }

    // digest §2.1: a structured binding is listed once, by its
    // decomposition variable; a tuple-like binding's holding variables
    // are implicit and initialised with it.
    #[test]
    fn a_structured_binding_is_listed_once() {
        let src = "struct P { int x, y; };\nP p;\nauto [a, b] = p;\n";
        assert_eq!(listed(src).len(), 2);
    }

    // digest §2.1: an explicit specialisation of a member class is
    // written in source, and so are its static members, although its
    // entity is declared in the implicit instantiation `A<int>`.
    #[test]
    fn members_of_a_member_class_explicit_specialisation_are_listed() {
        let src = "int h(); template <class T> struct A { struct N; };\n\
                   template <> struct A<int>::N { static inline int z = h(); };\n\
                   int g = h();\n";
        let got = listed(src);
        assert_eq!(got.len(), 2, "{got:?}");
        assert!(got[0].ends_with("::z"), "{got:?}");
        assert_eq!(got[1], "g");
    }

    // [temp.expl.spec]/14: an explicit specialisation of a static data
    // member without an initialiser is a declaration; the one with it is
    // listed, where it is.
    #[test]
    fn a_member_explicit_specialisation_without_initialiser_is_not_listed() {
        let src = "int h(); template <class T> struct A { static int s; };\n\
                   template <> int A<char>::s;\n\
                   int g = h();\n\
                   template <> int A<char>::s = h();\n";
        let got = listed(src);
        assert_eq!(got.len(), 2, "{got:?}");
        assert_eq!(got[0], "g");
        assert!(got[1].ends_with("::s"), "{got:?}");
    }

    // digest §2.1: the object of a namespace-scope anonymous union is an
    // implicit variable that is listed; a tuple-like binding's holding
    // variables are not.
    #[test]
    fn an_anonymous_union_object_is_listed() {
        with_walk("int h(); static union { int a = h(); }; int g = h();\n", |w| {
            let mut out = Vec::new();
            let mut s = Stream::new();
            super::collect(w, &mut s, w.ast.tu, 0, &mut out);
            assert_eq!(out.len(), 2, "{out:?}");
            assert!(w.ast.decls.get(out[0]).has(DeclFlags::IMPLICIT));
        });
    }

    // digest §2.1: the stream holds the variables in the order of their
    // definitions, `b` declared before `a` and defined after it, then no
    // imports.
    #[test]
    fn the_stream_is_the_keys_in_definition_order() {
        let src = "extern int b;\nint a = 1;\nint b = 2;\nnamespace N { int c = 3; }\n";
        with_walk(src, |w| {
            let var = |n: &str| {
                let all = named(w, n, DeclKind::Var);
                *all.last().expect("declared")
            };
            let (a, b, c) = (var("a"), var("b"), var("c"));
            let mut want = Stream::new();
            want.count(3);
            for v in [a, b, c] {
                w.write_key(&mut want, v);
            }
            want.count(0);
            let mut got = Stream::new();
            super::write(w, &mut got);
            assert_eq!(got.as_bytes(), want.as_bytes());
            assert_eq!(got.cause(), None);
        });
    }

    // A variable declared many times costs one walk of its chain, not
    // one per declaration: 2,000 declarations against 200 take about
    // ten times as long, not a hundred.
    #[test]
    fn redeclarations_cost_linear_time() {
        fn time(n: usize) -> std::time::Duration {
            let src = format!("{}int x = 1;\n", "extern int x;\n".repeat(n));
            with_walk(&src, |w| {
                (0..5)
                    .map(|_| {
                        let start = std::time::Instant::now();
                        for _ in 0..20 {
                            let mut out = Vec::new();
                            let mut s = Stream::new();
                            super::collect(w, &mut s, w.ast.tu, 0, &mut out);
                            assert_eq!(out.len(), 1);
                        }
                        start.elapsed()
                    })
                    .min()
                    .expect("runs")
            })
        }
        let small = time(200);
        let large = time(2_000);
        assert!(large < small * 40, "200: {small:?}, 2000: {large:?}");
    }

    // digest §2.1: the imports of a translation unit without modules.
    #[test]
    fn no_modules_no_imports() {
        with_walk("int x;", |w| {
            let mut s = Stream::new();
            super::imports(w, &mut s);
            assert_eq!(s.as_bytes(), &[0]);
        });
    }
}
