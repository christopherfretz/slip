//! Local entities (`spec/digest.md` §4.3, `spec/digest-kinds.md` §2.4):
//! their ordinals and the declarations walked in place; local types and
//! lambdas are pending (`Area::Local`).

use slip_kiln::kiln::ast::DeclId;
use slip_kiln::kiln::ast::decl::DeclKind;

use crate::stream::Stream;
use crate::walk::Walk;
use crate::{Area, Cause};

/// The ordinal of a local entity of the unit being written: the next
/// one when the walk first meets it, by its declaration or by an earlier
/// reference (a `goto` before its label).
pub(crate) fn ordinal(w: &mut Walk<'_>, d: DeclId) -> u64 {
    let d = w.ast.first(d);
    if let Some(&n) = w.unit.locals.get(&d) {
        return n;
    }
    let n = w.unit.next_local;
    w.unit.next_local += 1;
    w.unit.locals.insert(d, n);
    n
}

/// `decl`: a declaration met inside a unit, walked in place (§2.4).
///
/// A variable, parameter, binding or label writes its ordinal and then
/// its stream (§5.1); a block-scope variable with linkage a `ref`, and a
/// block-scope function a `ref` and the default arguments it holds; a
/// `static_assert`, a `consteval` block, a template parameter and a
/// lifetime-extended temporary their stream, with no ordinal (§9, §5.2
/// "parent"); a
/// typedef, alias, using-declaration, using-directive or empty
/// declaration nothing. A local class, enumeration or closure type, and a
/// member of one, are the local package's. A kind §2.4 does not name is
/// `Unaccounted`.
#[allow(clippy::match_same_arms, reason = "one arm per rule of §2.4")]
pub(crate) fn write_decl(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    let ast = w.ast;
    let kind = ast.decls.get(d).kind();
    match kind {
        DeclKind::Var if w.is_local_extern(d) => w.write_ref(s, d),
        DeclKind::Function => {
            w.write_ref(s, d);
            block_defaults(w, s, d);
        }
        DeclKind::Var
        | DeclKind::ParmVar
        | DeclKind::Binding
        | DeclKind::Label
        | DeclKind::ContractResultName => {
            let n = ordinal(w, d);
            s.uv(n);
            w.decl_stream(s, d, ast.first(d));
        }
        DeclKind::StaticAssert
        | DeclKind::ConstevalBlock
        | DeclKind::TemplateTypeParm
        | DeclKind::NonTypeTemplateParm
        | DeclKind::TemplateTemplateParm
        | DeclKind::LifetimeExtendedTemporary => w.decl_stream(s, d, d),
        DeclKind::Typedef
        | DeclKind::TypeAlias
        | DeclKind::NamespaceAlias
        | DeclKind::Using
        | DeclKind::UsingShadow
        | DeclKind::ConstructorUsingShadow
        | DeclKind::UnresolvedUsingValue
        | DeclKind::UnresolvedUsingTypename
        | DeclKind::UnresolvedUsingIfExists
        | DeclKind::UsingEnum
        | DeclKind::UsingDirective
        | DeclKind::Empty => {}
        // A local type and what only a local type holds: the package for
        // local entities writes them.
        DeclKind::CXXRecord
        | DeclKind::Enum
        | DeclKind::EnumConstant
        | DeclKind::Field
        | DeclKind::IndirectField
        | DeclKind::CXXMethod
        | DeclKind::CXXConstructor
        | DeclKind::CXXDestructor
        | DeclKind::CXXConversion
        | DeclKind::Friend
        | DeclKind::AccessSpec
        | DeclKind::FunctionTemplate => {
            s.pending(Area::Local);
        }
        DeclKind::UsingPack => unsupported(s, "DeclKind/UsingPack"),
        DeclKind::FriendTemplate => unsupported(s, "DeclKind/FriendTemplate"),
        DeclKind::ImplicitParam => unsupported(s, "DeclKind/ImplicitParam"),
        DeclKind::ImplicitConceptSpecialization => {
            unsupported(s, "DeclKind/ImplicitConceptSpecialization");
        }
        // Nothing of these is declared in a block (§5.2).
        DeclKind::TranslationUnit
        | DeclKind::Namespace
        | DeclKind::LinkageSpec
        | DeclKind::Export
        | DeclKind::FileScopeAsm
        | DeclKind::ClassTemplateSpecialization
        | DeclKind::ClassTemplatePartialSpecialization
        | DeclKind::CXXDeductionGuide
        | DeclKind::VarTemplateSpecialization
        | DeclKind::VarTemplatePartialSpecialization
        | DeclKind::ClassTemplate
        | DeclKind::VarTemplate
        | DeclKind::TypeAliasTemplate
        | DeclKind::Concept
        | DeclKind::BuiltinTemplate
        | DeclKind::RequiresExprBody
        | DeclKind::TemplateParamObject => {
            s.uv(0).indeterminate(NOT_IN_BLOCK);
        }
    }
}

