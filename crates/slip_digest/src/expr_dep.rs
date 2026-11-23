//! Names and constructs of templates (`spec/digest-kinds.md` §7), and
//! canonical expression operands (§4.4), WP-036.
//!
//! The rows of a kind's function (`expr::KindFn`): its kind's rows only,
//! in table order; `expr::child` writes the common rows before the call
//! and the `ColdExprData` rows and `templ::expr_maps` after it (`lib.rs`
//! "How to fill an expression kind").
//!
//! # Profiles (§4.4)
//!
//! A profile is the expression `ProfileEntry.first_expr` walked in
//! profile mode (`expr_profile.rs`: a row iff Kiln's profile
//! distinguishes it), with the expression state of the stream being
//! written put aside ([`Walk::take_expr_state`]) and a [`Frame`] in its
//! place, restored after. The frame numbers the entities declared inside
//! an expression walked in full under it (a requires-expression inside a
//! kind profile mode does not follow) from 0; a function parameter
//! declared outside is `Ref::param` (its function-scope depth and
//! position); any other local entity takes the unit's ordinal
//! ([`profile_local`]).

use std::collections::HashMap;

use slip_kiln::kiln::ast::decl::DeclKind;
use slip_kiln::kiln::ast::expr::{BinaryOp, ExprFlags};
use slip_kiln::kiln::ast::names::{DeclName, DeclNameView};
use slip_kiln::kiln::ast::stmt::ExpansionForm;
use slip_kiln::kiln::ast::templates::Requirement;
use slip_kiln::kiln::ast::{ArgListId, DeclId, ExprId, LambdaId, ListId, NnsId, ProfileId};
use slip_model::keys::Name;

use crate::Cause;
use crate::expr::{self, Node, check_aux, check_bits};
use crate::refs::{REF_LOCAL, REF_NONE, REF_PARAM};
use crate::stream::{FlagWord, Stream};
use crate::tables::{binary_op, expansion_form, overloaded_operator, requirement};
use crate::walk::Walk;

/// The numbering of a stream that holds no unit ordinal (§4.4): the
/// entities declared inside it, by first declaration (a
/// requires-expression's parameters in a key, or in an expression of a
/// kind profile mode walks in full).
#[derive(Debug, Default)]
pub(crate) struct Frame {
    locals: HashMap<DeclId, u64>,
    next: u64,
}

/// `Cause::Unaccounted` of row `row` of `ExprNode`.
const fn row(r: &'static str) -> Cause {
    Cause::Unaccounted(r)
}

/// The bit of a per-kind flag, in place (bits 4 to 7).
const fn bit(f: ExprFlags) -> u8 {
    f.bits()
}

/// `child` of a word: the placeholder and `cause` when it is 0.
fn child_word(w: &mut Walk<'_>, s: &mut Stream, raw: u32, cause: Cause) {
    match ExprId::try_new(raw) {
        Some(e) => expr::child(w, s, e),
        None => {
            s.uv(0).indeterminate(cause);
        }
    }
}

/// `ref` of a word: `Ref::none` and `cause` when it is 0.
fn ref_word(w: &mut Walk<'_>, s: &mut Stream, raw: u32, cause: Cause) {
    match DeclId::try_new(raw) {
        Some(d) => w.write_ref(s, d),
        None => {
            s.uv(REF_NONE).indeterminate(cause);
        }
    }
}

/// `enum ArgView*` of a raw `ArgListId`; 0 is the empty list, as Kiln
/// reads it (`ExprArena::subst_pack_args`, `expr.rs:2998`).
fn args_word(w: &mut Walk<'_>, s: &mut Stream, raw: u32) {
    w.write_args(s, ArgListId::try_new(raw).unwrap_or(ArgListId::EMPTY));
}

/// The declarations of the `ListId` in `raw`, in stored order.
fn decl_list(w: &Walk<'_>, s: &mut Stream, raw: u32, cause: Cause) -> Vec<DeclId> {
    let Some(l) = ListId::try_new(raw) else {
        s.indeterminate(cause);
        return Vec::new();
    };
    let words = w.ast.lists.get(l);
    let mut out = Vec::with_capacity(words.len());
    for &word in words {
        match DeclId::try_new(word) {
            Some(d) => out.push(d),
            None => {
                s.indeterminate(cause);
            }
        }
    }
    out
}

