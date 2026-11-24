//! The mark "emitted unreferenced" (`spec/digest.md` §11 flag bit 1,
//! `spec/verify.md` §4 step 3; WP-155, `spec/landed/verify/WP-155.md`
//! §1): the unit that holds the definition of an entity the translation
//! unit emits whether or not anything refers to it.
//!
//! Kiln's code generator decides that with `var_must_emit`
//! (`kiln_codegen/src/global_init.rs`), `USED` and `is_ctor_dtor_fn`
//! (`kiln_codegen/src/module.rs`), which Slip does not link. The walk
//! decides it from the AST, and where the AST does not say, it marks: a
//! mark too many is a finding too many, a mark too few a wrong
//! `verified`.
//!
//! - A variable (`DeclKind::Var`, not local, not in a template) is marked
//!   when it has the attribute `used` (`Ast::inherited_attr`, which reads
//!   the pattern an instantiation was made from too), or its type, through
//!   arrays and references, is a class whose definition is missing or has
//!   no `DefinitionBits::TRIVIAL_DTOR`, or it is not known to be
//!   constant-initialised: its `VarData` `eval` (the state the resolved
//!   unit `value` reads, `demand::value`) is not `Value`, except a
//!   variable no declaration of which has an initialiser and whose type
//!   is not a class (zero-initialised, [basic.start.static]/2). Static
//!   and thread storage alike: a thread-local is marked under the same
//!   conditions, although its initialisation is lazy.
//! - A function is marked when it has the attribute `constructor`,
//!   `destructor` or `used` (`Ast::inherited_attr`).
//!
//! The mark is set on the resolved unit `definition` of an entity of the
//! definition class (`digest-kinds.md` §11.9), which exists when Kiln
//! holds the definition, and otherwise on the entity's own unit when the
//! entity is defined (`units::defining_decl`). It is part of no unit's
//! digest: [`mark`] only sets `Written::emitted`.

use slip_kiln::kiln::ast::DeclId;
use slip_kiln::kiln::ast::attr::AttrKind;
use slip_kiln::kiln::ast::decl::{DeclFlags, DeclKind, DefinitionBits, TemplLinkKind, VarEvalState};
use slip_kiln::kiln::ast::types::TypeView;
use slip_kiln::kiln::ast::QualType;

use crate::demand::Property;
use crate::units::Written;
use crate::walk::Walk;

/// Steps through arrays and references [`destructed`] takes at most.
const TYPE_LIMIT: usize = 256;

/// Sets `emitted` on the unit of entity `e` that holds its definition:
/// its resolved unit `definition` among `resolved` (the units
/// `demand::resolved_units` just wrote for `own`), else `own`, when the
/// entity is emitted unreferenced ([`emits`]).
pub(crate) fn mark(w: &mut Walk<'_>, e: DeclId, own: &mut Written, resolved: &mut [Written]) {
    if own.entity != Some(e) || own.resolved.is_some() || own.default_argument.is_some() {
        return;
    }
    let ast = w.ast;
    let rep = w.representative(e);
    let var = ast.decls.get(rep).kind() == DeclKind::Var && ast.decls.var(rep).is_some();
    let function = ast.decls.function(rep).is_some();
    // Read on the representative, the declaration the unit is classified
    // from: an entity first declared at block scope (`extern int x;` in a
    // function, digest-kinds §2.4) is the namespace's.
    if !(var || function) || (w.is_local(rep) && !w.is_local_extern(rep)) || templated(w, rep) {
        return;
    }
    let at = resolved.iter().position(|u| {
        u.default_argument.is_none()
            && u.resolved.is_some_and(|r| r.property == Property::Definition)
    });
    let def = match at {
        Some(_) => crate::demand::defining_declaration(w, e),
        None if crate::demand::definition_class(w, e) => None,
        None => crate::units::defining_decl(w, e),
    };
    let Some(def) = def else { return };
    if !emits(w, e, rep, def, var) {
        return;
    }
    match at {
        Some(i) => resolved[i].emitted = true,
        None => own.emitted = true,
    }
}