/// A kind Kiln does not build (`Unsupported("<short name>/<row>")`).
fn unsupported(s: &mut Stream, row: &'static str) {
    s.uv(0).indeterminate(Cause::Unsupported(row));
}

/// The default arguments a block-scope function declaration `d` holds,
/// after its `ref`: `count`, then one `child?` per parameter. They are
/// that declaration's own ([dcl.fct.default]/4), which the entity's unit
/// leaves out (`spec/landed/digest-kinds/WP-026.md`, an addition to §2.4).
fn block_defaults(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    let ast = w.ast;
    let params: Vec<DeclId> = ast
        .decls
        .function(d)
        .map(|f| ast.lists.iter::<DeclId>(f.params).collect())
        .unwrap_or_default();
    s.count(params.len());
    for p in params {
        let default = ast.decls.param(p).and_then(|x| x.default_arg);
        crate::expr::child_opt(w, s, default);
    }
}

/// A declaration `decl` met that §2.4 does not say can be in a block.
const NOT_IN_BLOCK: Cause = Cause::Unaccounted("decl/kind");

/// `record LambdaData` (§10.4): the `lambda` row of a closure type's
/// `RecordData`.
#[allow(dead_code, reason = "the interface of an area that lands later (lib.rs)")]
pub(crate) fn lambda_data(_w: &mut Walk<'_>, s: &mut Stream, _l: slip_kiln::kiln::ast::LambdaId) {
    s.pending(Area::Local);
}

#[cfg(test)]
mod tests {
    use slip_kiln::kiln::ast::decl::DeclKind;

    use crate::decl_func::tests::{named, of_kind, with_walk};
    use crate::stream::Stream;
    use crate::{Area, Cause};

    fn write(w: &mut crate::walk::Walk<'_>, d: slip_kiln::kiln::ast::DeclId) -> Stream {
        let mut s = Stream::new();
        super::write_decl(w, &mut s, d);
        s
    }

    // digest-kinds §2.4, digest §4.3: a parameter writes its ordinal, the
    // next one, then its stream.
    #[test]
    fn a_parameter_writes_its_ordinal_first() {
        with_walk("void f(int a, int b);", |w| {
            let a = named(w, "a", DeclKind::ParmVar)[0];
            let b = named(w, "b", DeclKind::ParmVar)[0];
            let sa = write(w, a);
            let sb = write(w, b);
            assert_eq!(sa.as_bytes()[0], 0);
            assert_eq!(sb.as_bytes()[0], 1);
            assert_eq!(sa.as_bytes()[1..], sb.as_bytes()[1..]);
            // Met again, the same ordinal.
            assert_eq!(write(w, a).as_bytes(), sa.as_bytes());
        });
    }

