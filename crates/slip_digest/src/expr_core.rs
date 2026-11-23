//! The core expression kinds (`spec/digest-kinds.md` §7, WP-031): the
//! rows of each kind, as [`Item`]s for the walk of `expr.rs`. A function
//! here writes nothing to the stream except a cause; what it puts in
//! `out` is written in order once the common rows are.

use slip_kiln::kiln::ast::expr::{
    BinaryOp, CastKind, CharKind, DeclRefExtra, ExprFlags, NonOdrUse, Radix, StorageDuration,
    StringKind, UnaryOp,
};
use slip_kiln::kiln::ast::names::OverloadedOperator;
use slip_kiln::kiln::ast::{ArgListId, ConstValueId, DeclId, ExprId, ListId, StringLitId};
use slip_model::keys::KeyKind;

use crate::Cause;
use crate::expr::{Item, Node, check_aux, check_bits, expr_list};
use crate::keys::{Role, info};
use crate::stream::{FlagWord, Stream};
use crate::tables::{
    binary_op, cast_kind, char_kind, non_odr_use, overloaded_operator, storage_duration,
    string_kind, unary_op,
};
use crate::walk::Walk;

/// The bit of a per-kind flag, in place (bits 4 to 7).
const fn bit(f: ExprFlags) -> u8 {
    f.bits()
}

/// `true` when the per-kind flag `f` is set.
const fn has(n: &Node, f: ExprFlags) -> bool {
    n.flags & f.bits() != 0
}

/// A child word: [`Item::Node`], or a cause and the placeholder when
/// the word is 0.
fn child(out: &mut Vec<Item>, raw: u32, cause: Cause) {
    if let Some(e) = ExprId::try_new(raw) {
        out.push(Item::Node(e, false));
    } else {
        out.push(Item::Uv(0));
        out.push(Item::Cause(cause));
    }
}

/// A `ref` word: [`Item::Ref`], or `Ref::none` and a cause when it is 0.
fn entity(out: &mut Vec<Item>, raw: u32, cause: Cause) {
    if let Some(d) = DeclId::try_new(raw) {
        out.push(Item::Ref(d));
    } else {
        out.push(Item::RefOpt(None));
        out.push(Item::Cause(cause));
    }
}

/// `child*` of the list in a word.
fn child_list(w: &Walk<'_>, s: &mut Stream, out: &mut Vec<Item>, raw: u32, cause: Cause) {
    let es = expr_list(w, s, raw, cause);
    out.push(Item::Count(es.len()));
    out.extend(es.into_iter().map(|e| Item::Node(e, false)));
}

// ---------------------------------------------------------------------
// Literals.

/// `IntegerLiteral`: `bits-wide`, then the value: `a-low`, `b-high` when
/// not wide, `a-wide` when wide. `aux` (`Radix`) is `out`.
pub(crate) fn integer_literal(_w: &Walk<'_>, s: &mut Stream, n: &Node, out: &mut Vec<Item>) {
    check_bits(s, n, bit(ExprFlags::WIDE), Cause::Unaccounted("ExprNode/IntegerLiteral.bits"));
    radix(s, n.aux);
    let wide = has(n, ExprFlags::WIDE);
    out.push(Item::Flags(FlagWord::new().bit(wide)));
    if wide {
        value(out, n.a, Cause::Unaccounted("ExprNode/IntegerLiteral.a-wide"));
    } else {
        out.push(Item::Uv(u64::from(n.a)));
        out.push(Item::Uv(u64::from(n.b)));
    }
}

/// `aux` of an `IntegerLiteral` is a `Radix`, whose rows are `out`: any
/// other value is unaccounted.
fn radix(s: &mut Stream, aux: u8) {
    match Radix::from_u8(aux) {
        Some(Radix::Decimal | Radix::Hex | Radix::Octal | Radix::Binary) => {}
        None => {
            s.indeterminate(Cause::Unaccounted("ExprNode/IntegerLiteral.aux"));
        }
    }
}