/// Whether entity `e` (representative `rep`, defining declaration `def`)
/// is emitted whether or not anything refers to it: the rule of the
/// module documentation.
fn emits(w: &mut Walk<'_>, e: DeclId, rep: DeclId, def: DeclId, var: bool) -> bool {
    let ast = w.ast;
    let has = |k: AttrKind| {
        ast.inherited_attr(rep, k).is_some() || ast.inherited_attr(def, k).is_some()
    };
    if has(AttrKind::USED) {
        return true;
    }
    if !var {
        return has(AttrKind::CONSTRUCTOR) || has(AttrKind::DESTRUCTOR);
    }
    let ty = ast.decls.get(def).ty();
    if ty.is_none_or(|q| destructed(w, q)) {
        return true;
    }
    !constant_initialised(w, e, rep, ty)
}

/// Whether variable `e` is known to be constant-initialised: its
/// evaluated state is `Value`, read as the resolved unit `value` reads it
/// (over every declaration, the out-of-line definition an instantiation
/// made included); or no declaration has an initialiser and its type `ty`
/// is not a class (zero-initialisation is static).
fn constant_initialised(w: &mut Walk<'_>, e: DeclId, rep: DeclId, ty: Option<QualType>) -> bool {
    let ast = w.ast;
    // In definition mode, as `demand::value` reads it: the chain then
    // holds the out-of-line definition an instantiation made for a
    // static data member, which holds the initialiser and its state.
    let in_definition = std::mem::replace(&mut w.unit.definition, true);
    let eval = crate::decl_func::var_eval(w, rep);
    let chain = w.redecls(e);
    w.unit.definition = in_definition;
    if matches!(eval.state(), VarEvalState::Value(_)) {
        return true;
    }
    let initialised = chain.iter().any(|&d| ast.decls.var(d).is_some_and(|v| v.init.is_some()));
    !initialised && ty.is_some_and(|q| class_of(w, q).is_none())
}

/// Whether an object of type `q` needs destruction: through arrays and
/// references, a class whose definition is missing or whose destructor is
/// not trivial. What cannot be read is taken as needing it.
fn destructed(w: &Walk<'_>, q: QualType) -> bool {
    match class_of(w, q) {
        Some(Some(rd)) => {
            let ast = w.ast;
            ast.definition(rd)
                .and_then(|d| ast.decls.record(d))
                .is_none_or(|r| !r.defn.contains(DefinitionBits::TRIVIAL_DTOR))
        }
        Some(None) => true,
        None => false,
    }
}

/// The class of type `q` through arrays and references: `Some(Some(rd))`
/// for a class, `Some(None)` when it cannot be read (a chain past
/// [`TYPE_LIMIT`]), `None` for any other type.
fn class_of(w: &Walk<'_>, q: QualType) -> Option<Option<DeclId>> {
    let types = &w.ast.types;
    let mut cur = q;
    for _ in 0..TYPE_LIMIT {
        cur = match types.canonical_view(cur) {
            TypeView::ConstantArray(a) => a.elem,
            TypeView::IncompleteArray(x)
            | TypeView::VariableArray { elem: x, .. }
            | TypeView::DependentSizedArray { elem: x, .. }
            | TypeView::LValueRef(x)
            | TypeView::RValueRef(x) => x,
            _ => return types.as_record(cur).map(Some),
        };
    }
    Some(None)
}

/// `d` is a template's pattern, a partial specialisation or a
/// declaration in a dependent context: nothing of it is emitted.
fn templated(w: &Walk<'_>, d: DeclId) -> bool {
    let ast = w.ast;
    if ast.decls.get(d).has(DeclFlags::DEPENDENT_CTX) || ast.described_template(d).is_some() {
        return true;
    }
    matches!(
        ast.templ_link(d).get(),
        Some((TemplLinkKind::Pattern | TemplLinkKind::PartialSpecialization, _))
    )
}
