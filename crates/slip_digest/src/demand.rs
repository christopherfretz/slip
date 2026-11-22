//! Demand-driven fields and resolved units (`spec/digest-kinds.md` §11):
//! the deferrable class as far as stored facts decide it; resolved units
//! pending (WP-024).

use slip_kiln::kiln::ast::DeclId;
use slip_kiln::kiln::ast::decl::{DeclFlags, DeclKind, Defaulted};
use slip_kiln::kiln::ast::templates::SpecKind;

use crate::walk::Walk;

/// Whether function `f` is of the deferrable class (§11.2); `None` where
/// the rule needs what the demand area decides (an explicitly defaulted
/// function, whose own specification may or may not have been written;
/// an implicit instantiation, whose pattern's specification is an
/// expression), so the caller writes a pending row.
pub(crate) fn deferrable(w: &Walk<'_>, f: DeclId) -> Option<bool> {
    let ast = w.ast;
    let n = ast.decls.get(f);
    // Rule 4: every destructor.
    if n.kind() == DeclKind::CXXDestructor {
        return Some(true);
    }
    // Rules 1 and 2: implicitly declared special members, comparison
    // operators and inheriting constructors.
    if w.redecls(f).iter().all(|&d| ast.decls.get(d).has(DeclFlags::IMPLICIT)) {
        return Some(true);
    }
    let first = ast.first(f);
    if let Some(data) = ast.decls.function(first) {
        match data.spec.defaulted() {
            Defaulted::Implicit => return Some(true),
            Defaulted::Explicit => return None,
            Defaulted::No => {}
        }
    }
    // Rule 3: implicit instantiations.
    let implicit_instantiation = ast
        .spec_info(f)
        .map(|(_, i)| i.kind)
        .or_else(|| ast.member_spec_info(f).map(|(_, i)| i.kind))
        .is_some_and(|k| match k {
            SpecKind::ImplicitInstantiation => true,
            SpecKind::Undeclared
            | SpecKind::ExplicitSpecialization
            | SpecKind::ExplicitInstantiationDeclaration
            | SpecKind::ExplicitInstantiationDefinition => false,
        });
    if implicit_instantiation {
        return None;
    }
    Some(false)
}