/// A `value` word.
fn value(out: &mut Vec<Item>, raw: u32, cause: Cause) {
    if let Some(v) = ConstValueId::try_new(raw) {
        out.push(Item::Value(v));
    } else {
        out.push(Item::Uv(0));
        out.push(Item::Cause(cause));
    }
}

/// `FloatingLiteral.a`: `value`.
pub(crate) fn floating_literal(s: &mut Stream, n: &Node, out: &mut Vec<Item>) {
    check_bits(s, n, 0, Cause::Unaccounted("ExprNode/FloatingLiteral.bits"));
    value(out, n.a, Cause::Unaccounted("ExprNode/FloatingLiteral.a"));
}

/// `CharacterLiteral`: `aux` (`CharKind`), then `a`, the code unit.
pub(crate) fn character_literal(s: &mut Stream, n: &Node, out: &mut Vec<Item>) {
    check_bits(s, n, 0, Cause::Unaccounted("ExprNode/CharacterLiteral.bits"));
    if let Some(k) = CharKind::from_u8(n.aux) {
        out.push(Item::Tag(char_kind::row(&k)));
    } else {
        out.push(Item::Uv(0));
        out.push(Item::Cause(Cause::Unaccounted("ExprNode/CharacterLiteral.aux")));
    }
    out.push(Item::Uv(u64::from(n.a)));
}

/// `StringLiteral`: `aux` (`StringKind`), then `a`, the string.
pub(crate) fn string_literal(s: &mut Stream, n: &Node, out: &mut Vec<Item>) {
    check_bits(s, n, 0, Cause::Unaccounted("ExprNode/StringLiteral.bits"));
    if let Some(k) = StringKind::from_u8(n.aux) {
        out.push(Item::Tag(string_kind::row(&k)));
    } else {
        out.push(Item::Uv(0));
        out.push(Item::Cause(Cause::Unaccounted("ExprNode/StringLiteral.aux")));
    }
    if let Some(l) = StringLitId::try_new(n.a) {
        out.push(Item::String(l));
    } else {
        out.push(Item::Uv(0));
        out.push(Item::Cause(Cause::Unaccounted("ExprNode/StringLiteral.a")));
    }
}

/// `CXXBoolLiteralExpr.a`: `bool`.
pub(crate) fn bool_literal(s: &mut Stream, n: &Node, out: &mut Vec<Item>) {
    check_bits(s, n, 0, Cause::Unaccounted("ExprNode/CXXBoolLiteralExpr.bits"));
    if n.a > 1 {
        s.indeterminate(Cause::Unaccounted("ExprNode/CXXBoolLiteralExpr.a"));
    }
    out.push(Item::Bool(n.a != 0));
}

/// `ImaginaryLiteral.a`: `child`.
pub(crate) fn imaginary_literal(s: &mut Stream, n: &Node, out: &mut Vec<Item>) {
    check_bits(s, n, 0, Cause::Unaccounted("ExprNode/ImaginaryLiteral.bits"));
    child(out, n.a, Cause::Unaccounted("ExprNode/ImaginaryLiteral.a"));
}

// ---------------------------------------------------------------------
// Names.

/// The `NonOdrUse` of `aux`.
fn non_odr(out: &mut Vec<Item>, aux: u8, cause: Cause) {
    if let Some(u) = NonOdrUse::from_u8(aux) {
        out.push(Item::Tag(non_odr_use::row(&u)));
    } else {
        out.push(Item::Uv(0));
        out.push(Item::Cause(cause));
    }
}

