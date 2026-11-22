//! The expression kinds of no other file (`spec/digest-kinds.md` §7,
//! WP-037): names and builtins of a position, traits, `offsetof`,
//! coroutines, the GNU and vector extensions, `_Generic` and the
//! pseudo-object rewrite; and the cold rows `array_trait`,
//! `offsetof_pattern-components` and `offsetof_layout` (§7.4).
//!
//! The rows of a kind's function (`expr::KindFn`): its kind's rows only,
//! in table order; `expr::child` writes the common rows before the call
//! and the `ColdExprData` rows after it (`lib.rs` "How to fill an
//! expression kind"). Every word is read from the node itself, never
//! through a Kiln accessor that panics on a word Kiln did not build: a
//! word no row accounts for is `Unaccounted`, and the stream stays well
//! formed (a placeholder takes the place of what could not be written).

use slip_kiln::kiln::ast::expr::{
    ArrayTraitExtra, ExprFlags, ExprKind, GenericControl, OffsetofComponent, OffsetofStep,
    PredefinedKind, SourceLocKind, SuspendKind, TraitKind, TypeTrait, TypeidOperand,
};
use slip_kiln::kiln::ast::{DeclId, ExprId, ListId, QualType, RawId, StmtId};
use slip_kiln::kiln::source::Loc;
use slip_kiln::kiln::source::symbol::Symbol;

use crate::Cause;
use crate::expr::{Node, check_aux, check_bits, child, child_opt, expr_list, node};
use crate::stream::{FlagWord, Stream};
use crate::tables::{
    Row, generic_control, offsetof_component, offsetof_step, predefined_kind, source_loc_kind,
    suspend_kind, trait_kind, type_trait, typeid_operand,
};
use crate::walk::Walk;

/// `true` when the per-kind flag `f` is set.
const fn has(n: &Node, f: ExprFlags) -> bool {
    n.flags & f.bits() != 0
}

/// `child` of a word: the expression, or the placeholder and `cause`
/// when the word is 0.
fn child_word(w: &mut Walk<'_>, s: &mut Stream, raw: u32, cause: Cause) {
    if let Some(e) = ExprId::try_new(raw) {
        child(w, s, e);
    } else {
        s.uv(0).indeterminate(cause);
    }
}

/// `type` of a word: the type, or the placeholder and `cause` when the
/// word is 0.
fn type_word(w: &mut Walk<'_>, s: &mut Stream, raw: u32, cause: Cause) {
    if let Some(q) = QualType::from_raw(raw) {
        w.write_type(s, q);
    } else {
        s.uv(0).indeterminate(cause);
    }
}

/// An `enum` row's tag, or the placeholder and `cause` when the value is
/// none of the enumeration's.
fn tag_or(s: &mut Stream, row: Option<Row>, cause: Cause) {
    if let Some(r) = row {
        s.tag(r);
    } else {
        s.uv(0).indeterminate(cause);
    }
}

/// `child*` of the list in a word.
fn child_list(w: &mut Walk<'_>, s: &mut Stream, raw: u32, cause: Cause) {
    let es = expr_list(w, s, raw, cause);
    crate::expr::children(w, s, &es);
}

/// The raw words of the list in `raw`; `None` (and `cause`) when the word
/// is 0.
fn list_words<'a>(w: &Walk<'a>, s: &mut Stream, raw: u32, cause: Cause) -> Option<&'a [u32]> {
    let Some(l) = ListId::try_new(raw) else {
        s.indeterminate(cause);
        return None;
    };
    let ast = w.ast;
    Some(ast.lists.get(l))
}

/// Numbers `e` as the source of an opaque value now, before it is written
/// as a child: an `OpaqueValueExpr` naming it then writes the number
/// alone (§7 "Opaque values", as `Item::OpaqueSource` does for
/// `BinaryConditionalOperator`).
fn opaque_source(w: &mut Walk<'_>, e: ExprId) {
    let unit = &mut w.unit.expr;
    if !unit.opaques.contains_key(&e) {
        unit.opaques.insert(e, unit.next_opaque);
        unit.next_opaque += 1;
    }
}

