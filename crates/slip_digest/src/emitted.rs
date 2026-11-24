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

use std::collections::HashSet;

use slip_kiln::kiln::ast::attr::AttrKind;
use slip_kiln::kiln::ast::decl::{
    DeclFlags, DeclKind, DefinitionBits, TemplLinkKind, TlsKind, VarEvalState, VarStorage,
};
use slip_kiln::kiln::ast::expr::{CastKind, ExprKind, TraitKind, UnaryOp};
use slip_kiln::kiln::ast::types::TypeView;
use slip_kiln::kiln::ast::{DeclId, ExprId, QualType};

use crate::demand::Property;
use crate::units::Written;
use crate::walk::Walk;

/// Steps through arrays and references [`destructed`] takes at most.
const TYPE_LIMIT: usize = 256;

/// Enclosing declarations [`hosts`] climbs at most from a local function.
const CTX_LIMIT: usize = 256;

/// Expression nodes [`plain`] reads at most; past it the initialiser is
/// not plainly constant.
const PLAIN_LIMIT: usize = 64;

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
    let has =
        |k: AttrKind| ast.inherited_attr(rep, k).is_some() || ast.inherited_attr(def, k).is_some();
    if has(AttrKind::USED) {
        return true;
    }
    // A function definition with `constructor`, `destructor` or `used`
    // in its body or initialiser, at any depth (a local class's member
    // function, a lambda's call operator): emitted with it.
    let hosted = {
        let hosts = hosts(w);
        [e, rep, def].iter().any(|d| hosts.contains(d))
    };
    if hosted {
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

/// Whether variable `e` is known to be constant-initialised
/// (`spec/digest.md` §11, "The mark"):
/// - its evaluated state is `Value`, read as the resolved unit `value`
///   reads it (over every declaration, the out-of-line definition an
///   instantiation made included);
/// - or no declaration has an initialiser and its type `ty` is not a
///   class, or is a class (or an array of one) whose default constructor
///   is trivial; or the initialiser is the call, with no argument, of a
///   trivial constructor (`Ast::is_trivial_default_construction`, how
///   Kiln holds `K k;`): zero-initialisation is static, and default
///   initialisation does nothing;
/// - or its initialiser is plainly constant ([`plain`]) and `ty` is an
///   arithmetic, enumeration, pointer or reference type.
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
    let Some(ty) = ty else { return false };
    let init = chain.iter().find_map(|&d| ast.decls.var(d).and_then(|v| v.init));
    match init {
        None => match class_of(w, ty) {
            Held::Other => true,
            Held::Class(rd) => trivially_default_constructed(w, rd),
            Held::Unread => false,
        },
        // Default initialisation of a class (or an array of one) is a
        // `CXXConstructExpr` with no argument: constant when the
        // constructor it calls is trivial.
        Some(init) if ast.is_trivial_default_construction(init) => true,
        Some(init) => plain_type(w, ty) && plain(w, init),
    }
}

/// Class `rd` has a definition whose default constructor is trivial:
/// `TRIVIAL_DEFAULT_CTOR` set and `NONTRIVIAL_DEFAULT_CTOR` clear.
fn trivially_default_constructed(w: &Walk<'_>, rd: DeclId) -> bool {
    let ast = w.ast;
    ast.definition(rd).and_then(|d| ast.decls.record(d)).is_some_and(|r| {
        r.defn.contains(DefinitionBits::TRIVIAL_DEFAULT_CTOR)
            && !r.defn.contains(DefinitionBits::NONTRIVIAL_DEFAULT_CTOR)
    })
}

/// `q` is an arithmetic, enumeration, pointer or reference type: the
/// types [`plain`] is read for.
fn plain_type(w: &Walk<'_>, q: QualType) -> bool {
    let types = &w.ast.types;
    types.is_arithmetic(q)
        || types.is_pointer(q)
        || types.is_reference(q)
        || matches!(types.canonical_view(q), TypeView::Enum(_))
}