/// The `Name` (§3.2) of a raw `DeclName`, as `Walk::plain_name` gives the
/// name of a declaration (`keys.rs`), with the cause it carries; `None`
/// for a word that is no name, or the empty name.
pub(crate) fn decl_name(w: &mut Walk<'_>, raw: u32) -> Option<(Name, Option<Cause>)> {
    let ast = w.ast;
    let name = DeclName::from_raw(raw)?;
    Some(match ast.names.view(name) {
        DeclNameView::Empty => return None,
        DeclNameView::Identifier(sym) => {
            let text = ast.symbols().as_str(sym);
            if text.is_empty() {
                return None;
            }
            (Name::Identifier(text.as_bytes().to_vec()), None)
        }
        DeclNameView::Operator(op) => {
            (Name::Operator(u64::from(overloaded_operator::row(&op).tag)), None)
        }
        DeclNameView::Constructor(_) => (Name::Constructor, None),
        DeclNameView::Destructor(_) => (Name::Destructor, None),
        DeclNameView::Conversion(q) => {
            let t = w.type_digest(q, false);
            (Name::Conversion(t.digest), t.cause)
        }
        DeclNameView::LiteralOperator(sym) => {
            (Name::LiteralOperator(ast.symbols().as_str(sym).as_bytes().to_vec()), None)
        }
        DeclNameView::DeductionGuide(_) => (Name::DeductionGuide, None),
    })
}

/// `name` of a raw `DeclName`; `Name::Anonymous` and `cause` for a word
/// that is no name.
/// A `DeclNameId` word in profile mode (`expr_profile.rs`).
pub(crate) fn name_of(w: &mut Walk<'_>, s: &mut Stream, raw: u32) {
    name_word(w, s, raw, Cause::Unaccounted("profile/name"));
}

fn name_word(w: &mut Walk<'_>, s: &mut Stream, raw: u32, cause: Cause) {
    match decl_name(w, raw) {
        Some((name, c)) => {
            s.name(&name).absorb(c);
        }
        None => {
            s.name(&Name::Anonymous).indeterminate(cause);
        }
    }
}

/// `UnresolvedLookupExpr`: `aux` (ADL, a bool), `a` the name, `b` the
/// declarations found at the definition, sorted.
pub(crate) fn unresolved_lookup_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let n = expr::node(w, e);
    check_bits(s, &n, 0, row("ExprNode/UnresolvedLookupExpr.bits"));
    check_aux(s, &n, 1, row("ExprNode/UnresolvedLookupExpr.aux"));
    s.bool(n.aux & 1 != 0);
    name_word(w, s, n.a, row("ExprNode/UnresolvedLookupExpr.a"));
    let ds = decl_list(w, s, n.b, row("ExprNode/UnresolvedLookupExpr.b"));
    w.write_refs_sorted(s, &ds);
}

/// `DependentScopeDeclRefExpr`: `a` the qualifier, `b` the name.
pub(crate) fn dependent_scope_decl_ref_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let n = expr::node(w, e);
    check_bits(s, &n, 0, row("ExprNode/DependentScopeDeclRefExpr.bits"));
    check_aux(s, &n, 0, row("ExprNode/DependentScopeDeclRefExpr.aux"));
    match NnsId::try_new(n.a) {
        Some(qualifier) => w.write_nns(s, qualifier),
        None => {
            s.uv(0).indeterminate(row("ExprNode/DependentScopeDeclRefExpr.a"));
        }
    }
    name_word(w, s, n.b, row("ExprNode/DependentScopeDeclRefExpr.b"));
}

/// `CXXDependentScopeMemberExpr`: the flag word of `aux` (`arrow`;
/// `template-keyword` is `out`), `a` the object, `b` the name.
pub(crate) fn cxx_dependent_scope_member_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let n = expr::node(w, e);
    check_bits(s, &n, 0, row("ExprNode/CXXDependentScopeMemberExpr.bits"));
    check_aux(s, &n, 3, row("ExprNode/CXXDependentScopeMemberExpr.aux"));
    s.flags(FlagWord::new().bit(n.aux & 1 != 0));
    expr::child_opt(w, s, ExprId::try_new(n.a));
    name_word(w, s, n.b, row("ExprNode/CXXDependentScopeMemberExpr.b"));
}

/// `SubstNonTypeTemplateParmExpr`: the flag word (`reference-parameter`),
/// `a` the parameter replaced, `b` the replacement.
pub(crate) fn subst_non_type_template_parm_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let n = expr::node(w, e);
    let reference = bit(ExprFlags::REFERENCE_PARAMETER);
    check_bits(s, &n, reference, row("ExprNode/SubstNonTypeTemplateParmExpr.bits"));
    check_aux(s, &n, 0, row("ExprNode/SubstNonTypeTemplateParmExpr.aux"));
    s.flags(FlagWord::new().bit(n.flags & reference != 0));
    ref_word(w, s, n.a, row("ExprNode/SubstNonTypeTemplateParmExpr.a"));
    child_word(w, s, n.b, row("ExprNode/SubstNonTypeTemplateParmExpr.b"));
}