/// The source of `e` when it is an `OpaqueValueExpr` with one.
fn source_of(w: &Walk<'_>, e: ExprId) -> Option<ExprId> {
    let n = w.ast.exprs.get(e);
    if n.kind() == ExprKind::OpaqueValueExpr { ExprId::try_new(n.a()) } else { None }
}

/// A `bool` row stored as a word: `cause` when it is neither 0 nor 1.
fn bool_word(s: &mut Stream, raw: u32, cause: Cause) {
    if raw > 1 {
        s.indeterminate(cause);
    }
    s.bool(raw != 0);
}

// ---------------------------------------------------------------------
// Names and builtins of a position.

/// The `PredefinedKind` of `aux` (Kiln's `from_aux` reads `ALL` by
/// position, `kiln_ast` expr.rs:1528).
fn predefined(aux: u8) -> Option<PredefinedKind> {
    PredefinedKind::ALL.get(usize::from(aux)).copied()
}

/// The `SourceLocKind` of `aux` (expr.rs:1571, as [`predefined`]).
fn source_loc(aux: u8) -> Option<SourceLocKind> {
    SourceLocKind::ALL.get(usize::from(aux)).copied()
}

/// `PredefinedExpr`: `aux` (`PredefinedKind`); `a`, the string of the
/// name, is `symbolic`.
pub(crate) fn predefined_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let n = node(w, e);
    check_bits(s, &n, 0, Cause::Unaccounted("ExprNode/PredefinedExpr.bits"));
    tag_or(
        s,
        predefined(n.aux).map(|k| predefined_kind::row(&k)),
        Cause::Unaccounted("ExprNode/PredefinedExpr.aux"),
    );
}

/// `SourceLocExpr`: `aux` (`SourceLocKind`); `a`, the context, is
/// `symbolic`, and so are its `SourceLocValues` (§7.4 `source_loc`).
pub(crate) fn source_loc_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let n = node(w, e);
    check_bits(s, &n, 0, Cause::Unaccounted("ExprNode/SourceLocExpr.bits"));
    tag_or(
        s,
        source_loc(n.aux).map(|k| source_loc_kind::row(&k)),
        Cause::Unaccounted("ExprNode/SourceLocExpr.aux"),
    );
}

// ---------------------------------------------------------------------
// noexcept and typeid.

/// `CXXNoexceptExpr`: `a`, then `b`, the stored result.
pub(crate) fn cxx_noexcept_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let n = node(w, e);
    check_bits(s, &n, 0, Cause::Unaccounted("ExprNode/CXXNoexceptExpr.bits"));
    child_word(w, s, n.a, Cause::Unaccounted("ExprNode/CXXNoexceptExpr.a"));
    bool_word(s, n.b, Cause::Unaccounted("ExprNode/CXXNoexceptExpr.b"));
}

/// `CXXTypeidExpr`: the flag word (`aux-potentially-evaluated`), then `a`
/// as `enum TypeidOperand` (`bits-arg-is-type`, `out`, says which).
pub(crate) fn cxx_typeid_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let n = node(w, e);
    check_bits(
        s,
        &n,
        ExprFlags::ARG_IS_TYPE.bits(),
        Cause::Unaccounted("ExprNode/CXXTypeidExpr.bits"),
    );
    check_aux(s, &n, 1, Cause::Unaccounted("ExprNode/CXXTypeidExpr.aux"));
    s.flags(FlagWord::new().bit(n.aux & 1 != 0));
    let operand = if has(&n, ExprFlags::ARG_IS_TYPE) {
        QualType::from_raw(n.a).map(TypeidOperand::Type)
    } else {
        ExprId::try_new(n.a).map(TypeidOperand::Expr)
    };
    match operand {
        Some(op) => {
            s.tag(typeid_operand::row(&op));
            match op {
                TypeidOperand::Type(ty) => w.write_type(s, ty),
                TypeidOperand::Expr(operand) => child(w, s, operand),
            }
        }
        None => {
            s.uv(0).indeterminate(Cause::Unaccounted("ExprNode/CXXTypeidExpr.a"));
        }
    }
}

// ---------------------------------------------------------------------
// Traits.