/// Whether initialiser `e` is plainly constant: constant without
/// evaluation. The grammar (`spec/digest.md` §11, "The mark"):
///
/// ```text
/// plain   := literal | enumerator | sizeof/alignof | address | `( plain )`
///          | `{ }` | `{ plain }` | `+ plain` | `- plain`
///          | an implicit scalar conversion of plain
/// literal := integer | floating | character | `true`/`false` | string
///          | `nullptr` | `__null`
/// address := `& object` | `object` (bound to a reference, or decayed
///            from an array) | `function` (bound, or decayed)
/// object  := a variable of static storage duration, not thread-local,
///            not a reference; or a string literal
/// ```
///
/// Anything else (a call, a read of a variable, a temporary, a cast
/// written in the source) is not plainly constant.
fn plain(w: &Walk<'_>, e: ExprId) -> bool {
    let ast = w.ast;
    let exprs = &ast.exprs;
    let mut work = vec![e];
    let mut seen = 0;
    while let Some(e) = work.pop() {
        seen += 1;
        if seen > PLAIN_LIMIT {
            return false;
        }
        match exprs.get(e).kind() {
            ExprKind::IntegerLiteral
            | ExprKind::FloatingLiteral
            | ExprKind::CharacterLiteral
            | ExprKind::CXXBoolLiteralExpr
            | ExprKind::CXXNullPtrLiteralExpr
            | ExprKind::GNUNullExpr
            | ExprKind::StringLiteral => {}
            ExprKind::UnaryExprOrTypeTraitExpr => {
                if !matches!(
                    exprs.trait_kind(e),
                    TraitKind::SizeOf | TraitKind::AlignOf | TraitKind::PreferredAlignOf
                ) {
                    return false;
                }
            }
            ExprKind::ParenExpr => work.push(exprs.operand(e)),
            ExprKind::InitListExpr => {
                let kids = exprs.children(e, &ast.lists);
                if kids.len() > 1 {
                    return false;
                }
                work.extend(kids);
            }
            ExprKind::UnaryOperator => match exprs.unary_op(e) {
                UnaryOp::Plus | UnaryOp::Minus => work.push(exprs.operand(e)),
                UnaryOp::AddrOf => {
                    if !designates(w, exprs.operand(e)) {
                        return false;
                    }
                }
                _ => return false,
            },
            ExprKind::ImplicitCastExpr => match exprs.cast_kind(e) {
                CastKind::IntegralCast
                | CastKind::IntegralToFloating
                | CastKind::FloatingToIntegral
                | CastKind::FloatingCast
                | CastKind::IntegralToBoolean
                | CastKind::FloatingToBoolean
                | CastKind::BooleanToSignedIntegral
                | CastKind::NullToPointer
                | CastKind::NoOp => work.push(exprs.operand(e)),
                CastKind::ArrayToPointerDecay | CastKind::FunctionToPointerDecay => {
                    if !designates(w, exprs.operand(e)) {
                        return false;
                    }
                }
                _ => return false,
            },
            // An enumerator; or, bound to a reference, an object or a
            // function.
            ExprKind::DeclRefExpr => {
                let d = exprs.decl(e);
                if ast.decls.get(d).kind() != DeclKind::EnumConstant && !designates(w, e) {
                    return false;
                }
            }
            _ => return false,
        }
    }
    true
}

/// `e`, through parentheses, names an object of static storage duration
/// that is not thread-local and not a reference, a function, or is a
/// string literal: its address is a constant.
fn designates(w: &Walk<'_>, mut e: ExprId) -> bool {
    let ast = w.ast;
    let exprs = &ast.exprs;
    for _ in 0..PLAIN_LIMIT {
        match exprs.get(e).kind() {
            ExprKind::ParenExpr => e = exprs.operand(e),
            ExprKind::StringLiteral => return true,
            ExprKind::DeclRefExpr => {
                let d = exprs.decl(e);
                if ast.decls.function(d).is_some() {
                    return true;
                }
                let Some(v) = ast.decls.var(d) else { return false };
                if ast.decls.get(d).kind() != DeclKind::Var
                    || v.spec.tls() != TlsKind::None
                    || ast.decls.get(d).ty().is_none_or(|t| ast.types.is_reference(t))
                {
                    return false;
                }
                return !w.is_local(d) || v.spec.storage() == VarStorage::Static;
            }
            _ => return false,
        }
    }
    false
}

/// The declarations around a local function with the attribute
/// `constructor`, `destructor` or `used` (`Ast::inherited_attr`): every
/// enclosing declaration, and for a closure type the declaration its name
/// is prefixed by (a variable a lambda initialises). Built once per walk.
fn hosts<'w>(w: &'w mut Walk<'_>) -> &'w HashSet<DeclId> {
    if w.emitted_hosts.is_none() {
        let ast = w.ast;
        let mut set = HashSet::new();
        for d in ast.decls.ids() {
            if ast.decls.function(d).is_none() || !w.is_local(d) {
                continue;
            }
            let attr = |k: AttrKind| ast.inherited_attr(d, k).is_some();
            if !(attr(AttrKind::CONSTRUCTOR) || attr(AttrKind::DESTRUCTOR) || attr(AttrKind::USED))
            {
                continue;
            }
            let mut next = ast.decls.get(d).ctx();
            for _ in 0..CTX_LIMIT {
                let Some(up) = next else { break };
                if !set.insert(up) {
                    break;
                }
                let lambda = ast.decls.record(up).and_then(|r| r.lambda);
                let by = lambda.and_then(|id| ast.exprs.lambdas.get(id).mangling.context);
                if let Some(var) = by {
                    set.insert(var);
                }
                next = ast.decls.get(up).ctx();
            }
        }
        w.emitted_hosts = Some(set);
    }
    w.emitted_hosts.get_or_insert_default()
}

/// Whether an object of type `q` needs destruction: through arrays and
/// references, a class whose definition is missing or whose destructor is
/// not trivial. What cannot be read is taken as needing it.
fn destructed(w: &Walk<'_>, q: QualType) -> bool {
    match class_of(w, q) {
        Held::Class(rd) => {
            let ast = w.ast;
            ast.definition(rd)
                .and_then(|d| ast.decls.record(d))
                .is_none_or(|r| !r.defn.contains(DefinitionBits::TRIVIAL_DTOR))
        }
        Held::Unread => true,
        Held::Other => false,
    }
}

/// What a type holds through arrays and references.
enum Held {
    /// A class.
    Class(DeclId),
    /// Not read: a chain past [`TYPE_LIMIT`].
    Unread,
    /// Any other type.
    Other,
}

/// The class of type `q` through arrays and references.
fn class_of(w: &Walk<'_>, q: QualType) -> Held {
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
            _ => return types.as_record(cur).map_or(Held::Other, Held::Class),
        };
    }
    Held::Unread
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