/// `SubstNonTypeTemplateParmPackExpr`: `a` the parameter pack, `b` its
/// arguments.
pub(crate) fn subst_non_type_template_parm_pack_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let n = expr::node(w, e);
    check_bits(s, &n, 0, row("ExprNode/SubstNonTypeTemplateParmPackExpr.bits"));
    check_aux(s, &n, 0, row("ExprNode/SubstNonTypeTemplateParmPackExpr.aux"));
    ref_word(w, s, n.a, row("ExprNode/SubstNonTypeTemplateParmPackExpr.a"));
    args_word(w, s, n.b);
}

/// `Ref::param` of a function parameter: its function-scope depth and
/// position (`ParamData`), what Kiln's profile and the Itanium mangling
/// read; `Ref::none` and `cause` without its `ParamData`.
fn param_ref(w: &Walk<'_>, s: &mut Stream, d: DeclId, cause: Cause) {
    match w.ast.decls.param(d) {
        Some(p) => {
            s.uv(REF_PARAM).uv(u64::from(p.scope_depth())).uv(u64::from(p.index));
        }
        None => {
            s.uv(REF_NONE).indeterminate(cause);
        }
    }
}

/// `FunctionParmPackExpr`: `a` the pattern's parameter pack, which has no
/// ordinal in this unit and is `Ref::param` (as `size_of_pack_origin`,
/// `templ.rs`); `b` the expanded parameters, in order.
pub(crate) fn function_parm_pack_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let n = expr::node(w, e);
    check_bits(s, &n, 0, row("ExprNode/FunctionParmPackExpr.bits"));
    check_aux(s, &n, 0, row("ExprNode/FunctionParmPackExpr.aux"));
    let cause = row("ExprNode/FunctionParmPackExpr.a");
    match DeclId::try_new(n.a) {
        Some(pack) if w.ast.decls.get(pack).kind() == DeclKind::ParmVar => {
            param_ref(w, s, pack, cause);
        }
        Some(pack) => w.write_ref(s, pack),
        None => {
            s.uv(REF_NONE).indeterminate(cause);
        }
    }
    let ds = decl_list(w, s, n.b, row("ExprNode/FunctionParmPackExpr.b"));
    w.write_refs(s, &ds);
}

/// `SizeOfPackExpr`: `a` the pack. What a substituted `sizeof...` keeps
/// is `TemplateTables` `size_of_pack_origin` and `size_of_pack_partial`,
/// written by `templ::expr_maps`.
pub(crate) fn size_of_pack_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let n = expr::node(w, e);
    check_bits(s, &n, 0, row("ExprNode/SizeOfPackExpr.bits"));
    check_aux(s, &n, 0, row("ExprNode/SizeOfPackExpr.aux"));
    if n.b != 0 {
        s.indeterminate(row("ExprNode/SizeOfPackExpr.b"));
    }
    ref_word(w, s, n.a, row("ExprNode/SizeOfPackExpr.a"));
}

/// The stored result of a requires-expression or a concept-id (`aux`):
/// `uv`, 0, 1 or 2 (`ExprArena::satisfied`, `expr.rs:3134`).
fn satisfaction(s: &mut Stream, n: &Node, cause: Cause) {
    if n.aux > 2 {
        s.indeterminate(cause);
    }
    s.uv(u64::from(n.aux));
}

/// `RequiresExpr`: `aux` the stored result, `b` the `RequiresData`
/// (§9.3): the parameters by `decl`, then the requirements. Inside a
/// profile, or in a key, the parameters are numbered within the stream
/// being written (§4.4); elsewhere they take the unit's ordinals (§2.4).
pub(crate) fn requires_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let n = expr::node(w, e);
    check_bits(s, &n, 0, row("ExprNode/RequiresExpr.bits"));
    satisfaction(s, &n, row("ExprNode/RequiresExpr.aux"));
    let ast = w.ast;
    let Some(&data) = ast.templates.requires_data(n.b) else {
        s.count(0).count(0).indeterminate(row("ExprNode/RequiresExpr.b"));
        return;
    };
    // A key holds no ordinal of a unit: its requires-expression numbers
    // its own parameters, as a profile does.
    let own_frame = w.in_key > 0 && w.unit.expr.profile.is_none();
    if own_frame {
        w.unit.expr.profile = Some(Frame::default());
    }
    requires_data(w, s, &data);
    if own_frame {
        w.unit.expr.profile = None;
    }
}