/// `UnaryExprOrTypeTraitExpr`: the flag word (`bits-arg-is-type`), `aux`
/// (`TraitKind`), then `a-type` or `a-expr`.
pub(crate) fn unary_expr_or_type_trait_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let n = node(w, e);
    check_bits(
        s,
        &n,
        ExprFlags::ARG_IS_TYPE.bits(),
        Cause::Unaccounted("ExprNode/UnaryExprOrTypeTraitExpr.bits"),
    );
    let is_type = has(&n, ExprFlags::ARG_IS_TYPE);
    s.flags(FlagWord::new().bit(is_type));
    tag_or(
        s,
        TraitKind::from_u8(n.aux).map(|k| trait_kind::row(&k)),
        Cause::Unaccounted("ExprNode/UnaryExprOrTypeTraitExpr.aux"),
    );
    if is_type {
        type_word(w, s, n.a, Cause::Unaccounted("ExprNode/UnaryExprOrTypeTraitExpr.a-type"));
    } else {
        child_word(w, s, n.a, Cause::Unaccounted("ExprNode/UnaryExprOrTypeTraitExpr.a-expr"));
    }
}

/// `aux` of the three trait kinds: `TypeTrait`.
fn trait_aux(s: &mut Stream, aux: u8, cause: Cause) {
    tag_or(s, TypeTrait::from_u8(aux).map(|t| type_trait::row(&t)), cause);
}

/// `TypeTraitExpr`: `aux` (`TypeTrait`), `a` (`type*`), `b` the stored
/// result.
pub(crate) fn type_trait_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let n = node(w, e);
    check_bits(s, &n, 0, Cause::Unaccounted("ExprNode/TypeTraitExpr.bits"));
    trait_aux(s, n.aux, Cause::Unaccounted("ExprNode/TypeTraitExpr.aux"));
    let cause = Cause::Unaccounted("ExprNode/TypeTraitExpr.a");
    let words = list_words(w, s, n.a, cause).unwrap_or(&[]);
    let mut types = Vec::with_capacity(words.len());
    for &word in words {
        match QualType::from_raw(word) {
            Some(ty) => types.push(ty),
            None => {
                s.indeterminate(cause);
            }
        }
    }
    w.write_types(s, &types);
    s.uv(u64::from(n.b));
}

/// `ArrayTypeTraitExpr`: `aux` (`TypeTrait`), `a` the queried type; the
/// dimension and the result are the cold row `array_trait`.
pub(crate) fn array_type_trait_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let n = node(w, e);
    check_bits(s, &n, 0, Cause::Unaccounted("ExprNode/ArrayTypeTraitExpr.bits"));
    trait_aux(s, n.aux, Cause::Unaccounted("ExprNode/ArrayTypeTraitExpr.aux"));
    type_word(w, s, n.a, Cause::Unaccounted("ExprNode/ArrayTypeTraitExpr.a"));
}

/// `ExpressionTraitExpr`: `aux` (`TypeTrait`), `a`, `b` the stored result.
pub(crate) fn expression_trait_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let n = node(w, e);
    check_bits(s, &n, 0, Cause::Unaccounted("ExprNode/ExpressionTraitExpr.bits"));
    trait_aux(s, n.aux, Cause::Unaccounted("ExprNode/ExpressionTraitExpr.aux"));
    child_word(w, s, n.a, Cause::Unaccounted("ExprNode/ExpressionTraitExpr.a"));
    s.uv(u64::from(n.b));
}

// ---------------------------------------------------------------------
// offsetof.

/// `OffsetOfExpr`: `aux`, `a` the indices, `b` the stored result.
///
/// `aux` is Kiln's `invalid_record` (`kiln_sema` gnu.rs:336-385,
/// `kiln_ast` builder.rs:3530): the designator passes through an invalid
/// class, which comes with an error diagnostic. The table's note
/// ("not standard-layout") is wrong about Kiln (WP-037 landed file): the
/// bool is written, and when it is set the unit is indeterminate
/// (`Cause::Error`, digest §10).
pub(crate) fn offset_of_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let n = node(w, e);
    check_bits(s, &n, 0, Cause::Unaccounted("ExprNode/OffsetOfExpr.bits"));
    bool_word(s, u32::from(n.aux), Cause::Unaccounted("ExprNode/OffsetOfExpr.aux"));
    if n.aux != 0 {
        s.indeterminate(Cause::Error);
    }
    child_list(w, s, n.a, Cause::Unaccounted("ExprNode/OffsetOfExpr.a"));
    s.uv(u64::from(n.b));
}