/// `DeclRefExpr`: the flag word (`names-closure-copy`,
/// `refers-to-enclosing`, `immediate-escalating`; `had-multiple-candidates`
/// is `out`), `aux` (`NonOdrUse`), `a` the entity, `b` the `DeclRefExtra`.
pub(crate) fn decl_ref(w: &Walk<'_>, s: &mut Stream, e: ExprId, n: &Node, out: &mut Vec<Item>) {
    check_bits(
        s,
        n,
        bit(ExprFlags::NAMES_CLOSURE_COPY)
            | bit(ExprFlags::REFERS_TO_ENCLOSING)
            | bit(ExprFlags::HAD_MULTIPLE_CANDIDATES)
            | bit(ExprFlags::IMMEDIATE_ESCALATING),
        Cause::Unaccounted("ExprNode/DeclRefExpr.bits"),
    );
    out.push(Item::Flags(
        FlagWord::new()
            .bit(has(n, ExprFlags::NAMES_CLOSURE_COPY))
            .bit(has(n, ExprFlags::REFERS_TO_ENCLOSING))
            .bit(has(n, ExprFlags::IMMEDIATE_ESCALATING)),
    ));
    non_odr(out, n.aux, Cause::Unaccounted("ExprNode/DeclRefExpr.aux"));
    entity(out, n.a, Cause::Unaccounted("ExprNode/DeclRefExpr.a"));
    let args = w.ast.exprs.decl_ref_extra(e).and_then(|x| {
        let &DeclRefExtra { qualifier: _, template_args, found_decl: _ } = x;
        template_args
    });
    decl_ref_extra(w, DeclId::try_new(n.a), args, out);
    out.push(Item::Exec(crate::own_spelling::name_parts, e));
    out.push(Item::Exec(local_value, e));
}

/// After a `DeclRefExpr`, where name-derived values are read by value
/// ([`Walk::by_value`]) and only there: `value?`, the evaluated value of
/// the block-scope variable it names, by value; nothing for any other
/// entity. Such a variable's own stream writes its value in the mode of
/// the function (`decl_func::var_data`), which passes a name on; a
/// `static_assert` condition, a key or a type that reads the variable
/// reads its value (digest §9).
fn local_value(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    if w.by_value == 0 {
        return;
    }
    let ast = w.ast;
    let Some(var) = DeclId::try_new(ast.exprs.get(e).a()) else { return };
    let var = ast.strip_shadow(var);
    if ast.decls.get(var).kind() != slip_kiln::kiln::ast::decl::DeclKind::Var
        || !w.is_local(var)
        || w.is_local_extern(var)
    {
        return;
    }
    match crate::decl_func::var_eval(w, var).state() {
        slip_kiln::kiln::ast::decl::VarEvalState::Value(eval) => {
            s.opt(true);
            crate::value::value(w, s, eval);
        }
        _ => {
            s.opt(false);
        }
    }
}

/// `record DeclRefExtra` (§7.3) of a name of `named`: `template_args`
/// (`enum ArgView*?`) only when the entity named is a template;
/// `qualifier` and `found_decl` are `out`.
pub(crate) fn decl_ref_extra(
    w: &Walk<'_>,
    named: Option<DeclId>,
    template_args: Option<ArgListId>,
    out: &mut Vec<Item>,
) {
    let Some(d) = named else { return };
    if !names_template(w, d) {
        return;
    }
    out.push(Item::Opt(template_args.is_some()));
    if let Some(l) = template_args {
        out.push(Item::Args(l));
    }
}

/// Whether `d` (through a using-shadow, from its entity) is a template:
/// its arguments as written select what the name means (digest §6).
pub(crate) fn names_template(w: &Walk<'_>, d: DeclId) -> bool {
    let ast = w.ast;
    let e = w.entity(ast.strip_shadow(d));
    let kind = ast.decls.get(e).kind();
    // `KeyKind` is Slip's own enumeration (digest §13 obligation 2
    // concerns Kiln's).
    matches!(
        info(kind).role,
        Role::Entity(
            KeyKind::ClassTemplate
                | KeyKind::FunctionTemplate
                | KeyKind::VariableTemplate
                | KeyKind::AliasTemplate
                | KeyKind::Concept
        )
    )
}