/// `record RequiresData` (§9.3).
fn requires_data(
    w: &mut Walk<'_>,
    s: &mut Stream,
    data: &slip_kiln::kiln::ast::templates::RequiresData,
) {
    let ast = w.ast;
    // `body` and `len` are `out`; `first` is read through `requirements`.
    let params = ast.lists.iter::<DeclId>(data.params).collect::<Vec<_>>();
    s.count(params.len());
    for p in params {
        let first = ast.first(p);
        let numbered = w.unit.expr.profile.as_mut().map(|f| {
            let k = f.next;
            f.next += 1;
            f.locals.insert(first, k);
            k
        });
        match numbered {
            Some(k) => {
                s.uv(k);
                w.decl_stream(s, p, first);
            }
            None => crate::local::write_decl(w, s, p),
        }
    }
    let reqs = ast.templates.requirements(data);
    s.count(reqs.len());
    for &r in reqs {
        s.tag(requirement::row(&r));
        match r {
            Requirement::Simple(x) | Requirement::Nested(x) => expr::child(w, s, x),
            Requirement::Type(q, _position) => w.write_type(s, q),
            Requirement::Compound { expr: x, noexcept, constraint } => {
                expr::child(w, s, x);
                s.bool(noexcept.is_some());
                expr::child_opt(w, s, constraint);
            }
        }
    }
}

/// `ConceptSpecializationExpr`: `aux` the stored result, `a` the concept,
/// `b` the arguments.
pub(crate) fn concept_specialization_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let n = expr::node(w, e);
    check_bits(s, &n, 0, row("ExprNode/ConceptSpecializationExpr.bits"));
    satisfaction(s, &n, row("ExprNode/ConceptSpecializationExpr.aux"));
    ref_word(w, s, n.a, row("ExprNode/ConceptSpecializationExpr.a"));
    args_word(w, s, n.b);
}

/// `LambdaExpr`: `a` the closure type, written whole (§2.4).
pub(crate) fn lambda_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let n = expr::node(w, e);
    check_bits(s, &n, 0, row("ExprNode/LambdaExpr.bits"));
    check_aux(s, &n, 0, row("ExprNode/LambdaExpr.aux"));
    let Some(lambda) = LambdaId::try_new(n.a) else {
        s.uv(0).indeterminate(row("ExprNode/LambdaExpr.a"));
        return;
    };
    let closure = w.ast.exprs.lambdas.get(lambda).closure;
    crate::local::write_decl(w, s, closure);
}

/// `PackExpansionExpr`: `a` the pattern.
pub(crate) fn pack_expansion_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let n = expr::node(w, e);
    check_bits(s, &n, 0, row("ExprNode/PackExpansionExpr.bits"));
    check_aux(s, &n, 0, row("ExprNode/PackExpansionExpr.aux"));
    child_word(w, s, n.a, row("ExprNode/PackExpansionExpr.a"));
}

/// `CXXFoldExpr`: `aux` the operator, `a` the left operand, `b` the
/// right, either absent (the pack's side is the one present when the
/// fold is unary).
pub(crate) fn cxx_fold_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let n = expr::node(w, e);
    check_bits(s, &n, 0, row("ExprNode/CXXFoldExpr.bits"));
    match BinaryOp::from_u8(n.aux) {
        Some(op) => {
            s.tag(binary_op::row(&op));
        }
        None => {
            s.uv(0).indeterminate(row("ExprNode/CXXFoldExpr.aux"));
        }
    }
    expr::child_opt(w, s, ExprId::try_new(n.a));
    expr::child_opt(w, s, ExprId::try_new(n.b));
}

/// `PackIndexingExpr`: `a` the pack, `b` the index.
pub(crate) fn pack_indexing_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let n = expr::node(w, e);
    check_bits(s, &n, 0, row("ExprNode/PackIndexingExpr.bits"));
    check_aux(s, &n, 0, row("ExprNode/PackIndexingExpr.aux"));
    child_word(w, s, n.a, row("ExprNode/PackIndexingExpr.a"));
    child_word(w, s, n.b, row("ExprNode/PackIndexingExpr.b"));
}

/// `CXXUnresolvedConstructExpr`: `aux` (braced), `a` the arguments; the
/// type written is `ColdExprData` `cast_written`.
pub(crate) fn cxx_unresolved_construct_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let n = expr::node(w, e);
    check_bits(s, &n, 0, row("ExprNode/CXXUnresolvedConstructExpr.bits"));
    check_aux(s, &n, 1, row("ExprNode/CXXUnresolvedConstructExpr.aux"));
    s.bool(n.aux & 1 != 0);
    let es = expr::expr_list(w, s, n.a, row("ExprNode/CXXUnresolvedConstructExpr.a"));
    expr::children(w, s, &es);
}