/// `ColdExprData` `offsetof_pattern-components` (§7.4) of `e`, which has
/// an `offsetof_pattern` entry: `enum OffsetofComponent*` (`expr::child`
/// wrote `offsetof_pattern-type`).
pub(crate) fn offsetof_components(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let ast = w.ast;
    let components: &[OffsetofComponent] =
        ast.exprs.cold.offsetof_pattern.get(e).map_or(&[], |(_, c)| &**c);
    s.count(components.len());
    for c in components {
        s.tag(offsetof_component::row(c));
        match *c {
            OffsetofComponent::Field { name, start: _, end: _ } => w.write_sym(s, name),
            OffsetofComponent::Index { index, start: _, end: _ } => child(w, s, index),
        }
    }
}

/// `ColdExprData` `offsetof_layout` (§7.4) of `e`, which has an entry:
/// `enum OffsetofStep*`.
pub(crate) fn offsetof_layout(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let ast = w.ast;
    let steps: &[OffsetofStep] = ast.exprs.cold.offsetof_layout.get(e).map_or(&[], |v| &**v);
    s.count(steps.len());
    for step in steps {
        s.tag(offsetof_step::row(step));
        match *step {
            OffsetofStep::Offset(bytes) => {
                s.uv(bytes);
            }
            OffsetofStep::Index { index, elem_size } => {
                child(w, s, index);
                s.uv(elem_size);
            }
        }
    }
}

/// `ColdExprData` `array_trait` (§7.4) of `e`, which has an entry: the
/// rows of `record ArrayTraitExtra` (`expr::child` wrote the presence).
pub(crate) fn array_trait_extra(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let ast = w.ast;
    let ArrayTraitExtra { dim, value } = ast
        .exprs
        .cold
        .array_trait
        .get(e)
        .copied()
        .unwrap_or(ArrayTraitExtra { dim: None, value: 0 });
    child_opt(w, s, dim);
    s.uv(value);
}

// ---------------------------------------------------------------------
// Coroutines.

/// The `SuspendKind` of bits 1-2 of `aux` (Kiln's numbering,
/// `kiln_ast` expr.rs:3542; 3 is none).
fn suspend(aux: u8) -> Option<SuspendKind> {
    match (aux >> 1) & 3 {
        0 => Some(SuspendKind::Void),
        1 => Some(SuspendKind::Bool),
        2 => Some(SuspendKind::Handle),
        _ => None,
    }
}

/// `CoawaitExpr` and `CoyieldExpr`: the flag word (`aux-implicit`),
/// `aux-suspend`, then `a`.
///
/// `a` of a resolved expression is the first five elements of the list
/// `[operand, common, ready, suspend, resume, opaque]` (`kiln_sema`
/// coroutine.rs:716, `kiln_ast` builder.rs:3345). The sixth is the
/// `OpaqueValueExpr` whose source is `common`, and the three calls name
/// it: `common` takes its opaque number before it is written, so the
/// calls' opaque values write the number alone and `common` is written
/// once. A `CoyieldExpr` of a template pattern holds the operand alone
/// (builder.rs:3376), written as a list of one.
fn coawait(w: &mut Walk<'_>, s: &mut Stream, e: ExprId, yield_: bool) {
    let (bits, aux, suspend_row, list) = if yield_ {
        (
            "ExprNode/CoyieldExpr.bits",
            "ExprNode/CoyieldExpr.aux",
            "ExprNode/CoyieldExpr.aux-suspend",
            "ExprNode/CoyieldExpr.a",
        )
    } else {
        (
            "ExprNode/CoawaitExpr.bits",
            "ExprNode/CoawaitExpr.aux",
            "ExprNode/CoawaitExpr.aux-suspend",
            "ExprNode/CoawaitExpr.a",
        )
    };
    let n = node(w, e);
    check_bits(s, &n, 0, Cause::Unaccounted(bits));
    check_aux(s, &n, 7, Cause::Unaccounted(aux));
    s.flags(FlagWord::new().bit(n.aux & 1 != 0));
    tag_or(s, suspend(n.aux).map(|k| suspend_kind::row(&k)), Cause::Unaccounted(suspend_row));
    let es = expr_list(w, s, n.a, Cause::Unaccounted(list));
    match es.len() {
        6 => {
            s.count(5);
            for (i, &x) in es[..5].iter().enumerate() {
                if i == 1 {
                    opaque_source(w, x);
                }
                child(w, s, x);
            }
        }
        1 if yield_ => crate::expr::children(w, s, &es),
        _ => {
            s.indeterminate(Cause::Unaccounted(list));
            crate::expr::children(w, s, &es);
        }
    }
}

