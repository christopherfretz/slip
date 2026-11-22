//! Local entities (`spec/digest.md` §4.3, `spec/digest-kinds.md` §2.4):
//! their ordinals and the declarations walked in place; local types and
//! lambdas are pending (`Area::Local`).

use slip_kiln::kiln::ast::DeclId;
use slip_kiln::kiln::ast::decl::DeclKind;

use crate::{Area, Cause};
use crate::stream::Stream;
use crate::walk::Walk;

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
/// its stream (§5.1); a block-scope function or variable with linkage a
/// `ref`; a `static_assert`, a template parameter and a lifetime-extended
/// temporary their stream, with no ordinal (§9, §5.2 "parent"); a
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
        DeclKind::Function => w.write_ref(s, d),
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
        DeclKind::UsingPack
        | DeclKind::FriendTemplate
        | DeclKind::ImplicitParam
        | DeclKind::ImplicitConceptSpecialization => {
            s.uv(0).indeterminate(Cause::Unsupported("DeclKind"));
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
        | DeclKind::TemplateParamObject
        | DeclKind::ConstevalBlock => {
            s.uv(0).indeterminate(NOT_IN_BLOCK);
        }
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