    // digest-kinds §2.4: a block-scope extern variable writes a `ref`; a
    // block-scope function a `ref`, then a count and one `child?` per
    // parameter: the defaults that declaration holds (WP-026).
    #[test]
    fn a_block_scope_entity_with_linkage_is_a_ref() {
        let src = "void h(int, int); extern int v;\n\
                   void g() { void h(int, int = 1); extern int v; }";
        with_walk(src, |w| {
            let v = *named(w, "v", DeclKind::Var).last().expect("the block-scope v");
            let mut want = Stream::new();
            w.write_ref(&mut want, v);
            assert_eq!(write(w, v).as_bytes(), want.as_bytes());
            let h = *named(w, "h", DeclKind::Function).last().expect("the block-scope h");
            let params: Vec<_> = w
                .ast
                .lists
                .iter::<slip_kiln::kiln::ast::DeclId>(w.ast.decls.function(h).expect("h").params)
                .collect();
            let second = w.ast.decls.param(params[1]).and_then(|p| p.default_arg);
            assert!(second.is_some());
            let mut want = Stream::new();
            w.write_ref(&mut want, h);
            want.count(2);
            crate::expr::child_opt(w, &mut want, None);
            crate::expr::child_opt(w, &mut want, second);
            assert_eq!(write(w, h).as_bytes(), want.as_bytes());
            // The namespace-scope declaration holds no default.
            let outer = named(w, "h", DeclKind::Function)[0];
            assert_ne!(write(w, outer).as_bytes(), want.as_bytes());
            assert_eq!(w.unit.next_local, 0);
        });
    }

    // digest-kinds §2.4: a typedef, alias, using-declaration or
    // using-directive writes nothing.
    #[test]
    fn a_name_only_declaration_writes_nothing() {
        let src = "namespace n { int i; } void g() { typedef int T; using U = int; using n::i; using namespace n; }";
        with_walk(src, |w| {
            for kind in
                [DeclKind::Typedef, DeclKind::TypeAlias, DeclKind::Using, DeclKind::UsingDirective]
            {
                let d = of_kind(w, kind)[0];
                let s = write(w, d);
                assert!(s.as_bytes().is_empty(), "{kind:?}");
                assert_eq!(s.cause(), None, "{kind:?}");
            }
        });
    }

    // digest-kinds §2.4: a local class is the local package's; a kind not
    // declared in a block is unaccounted, never a panic.
    #[test]
    fn local_types_are_pending_and_strangers_unaccounted() {
        with_walk("namespace n {} void g() { struct L {}; }", |w| {
            let l = named(w, "L", DeclKind::CXXRecord)[0];
            assert_eq!(write(w, l).cause(), Some(Cause::Pending(Area::Local)));
            let n = named(w, "n", DeclKind::Namespace)[0];
            assert_eq!(write(w, n).cause(), Some(super::NOT_IN_BLOCK));
        });
    }

    // digest-kinds §2.4 (WP-026): a block-scope `consteval` block, legal
    // C++26, writes its stream with no ordinal, as a `static_assert` does.
    #[test]
    fn a_consteval_block_in_a_block() {
        with_walk("void g() { consteval { } }", |w| {
            let d = of_kind(w, DeclKind::ConstevalBlock)[0];
            let mut want = Stream::new();
            w.decl_stream(&mut want, d, d);
            let got = write(w, d);
            assert_eq!(got.as_bytes(), want.as_bytes());
            assert_ne!(got.cause(), Some(super::NOT_IN_BLOCK));
            assert_eq!(w.unit.next_local, 0);
        });
    }

    // digest-kinds §2.4, §9: a static_assert in a block writes its stream
    // with no ordinal.
    #[test]
    fn a_static_assert_has_no_ordinal() {
        with_walk("void g() { static_assert(true); }", |w| {
            let d = of_kind(w, DeclKind::StaticAssert)[0];
            let mut want = Stream::new();
            w.decl_stream(&mut want, d, d);
            assert_eq!(write(w, d).as_bytes(), want.as_bytes());
            assert_eq!(w.unit.next_local, 0);
        });
    }
}