/// `MemberExpr`: the flag word (`arrow`; `spliced` is `unsupported`,
/// `had-multiple-candidates` `out`), `aux` (`NonOdrUse`), `a` the object,
/// `b` the member (the first candidate of an overloaded access).
pub(crate) fn member(s: &mut Stream, n: &Node, out: &mut Vec<Item>) {
    check_bits(
        s,
        n,
        bit(ExprFlags::ARROW) | bit(ExprFlags::SPLICED) | bit(ExprFlags::HAD_MULTIPLE_CANDIDATES),
        Cause::Unaccounted("ExprNode/MemberExpr.bits"),
    );
    if has(n, ExprFlags::SPLICED) {
        s.indeterminate(Cause::Unsupported("ExprNode/MemberExpr.bits-spliced"));
    }
    out.push(Item::Flags(FlagWord::new().bit(has(n, ExprFlags::ARROW))));
    non_odr(out, n.aux, Cause::Unaccounted("ExprNode/MemberExpr.aux"));
    child(out, n.a, Cause::Unaccounted("ExprNode/MemberExpr.a"));
    entity(out, n.b, Cause::Unaccounted("ExprNode/MemberExpr.b"));
}

/// `CXXThisExpr.aux-implicit`: bit 0 of `aux`, in the flag word, written
/// only inside an instantiation-dependent expression (`dependent`: the
/// node itself or one around it; WP-031 landed file), else the empty
/// form. Outside one, only diagnostics read the bit (pitfalls E07).
pub(crate) fn this(s: &mut Stream, n: &Node, dependent: bool, out: &mut Vec<Item>) {
    check_bits(s, n, 0, Cause::Unaccounted("ExprNode/CXXThisExpr.bits"));
    check_aux(s, n, 1, Cause::Unaccounted("ExprNode/CXXThisExpr.aux"));
    out.push(Item::Flags(FlagWord::new().bit(dependent && n.aux & 1 != 0)));
}

// ---------------------------------------------------------------------
// Operators.

/// `UnaryOperator`: the flag word (`cannot-overflow`), `aux`, `a`.
pub(crate) fn unary(s: &mut Stream, n: &Node, out: &mut Vec<Item>) {
    check_bits(
        s,
        n,
        bit(ExprFlags::CANNOT_OVERFLOW),
        Cause::Unaccounted("ExprNode/UnaryOperator.bits"),
    );
    out.push(Item::Flags(FlagWord::new().bit(has(n, ExprFlags::CANNOT_OVERFLOW))));
    if let Some(op) = UnaryOp::from_u8(n.aux) {
        out.push(Item::Tag(unary_op::row(&op)));
    } else {
        out.push(Item::Uv(0));
        out.push(Item::Cause(Cause::Unaccounted("ExprNode/UnaryOperator.aux")));
    }
    child(out, n.a, Cause::Unaccounted("ExprNode/UnaryOperator.a"));
}

/// The `BinaryOp` of `aux`.
fn binary_op_row(out: &mut Vec<Item>, aux: u8, cause: Cause) {
    if let Some(op) = BinaryOp::from_u8(aux) {
        out.push(Item::Tag(binary_op::row(&op)));
    } else {
        out.push(Item::Uv(0));
        out.push(Item::Cause(cause));
    }
}

/// `BinaryOperator`: `aux`, `a`, `b`.
pub(crate) fn binary(s: &mut Stream, n: &Node, out: &mut Vec<Item>) {
    check_bits(s, n, 0, Cause::Unaccounted("ExprNode/BinaryOperator.bits"));
    binary_op_row(out, n.aux, Cause::Unaccounted("ExprNode/BinaryOperator.aux"));
    child(out, n.a, Cause::Unaccounted("ExprNode/BinaryOperator.a"));
    child(out, n.b, Cause::Unaccounted("ExprNode/BinaryOperator.b"));
}

