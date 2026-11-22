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

use slip_kiln::kiln::ast::DeclId;
use slip_kiln::kiln::ast::decl::{DeclFlags, DeclKind, TemplLinkKind};
use slip_kiln::kiln::ast::templates::SpecKind;

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
    if depth > DEPTH_LIMIT {
        s.indeterminate(TOO_DEEP);
        return;
    }
    let ast = w.ast;
    for m in ast.decls.members(ctx) {
        if listed(w, m) {
            out.push(m);
        }
        if walked(w, m) {
            collect(w, s, m, depth + 1, out);
        }
    }
}

/// Whether the member `m` is a scope whose members the walk reads: a
/// namespace, a transparent scope, or a class written in source (not a
/// template's pattern or partial specialisation, not an instantiation).
fn walked(w: &Walk<'_>, m: DeclId) -> bool {
    let ast = w.ast;
    let kind = ast.decls.get(m).kind();
    if kind == DeclKind::Namespace || kind == DeclKind::LinkageSpec || kind == DeclKind::Export {
        return true;
    }
    (kind == DeclKind::CXXRecord || kind == DeclKind::ClassTemplateSpecialization)
        && ast.decls.record(m).is_some_and(|r| r.lambda.is_none())
        && !templated(w, m)
        && !instantiated(w, m)
}

/// Whether the member `m` is the definition of a variable the order
/// unit lists: a variable or a variable template's explicit
/// specialisation, written in source, not in a template, not made by
/// instantiation, and the definition of its entity (§2.3).
fn listed(w: &Walk<'_>, m: DeclId) -> bool {
    let ast = w.ast;
    let n = ast.decls.get(m);
    if n.kind() != DeclKind::Var && n.kind() != DeclKind::VarTemplateSpecialization {
        return false;
    }
    if n.has(DeclFlags::IMPLICIT) || templated(w, m) || instantiated(w, m) {
        // An implicit variable (the holding variable of a tuple-like
        // binding) is initialised with the declaration that made it.
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
        Some((
            TemplLinkKind::Pattern
            | TemplLinkKind::PartialSpecialization
            | TemplLinkKind::DependentFriend,
            _,
        )) => true,
        Some((TemplLinkKind::Specialization | TemplLinkKind::MemberOfSpecialization, _)) | None => {
            false
        }
    }
}

/// `m` was made by instantiation, of any form: the kind of its own
/// `SpecInfo` or `MemberSpecInfo`, else of the nearest enclosing context
/// that has one. An explicit specialisation, and a member of one, are
/// written in source.
fn instantiated(w: &Walk<'_>, m: DeclId) -> bool {
    let ast = w.ast;
    let mut c = Some(w.entity(m));
    for _ in 0..DEPTH_LIMIT {
        let Some(x) = c else { return false };
        let kind = ast
            .spec_info(x)
            .map(|(_, i)| i.kind)
            .or_else(|| ast.member_spec_info(x).map(|(_, i)| i.kind));
        match kind {
            Some(
                SpecKind::ImplicitInstantiation
                | SpecKind::ExplicitInstantiationDeclaration
                | SpecKind::ExplicitInstantiationDefinition,
            ) => return true,
            Some(SpecKind::ExplicitSpecialization | SpecKind::Undeclared) => return false,
            None => {}
        }
        c = ast.decls.get(x).ctx();
    }
    false
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
    use crate::decl_func::tests::with_walk;
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