/// `CoawaitExpr` (see [`coawait`]).
pub(crate) fn coawait_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    coawait(w, s, e, false);
}

/// `DependentCoawaitExpr`: `a`, the operand.
pub(crate) fn dependent_coawait_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let n = node(w, e);
    check_bits(s, &n, 0, Cause::Unaccounted("ExprNode/DependentCoawaitExpr.bits"));
    check_aux(s, &n, 0, Cause::Unaccounted("ExprNode/DependentCoawaitExpr.aux"));
    child_word(w, s, n.a, Cause::Unaccounted("ExprNode/DependentCoawaitExpr.a"));
}

/// `CoyieldExpr` (see [`coawait`]).
pub(crate) fn coyield_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    coawait(w, s, e, true);
}

// ---------------------------------------------------------------------
// GNU and C extensions.

/// `StmtExpr.a`: the compound statement, by the statements' walk.
pub(crate) fn stmt_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let n = node(w, e);
    check_bits(s, &n, 0, Cause::Unaccounted("ExprNode/StmtExpr.bits"));
    if let Some(st) = StmtId::try_new(n.a) {
        crate::stmt::child(w, s, st);
    } else {
        s.uv(0).indeterminate(Cause::Unaccounted("ExprNode/StmtExpr.a"));
    }
}

/// `AddrLabelExpr.a`: the label.
pub(crate) fn addr_label_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let n = node(w, e);
    check_bits(s, &n, 0, Cause::Unaccounted("ExprNode/AddrLabelExpr.bits"));
    if let Some(d) = DeclId::try_new(n.a) {
        w.write_ref(s, d);
    } else {
        w.write_ref_opt(s, None);
        s.indeterminate(Cause::Unaccounted("ExprNode/AddrLabelExpr.a"));
    }
}

/// `ChooseExpr`: `aux` the arm chosen, `a` the condition, then the two
/// arms of `cond_arms[b]`.
pub(crate) fn choose_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let n = node(w, e);
    check_bits(s, &n, 0, Cause::Unaccounted("ExprNode/ChooseExpr.bits"));
    if n.aux > 2 {
        s.indeterminate(Cause::Unaccounted("ExprNode/ChooseExpr.aux"));
    }
    s.uv(u64::from(n.aux));
    if n.a == 0 {
        // `choose_operands` would panic on the condition word.
        s.uv(0).indeterminate(Cause::Unaccounted("ExprNode/ChooseExpr.a"));
        s.uv(0).uv(0);
        return;
    }
    let [cond, first, second] = w.ast.exprs.choose_operands(e);
    child(w, s, cond);
    child(w, s, first);
    child(w, s, second);
}

/// `VAArgExpr`: `a` the `va_list`, `b` the written type.
pub(crate) fn va_arg_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let n = node(w, e);
    check_bits(s, &n, 0, Cause::Unaccounted("ExprNode/VAArgExpr.bits"));
    child_word(w, s, n.a, Cause::Unaccounted("ExprNode/VAArgExpr.a"));
    type_word(w, s, n.b, Cause::Unaccounted("ExprNode/VAArgExpr.b"));
}