/// `CompoundAssignOperator`: `aux`, `a`, then `b`, the
/// `CompoundAssignExtra` (`rhs`, `lhs_ty`, `result_ty`).
pub(crate) fn compound_assign(
    w: &Walk<'_>,
    s: &mut Stream,
    e: ExprId,
    n: &Node,
    out: &mut Vec<Item>,
) {
    check_bits(s, n, 0, Cause::Unaccounted("ExprNode/CompoundAssignOperator.bits"));
    binary_op_row(out, n.aux, Cause::Unaccounted("ExprNode/CompoundAssignOperator.aux"));
    child(out, n.a, Cause::Unaccounted("ExprNode/CompoundAssignOperator.a"));
    let &slip_kiln::kiln::ast::expr::CompoundAssignExtra { rhs, lhs_ty, result_ty } =
        w.ast.exprs.compound_assign(e);
    out.push(Item::Node(rhs, false));
    out.push(Item::Type(lhs_ty));
    out.push(Item::Type(result_ty));
}

/// `ConditionalOperator`: `a`, then `b-then` and `b-else`.
pub(crate) fn conditional(w: &Walk<'_>, s: &mut Stream, e: ExprId, n: &Node, out: &mut Vec<Item>) {
    check_bits(s, n, 0, Cause::Unaccounted("ExprNode/ConditionalOperator.bits"));
    child(out, n.a, Cause::Unaccounted("ExprNode/ConditionalOperator.a"));
    let [then, els] = w.ast.exprs.cond_arms(e);
    out.push(Item::Node(then, false));
    out.push(Item::Node(els, false));
}

/// `BinaryConditionalOperator.a`: `child*`, the common expression, the
/// condition and the two arms. The common expression is the source of
/// the opaque value the condition and the true arm name (`kiln_sema`
/// forms.rs:699, the only builder): it takes its opaque number before it
/// is written, so that the opaque value writes the number alone and the
/// common expression is written once (a left-nested `?:` stays linear).
pub(crate) fn binary_conditional(w: &Walk<'_>, s: &mut Stream, n: &Node, out: &mut Vec<Item>) {
    check_bits(s, n, 0, Cause::Unaccounted("ExprNode/BinaryConditionalOperator.bits"));
    let es = expr_list(w, s, n.a, Cause::Unaccounted("ExprNode/BinaryConditionalOperator.a"));
    out.push(Item::Count(es.len()));
    for (i, e) in es.into_iter().enumerate() {
        if i == 0 {
            out.push(Item::OpaqueSource(e));
        }
        out.push(Item::Node(e, false));
    }
}

/// `ArraySubscriptExpr`: `a`, `b`.
pub(crate) fn array_subscript(s: &mut Stream, n: &Node, out: &mut Vec<Item>) {
    check_bits(s, n, 0, Cause::Unaccounted("ExprNode/ArraySubscriptExpr.bits"));
    child(out, n.a, Cause::Unaccounted("ExprNode/ArraySubscriptExpr.a"));
    child(out, n.b, Cause::Unaccounted("ExprNode/ArraySubscriptExpr.b"));
}

/// `ParenExpr.a`, written only by the rule for parentheses (`b`, the
/// closing parenthesis, is `out`). Its operand is walked with the rule
/// again: inner parentheses are skipped.
pub(crate) fn paren(s: &mut Stream, n: &Node, out: &mut Vec<Item>) {
    check_bits(s, n, 0, Cause::Unaccounted("ExprNode/ParenExpr.bits"));
    child(out, n.a, Cause::Unaccounted("ExprNode/ParenExpr.a"));
}

// ---------------------------------------------------------------------
// Calls.

/// `CallExpr`: the flag word (`assume-ignored`; `adl` is `out`), `a` the
/// callee, `b` the arguments.
pub(crate) fn call(w: &Walk<'_>, s: &mut Stream, n: &Node, out: &mut Vec<Item>) {
    check_bits(
        s,
        n,
        bit(ExprFlags::ADL) | bit(ExprFlags::ASSUME_IGNORED),
        Cause::Unaccounted("ExprNode/CallExpr.bits"),
    );
    out.push(Item::Flags(FlagWord::new().bit(has(n, ExprFlags::ASSUME_IGNORED))));
    child(out, n.a, Cause::Unaccounted("ExprNode/CallExpr.a"));
    child_list(w, s, out, n.b, Cause::Unaccounted("ExprNode/CallExpr.b"));
}