/// `CXXExpansionSelectExpr`: `aux` the form, `a` the depth.
pub(crate) fn cxx_expansion_select_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let n = expr::node(w, e);
    check_bits(s, &n, 0, row("ExprNode/CXXExpansionSelectExpr.bits"));
    check_aux(s, &n, 3, row("ExprNode/CXXExpansionSelectExpr.aux"));
    let form = ExpansionForm::from_aux(u16::from(n.aux));
    s.tag(expansion_form::row(&form));
    s.uv(u64::from(n.a));
}

/// `IntegerPackExpr`: `a` the count.
pub(crate) fn integer_pack_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let n = expr::node(w, e);
    check_bits(s, &n, 0, row("ExprNode/IntegerPackExpr.bits"));
    check_aux(s, &n, 0, row("ExprNode/IntegerPackExpr.aux"));
    child_word(w, s, n.a, row("ExprNode/IntegerPackExpr.a"));
}

/// `profile`: a canonical expression operand (§4.4), the expression
/// `ProfileEntry.first_expr`. Kiln interns every profile with its first
/// expression (`TypeContext::profile`, `kiln_ast` `types/context.rs:1422`);
/// only the table's sentinel entry 0, which no `ProfileId` names, has
/// none.
pub(crate) fn profile(w: &mut Walk<'_>, s: &mut Stream, p: ProfileId) {
    match w.ast.types.profile_expr(p) {
        Some(e) => profile_expr(w, s, e),
        None => {
            s.uv(0).indeterminate(Cause::Unaccounted("ProfileEntry/first_expr"));
        }
    }
}

/// `profile` given by its expression (a function's trailing
/// requires-clause in its discriminator, digest-kinds §3.3; a template
/// head's requires-clause in a key, §3.4): the canonical operand of that
/// expression (§4.4), walked in profile mode (`expr_profile.rs`) with the
/// stream's expression state put aside and a fresh [`Frame`], both
/// restored after.
///
/// Whether a local entity took the unit's ordinal is read from
/// `UnitState::local_uses`, which a nested profile or type digest raises
/// too, so it carries to every enclosing frame and to the unit (F7).
/// Whether the profile left out a fact (`Walk::left_out`) is read the same
/// way: outside a type and a key the unit is `Pending(Area::Spelling)`.
pub(crate) fn profile_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let saved = w.take_expr_state();
    let uses = w.unit.local_uses;
    let left_out = w.left_out;
    w.unit.expr.profile = Some(Frame::default());
    crate::expr_profile::walk(w, s, e);
    let frame = w.unit.expr.profile.take().unwrap_or_default();
    w.unit.expr = saved;
    if w.left_out != left_out && w.type_nesting == 0 && w.in_key == 0 {
        // Written straight into a unit stream (a template argument, a
        // pattern's table row): the left-out fact is the unit's, as for a
        // marked type (`Walk::absorb_type`).
        s.absorb(Some(Cause::Pending(crate::Area::Spelling)));
    }
    if w.unit.local_uses != uses && !frame.locals.is_empty() {
        // Two numberings in one stream: an ordinal of the frame and one of
        // the unit could be equal.
        s.indeterminate(Cause::Unaccounted("profile/local-mix"));
    }
}

/// A reference to local object `d` while a profile is walked
/// (`Walk::write_ref`, §4.4): an entity declared inside the profile is
/// `Ref::local` with the frame's number; a function parameter declared
/// outside it `Ref::param`; any other local entity the unit's ordinal
/// (none in a key).
pub(crate) fn profile_local(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    let first = w.ast.first(d);
    let inside = w.unit.expr.profile.as_ref().and_then(|f| f.locals.get(&first).copied());
    if let Some(k) = inside {
        s.uv(REF_LOCAL).uv(k);
        return;
    }
    if w.ast.decls.get(d).kind() == DeclKind::ParmVar {
        param_ref(w, s, d, Cause::Unaccounted("profile/param"));
        return;
    }
    if w.in_key > 0 {
        s.uv(REF_NONE).indeterminate(Cause::Unaccounted("key/local-ref"));
        return;
    }
    let k = crate::local::ordinal(w, d);
    w.unit.local_uses += 1;
    s.uv(REF_LOCAL).uv(k);
}

#[cfg(test)]
#[path = "expr_dep_tests.rs"]
mod tests;