/// The operations an `AtomicExpr` names, in Kiln's numbering
/// (`kiln_ast` decl.rs:4879 `builtin::atomic`, `is_op` decl.rs:5007):
/// the number is what the digest writes, so the unit test
/// `atomic_numbering_is_kilns` holds each against Kiln's constant.
const ATOMIC_OPS: [(&str, u32); 38] = [
    ("ADD_FETCH", 1),
    ("AND_FETCH", 2),
    ("CMPXCHG", 3),
    ("CMPXCHG_N", 4),
    ("EXCHANGE", 5),
    ("EXCHANGE_N", 6),
    ("FETCH_ADD", 7),
    ("FETCH_AND", 8),
    ("FETCH_MAX", 9),
    ("FETCH_MIN", 10),
    ("FETCH_NAND", 11),
    ("FETCH_OR", 12),
    ("FETCH_SUB", 13),
    ("FETCH_XOR", 14),
    ("LOAD", 15),
    ("LOAD_N", 16),
    ("MAX_FETCH", 17),
    ("MIN_FETCH", 18),
    ("NAND_FETCH", 19),
    ("OR_FETCH", 20),
    ("STORE", 21),
    ("STORE_N", 22),
    ("SUB_FETCH", 23),
    ("XOR_FETCH", 24),
    ("C11_CMPXCHG_STRONG", 25),
    ("C11_CMPXCHG_WEAK", 26),
    ("C11_EXCHANGE", 27),
    ("C11_FETCH_ADD", 28),
    ("C11_FETCH_AND", 29),
    ("C11_FETCH_MAX", 30),
    ("C11_FETCH_MIN", 31),
    ("C11_FETCH_NAND", 32),
    ("C11_FETCH_OR", 33),
    ("C11_FETCH_SUB", 34),
    ("C11_FETCH_XOR", 35),
    ("C11_INIT", 36),
    ("C11_LOAD", 37),
    ("C11_STORE", 38),
];

/// `AtomicExpr`: `aux` the operation, in Kiln's numbering (a number of
/// [`ATOMIC_OPS`], else `Unaccounted`), then `a` the operands.
pub(crate) fn atomic_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let n = node(w, e);
    check_bits(s, &n, 0, Cause::Unaccounted("ExprNode/AtomicExpr.bits"));
    let op = u32::from(n.aux);
    if !ATOMIC_OPS.iter().any(|&(_, v)| v == op) {
        s.indeterminate(Cause::Unaccounted("ExprNode/AtomicExpr.aux"));
    }
    s.uv(u64::from(op));
    child_list(w, s, n.a, Cause::Unaccounted("ExprNode/AtomicExpr.a"));
}

/// `ShuffleVectorExpr`: the flag word (`aux-mask-known`), `a` the
/// operands, then `b` the folded indices when the mask is known.
pub(crate) fn shuffle_vector_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let n = node(w, e);
    check_bits(s, &n, 0, Cause::Unaccounted("ExprNode/ShuffleVectorExpr.bits"));
    check_aux(s, &n, 1, Cause::Unaccounted("ExprNode/ShuffleVectorExpr.aux"));
    let known = n.aux & 1 != 0;
    s.flags(FlagWord::new().bit(known));
    child_list(w, s, n.a, Cause::Unaccounted("ExprNode/ShuffleVectorExpr.a"));
    if known {
        let mask = list_words(w, s, n.b, Cause::Unaccounted("ExprNode/ShuffleVectorExpr.b"))
            .unwrap_or(&[]);
        s.count(mask.len());
        for &i in mask {
            s.uv(u64::from(i));
        }
    }
}

/// `ConvertVectorExpr.a`: the operand (the written type is the node's).
pub(crate) fn convert_vector_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let n = node(w, e);
    check_bits(s, &n, 0, Cause::Unaccounted("ExprNode/ConvertVectorExpr.bits"));
    child_word(w, s, n.a, Cause::Unaccounted("ExprNode/ConvertVectorExpr.a"));
}

/// `ExtVectorElementExpr`: the flag word (`bits-arrow`), `a` the vector,
/// `b` the accessor.
pub(crate) fn ext_vector_element_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let n = node(w, e);
    check_bits(
        s,
        &n,
        ExprFlags::ARROW.bits(),
        Cause::Unaccounted("ExprNode/ExtVectorElementExpr.bits"),
    );
    s.flags(FlagWord::new().bit(has(&n, ExprFlags::ARROW)));
    child_word(w, s, n.a, Cause::Unaccounted("ExprNode/ExtVectorElementExpr.a"));
    w.write_sym(s, Symbol::from_index(n.b));
}