/// `CXXMemberCallExpr`: `a` the callee, `b` the arguments.
pub(crate) fn member_call(w: &Walk<'_>, s: &mut Stream, n: &Node, out: &mut Vec<Item>) {
    check_bits(s, n, 0, Cause::Unaccounted("ExprNode/CXXMemberCallExpr.bits"));
    child(out, n.a, Cause::Unaccounted("ExprNode/CXXMemberCallExpr.a"));
    child_list(w, s, out, n.b, Cause::Unaccounted("ExprNode/CXXMemberCallExpr.b"));
}

/// `CXXOperatorCallExpr`: `aux` (the operator as written; `adl` is
/// `out`), `a` the callee, `b` the arguments.
pub(crate) fn operator_call(w: &Walk<'_>, s: &mut Stream, n: &Node, out: &mut Vec<Item>) {
    check_bits(s, n, bit(ExprFlags::ADL), Cause::Unaccounted("ExprNode/CXXOperatorCallExpr.bits"));
    if let Some(op) = OverloadedOperator::from_u8(n.aux) {
        out.push(Item::Tag(overloaded_operator::row(&op)));
    } else {
        out.push(Item::Uv(0));
        out.push(Item::Cause(Cause::Unaccounted("ExprNode/CXXOperatorCallExpr.aux")));
    }
    child(out, n.a, Cause::Unaccounted("ExprNode/CXXOperatorCallExpr.a"));
    child_list(w, s, out, n.b, Cause::Unaccounted("ExprNode/CXXOperatorCallExpr.b"));
}

// ---------------------------------------------------------------------
// Casts.

/// A cast: for an `ImplicitCastExpr` the flag word
/// (`part-of-explicit-cast`), then `aux` (`CastKind`), `a` the operand,
/// `b` the base path (`type*`).
pub(crate) fn cast(s: &mut Stream, n: &Node, implicit: bool, out: &mut Vec<Item>) {
    if implicit {
        check_bits(
            s,
            n,
            bit(ExprFlags::PART_OF_EXPLICIT_CAST),
            Cause::Unaccounted("ExprNode/ImplicitCastExpr.bits"),
        );
        out.push(Item::Flags(FlagWord::new().bit(has(n, ExprFlags::PART_OF_EXPLICIT_CAST))));
    } else {
        check_bits(s, n, 0, Cause::Unaccounted("ExprNode/cast.bits"));
    }
    if let Some(k) = CastKind::from_u8(n.aux) {
        out.push(Item::Tag(cast_kind::row(&k)));
    } else {
        out.push(Item::Uv(0));
        out.push(Item::Cause(Cause::Unaccounted("ExprNode/cast.aux")));
    }
    // The operand of an array-to-pointer conversion passes a name-derived
    // value through (digest §9, `expr_misc::name_type`).
    match ExprId::try_new(n.a) {
        Some(a) if implicit && CastKind::from_u8(n.aux) == Some(CastKind::ArrayToPointerDecay) => {
            out.push(Item::Decayed(a));
        }
        Some(_) | None => child(out, n.a, Cause::Unaccounted("ExprNode/cast.a")),
    }
    out.push(Item::BasePath(ListId::try_new(n.b)));
}

// ---------------------------------------------------------------------
// Temporaries and wrappers.

/// `MaterializeTemporaryExpr`: `aux-storage` (bits 0-1), the flag word
/// (`aux-lvalue-reference`, bit 2), `a`, then `b`: `decl?`, the
/// lifetime-extended temporary.
pub(crate) fn materialize(s: &mut Stream, n: &Node, out: &mut Vec<Item>) {
    check_bits(s, n, 0, Cause::Unaccounted("ExprNode/MaterializeTemporaryExpr.bits"));
    check_aux(s, n, 7, Cause::Unaccounted("ExprNode/MaterializeTemporaryExpr.aux"));
    if let Some(d) = StorageDuration::from_u8(n.aux & 3) {
        out.push(Item::Tag(storage_duration::row(&d)));
    } else {
        out.push(Item::Uv(0));
        out.push(Item::Cause(Cause::Unaccounted("ExprNode/MaterializeTemporaryExpr.aux-storage")));
    }
    out.push(Item::Flags(FlagWord::new().bit(n.aux & 4 != 0)));
    child(out, n.a, Cause::Unaccounted("ExprNode/MaterializeTemporaryExpr.a"));
    let extended = DeclId::try_new(n.b);
    out.push(Item::Opt(extended.is_some()));
    if let Some(d) = extended {
        out.push(Item::Decl(d));
    }
}

/// `CXXBindTemporaryExpr`: `a`, then `b`, the destructor.
pub(crate) fn bind_temporary(s: &mut Stream, n: &Node, out: &mut Vec<Item>) {
    check_bits(s, n, 0, Cause::Unaccounted("ExprNode/CXXBindTemporaryExpr.bits"));
    child(out, n.a, Cause::Unaccounted("ExprNode/CXXBindTemporaryExpr.a"));
    entity(out, n.b, Cause::Unaccounted("ExprNode/CXXBindTemporaryExpr.b"));
}

/// `ExprWithCleanups`: the flag word (`aux-side-effects`, bit 0), `a`.
pub(crate) fn cleanups(s: &mut Stream, n: &Node, out: &mut Vec<Item>) {
    check_bits(s, n, 0, Cause::Unaccounted("ExprNode/ExprWithCleanups.bits"));
    check_aux(s, n, 1, Cause::Unaccounted("ExprNode/ExprWithCleanups.aux"));
    out.push(Item::Flags(FlagWord::new().bit(n.aux & 1 != 0)));
    child(out, n.a, Cause::Unaccounted("ExprNode/ExprWithCleanups.a"));
}

/// `OpaqueValueExpr.a`: by the rule for opaque values (`expr.rs`).
pub(crate) fn opaque_value(s: &mut Stream, e: ExprId, n: &Node, out: &mut Vec<Item>) {
    check_bits(s, n, 0, Cause::Unaccounted("ExprNode/OpaqueValueExpr.bits"));
    out.push(Item::Opaque(e));
}

/// Raises [`Walk::by_value`] for the operand of a `ConstantExpr` with no
/// stored value ([`constant`]).
fn raise_by_value(w: &mut Walk<'_>, _: &mut Stream, _: ExprId) {
    w.by_value += 1;
}

/// Lowers what [`raise_by_value`] raised.
fn lower_by_value(w: &mut Walk<'_>, _: &mut Stream, _: ExprId) {
    w.by_value -= 1;
}

/// `ConstantExpr`: the flag word (`aux-elided`, `aux-value-hidden`), `a`,
/// then `b`: `value?`.
pub(crate) fn constant(s: &mut Stream, e: ExprId, n: &Node, out: &mut Vec<Item>) {
    check_bits(s, n, 0, Cause::Unaccounted("ExprNode/ConstantExpr.bits"));
    check_aux(s, n, 3, Cause::Unaccounted("ExprNode/ConstantExpr.aux"));
    out.push(Item::Flags(FlagWord::new().bit(n.aux & 1 != 0).bit(n.aux & 2 != 0)));
    let v = ConstValueId::try_new(n.b);
    // With no stored value, what the operand reads of a name is read by
    // value (digest §9); with one, the value is written and the operand
    // passes names on as anywhere.
    if v.is_none() {
        out.push(Item::Exec(raise_by_value, e));
    }
    child(out, n.a, Cause::Unaccounted("ExprNode/ConstantExpr.a"));
    if v.is_none() {
        out.push(Item::Exec(lower_by_value, e));
    }
    out.push(Item::Opt(v.is_some()));
    if let Some(v) = v {
        out.push(Item::Value(v));
    }
}