/// `GenericSelectionExpr`: `a-control` (`enum GenericControl`, the first
/// two words of the list; `bits-arg-is-type`, `out`, says which),
/// `a-associations` (three words each: `ty?`, its location, `expr`),
/// then `b`: the position of the association selected, absent while
/// dependent (Kiln stores the selected expression, builder.rs:2098).
pub(crate) fn generic_selection_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let n = node(w, e);
    check_bits(
        s,
        &n,
        ExprFlags::ARG_IS_TYPE.bits(),
        Cause::Unaccounted("ExprNode/GenericSelectionExpr.bits"),
    );
    let words =
        list_words(w, s, n.a, Cause::Unaccounted("ExprNode/GenericSelectionExpr.a-control"))
            .unwrap_or(&[]);
    // a-control.
    let control = match words.first() {
        Some(&word) if has(&n, ExprFlags::ARG_IS_TYPE) => QualType::from_raw(word).map(|ty| {
            // The location is `out`: the control's row reads the type.
            GenericControl::Type(ty, Loc::INVALID)
        }),
        Some(&word) => ExprId::try_new(word).map(GenericControl::Expr),
        None => None,
    };
    match control {
        Some(control) => {
            s.tag(generic_control::row(&control));
            match control {
                GenericControl::Expr(operand) => child(w, s, operand),
                GenericControl::Type(ty, _) => w.write_type(s, ty),
            }
        }
        None => {
            s.uv(0).indeterminate(Cause::Unaccounted("ExprNode/GenericSelectionExpr.a-control"));
        }
    }
    // a-associations.
    let rest = words.get(2..).unwrap_or(&[]);
    if rest.len() % 3 != 0 {
        s.indeterminate(Cause::Unaccounted("ExprNode/GenericSelectionExpr.a-associations"));
    }
    let assocs: Vec<(Option<QualType>, u32)> =
        rest.chunks_exact(3).map(|c| (QualType::from_raw(c[0]), c[2])).collect();
    s.count(assocs.len());
    for &(ty, expr) in &assocs {
        w.write_type_opt(s, ty);
        child_word(w, s, expr, Cause::Unaccounted("ExprNode/GenericSelectionExpr.a-associations"));
    }
    // b.
    let selected = ExprId::try_new(n.b);
    let position = selected.and_then(|sel| assocs.iter().position(|&(_, expr)| expr == sel.raw()));
    if selected.is_some() && position.is_none() {
        s.indeterminate(Cause::Unaccounted("ExprNode/GenericSelectionExpr.b"));
    }
    s.opt(position.is_some());
    if let Some(p) = position {
        s.uv(p as u64);
    }
}

// ---------------------------------------------------------------------
// Rewrites.

/// `PseudoObjectExpr`: `a` the syntactic form, then the semantic
/// expressions in evaluation order; `b` which one is the result.
///
/// A semantic expression that is an `OpaqueValueExpr` binds a
/// subexpression of the syntactic form (clang `PseudoObjectExpr`): each
/// such source that is a direct child of the syntactic node takes its
/// opaque number before the syntactic form is written, so the form
/// writes it once and the opaque value writes the number alone (the
/// rule of `BinaryConditionalOperator`, WP-031). The only builder is the
/// synthesized three-way comparison of a defaulted `operator<=>`
/// (`kiln_sema` `class/compare/three_way.rs:76`), whose operands are
/// subobject accesses: pseudo-objects do not nest from source.
pub(crate) fn pseudo_object_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let n = node(w, e);
    check_bits(s, &n, 0, Cause::Unaccounted("ExprNode/PseudoObjectExpr.bits"));
    let es = expr_list(w, s, n.a, Cause::Unaccounted("ExprNode/PseudoObjectExpr.a"));
    if let Some((&syntactic, semantics)) = es.split_first() {
        let ast = w.ast;
        let direct = ast.exprs.children(syntactic, &ast.lists);
        for &x in semantics {
            if let Some(src) = source_of(w, x)
                && direct.contains(&src)
            {
                opaque_source(w, src);
            }
        }
    }
    crate::expr::children(w, s, &es);
    if !es.is_empty() && n.b as usize + 1 >= es.len() {
        s.indeterminate(Cause::Unaccounted("ExprNode/PseudoObjectExpr.b"));
    }
    s.uv(u64::from(n.b));
}
