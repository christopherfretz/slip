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
//! The one Kiln accessor called, `choose_operands`, is called after both
//! words it reads unchecked are checked; a symbol index is checked against
//! the interner's length before it is read.

use slip_kiln::kiln::ast::expr::{
    ArrayTraitExtra, ExprFlags, ExprKind, GenericControl, OffsetofComponent, OffsetofStep,
    PredefinedKind, SourceLocKind, SuspendKind, TraitKind, TypeTrait, TypeidOperand,
};
use slip_kiln::kiln::ast::{DeclId, ExprId, HeapSize, ListId, QualType, RawId, StmtId};
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
    // The names digest (digest §9): the value Kiln evaluated, when it
    // made one (none in a dependent context).
    if let (Some(kind), Some(lit)) = (predefined(n.aux), name_literal(w, &n)) {
        let ast = w.ast;
        let text = ast.exprs.string_lit(lit);
        crate::value::feed_predefined(w, kind, ast.strings.get(text).1);
    }
    crate::local::write_scope_names(w, s);
}

/// `ExprNode` `ty` of a `PredefinedExpr` `n` (digest §9, digest-kinds §7
/// "Values that come from names"): its type is `const char[N]`, `N` the
/// length of the name plus one. As the operand of an array-to-pointer
/// conversion (`decayed`), which passes the value on and nothing of its
/// length, the type is written without the bound
/// ([`Walk::unbounded_array`]); anywhere else (`sizeof`, `decltype`, a
/// reference bound to it, a deduction, a parenthesis the walk keeps) the
/// array's type is what is read, and it is written by value; so it is
/// where [`Walk::by_value`] is raised (a `static_assert` condition, a
/// constant whose value is not stored).
pub(crate) fn name_type(w: &mut Walk<'_>, s: &mut Stream, n: &Node, decayed: bool) {
    let symbolic = if decayed && w.by_value == 0 && name_literal(w, n).is_some() {
        w.unbounded_array(n.ty)
    } else {
        None
    };
    match symbolic {
        Some(t) => w.absorb_type(s, t, n.ty, None),
        None => w.write_type(s, n.ty),
    }
}

/// The `StringLiteral` that holds a `PredefinedExpr`'s value (`a`;
/// absent in a dependent context, `kiln_sema/src/predefined.rs:26-28`).
fn name_literal(w: &Walk<'_>, n: &Node) -> Option<ExprId> {
    let lit = ExprId::try_new(n.a)?;
    (w.ast.exprs.get(lit).kind() == ExprKind::StringLiteral).then_some(lit)
}

/// `SourceLocExpr`: `aux` (`SourceLocKind`); `a`, the context, is
/// `symbolic`, and so are its `SourceLocValues` (§7.4 `source_loc`).
pub(crate) fn source_loc_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let n = node(w, e);
    source_loc_expr_of(s, &n);
    if matches!(source_loc(n.aux), Some(SourceLocKind::Function | SourceLocKind::SourceLocStruct)) {
        feed_source_loc_values(w, e, source_loc(n.aux));
        crate::local::write_scope_names(w, s);
    }
}

/// The names digest (digest §9) of a source-location builtin's node `e`
/// of kind `kind`: the function name Kiln recorded for it, plain for
/// `__builtin_FUNCTION()`, the `__impl` object's for
/// `__builtin_source_location()`. Nothing for the other kinds (the file,
/// the line and the column are not fed) nor where Kiln recorded no value.
fn feed_source_loc_values(w: &mut Walk<'_>, e: ExprId, kind: Option<SourceLocKind>) {
    let ast = w.ast;
    let Some(&v) = ast.exprs.cold.source_loc.get(e) else { return };
    match kind {
        Some(SourceLocKind::Function) => {
            if let Some(l) = crate::value::string_of(w, v.function) {
                crate::value::feed_source_loc(w, false, ast.strings.get(l).1);
            }
        }
        Some(SourceLocKind::SourceLocStruct) => {
            let Some(location) = v.location else { return };
            if let crate::value::ImplObject::Impl(pretty) =
                crate::value::impl_function_name(w, location)
            {
                crate::value::feed_impl(w, pretty.map(|l| ast.strings.get(l).1));
            }
        }
        Some(
            SourceLocKind::File
            | SourceLocKind::FileName
            | SourceLocKind::Line
            | SourceLocKind::Column,
        )
        | None => {}
    }
}

/// [`source_loc_expr`] of the words `n`: those of `e`, or changed by a test.
fn source_loc_expr_of(s: &mut Stream, n: &Node) {
    check_bits(s, n, 0, Cause::Unaccounted("ExprNode/SourceLocExpr.bits"));
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
            let ty = match op {
                TypeidOperand::Type(ty) => ty,
                TypeidOperand::Expr(operand) => node(w, operand).ty,
            };
            crate::local::write_type_names(w, s, ty);
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
    type_trait_expr_of(w, s, node(w, e));
}

/// [`type_trait_expr`] of the words `n`: those of `e`, or changed by a test.
fn type_trait_expr_of(w: &mut Walk<'_>, s: &mut Stream, n: Node) {
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
            OffsetofComponent::Index { index, start: _, end: _ } => {
                index_position(w, s, e, index, Cause::Unaccounted("OffsetofComponent/Index.index"));
            }
        }
    }
}

/// `OffsetofStep::Index.index` and `OffsetofComponent::Index.index`: `uv`
/// of the position of `index` in `OffsetOfExpr.a` of `e`, where it is
/// written (both builders put every index there: `kiln_ast`
/// builder.rs:3538, :3597). An index not in the list is `cause`, and
/// writes `uv` of the list's length.
fn index_position(w: &Walk<'_>, s: &mut Stream, e: ExprId, index: ExprId, cause: Cause) {
    let words: &[u32] = ListId::try_new(node(w, e).a).map_or(&[], |l| w.ast.lists.get(l));
    if let Some(i) = words.iter().position(|&x| x == index.raw()) {
        s.uv(i as u64);
    } else {
        s.indeterminate(cause);
        s.uv(words.len() as u64);
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
                index_position(w, s, e, index, Cause::Unaccounted("OffsetofStep/Index.index"));
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
/// `a` of a resolved expression is elements two to five of the list
/// `[operand, common, ready, suspend, resume, opaque]` (`kiln_sema`
/// coroutine.rs:716, `kiln_ast` builder.rs:3345). The operand is a node
/// of `common`: `common` is the operand itself, its materialisation, or
/// a call with it as argument (`build_resolved_coawait`,
/// `build_unresolved_coawait`, `build_coyield`), so it is not written
/// again. The sixth is the `OpaqueValueExpr` whose source is `common`,
/// and the three calls name it: `common` takes its opaque number before
/// it is written, so the calls' opaque values write the number alone and
/// `common` is written once. A `CoyieldExpr` of a template pattern holds the operand alone
/// (builder.rs:3376), written as a list of one.
fn coawait(w: &mut Walk<'_>, s: &mut Stream, n: Node, yield_: bool) {
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
    check_bits(s, &n, 0, Cause::Unaccounted(bits));
    check_aux(s, &n, 7, Cause::Unaccounted(aux));
    s.flags(FlagWord::new().bit(n.aux & 1 != 0));
    tag_or(s, suspend(n.aux).map(|k| suspend_kind::row(&k)), Cause::Unaccounted(suspend_row));
    let es = expr_list(w, s, n.a, Cause::Unaccounted(list));
    match es.len() {
        6 => {
            // The operand (the first) is a node of `common`, which every
            // builder makes from it: it is not written (§7.2
            // `CoawaitParts.operand`).
            opaque_source(w, es[1]);
            crate::expr::children(w, s, &es[1..5]);
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
    coawait(w, s, node(w, e), false);
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
    coawait(w, s, node(w, e), true);
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
    choose_expr_of(w, s, e, node(w, e));
}

/// [`choose_expr`] of the words `n`: those of `e`, or changed by a test.
fn choose_expr_of(w: &mut Walk<'_>, s: &mut Stream, e: ExprId, n: Node) {
    check_bits(s, &n, 0, Cause::Unaccounted("ExprNode/ChooseExpr.bits"));
    if n.aux > 2 {
        s.indeterminate(Cause::Unaccounted("ExprNode/ChooseExpr.aux"));
    }
    s.uv(u64::from(n.aux));
    // `choose_operands` panics on a condition word of 0 and indexes
    // `cond_arms` by `b` unchecked (`kiln_ast` expr.rs:1859-1863): both are
    // checked first, the length through the arena's memory report, which
    // is the one public reading of it.
    let arms = HeapSize::heap_bytes(&w.ast.exprs).row("cond_arms").map_or(0, |r| r.count);
    if let Some(row) = choose_unreadable(arms, n.a, n.b) {
        s.uv(0).indeterminate(Cause::Unaccounted(row));
        s.uv(0).uv(0);
        return;
    }
    let [cond, first, second] = w.ast.exprs.choose_operands(e);
    child(w, s, cond);
    child(w, s, first);
    child(w, s, second);
}

/// The row of a `ChooseExpr` whose words `choose_operands` cannot read:
/// a condition word of 0, or an index `b` past `cond_arms` (`arms`
/// entries).
fn choose_unreadable(arms: u64, a: u32, b: u32) -> Option<&'static str> {
    if a == 0 {
        Some("ExprNode/ChooseExpr.a")
    } else if u64::from(b) >= arms {
        Some("ExprNode/ChooseExpr.b-first")
    } else {
        None
    }
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
    atomic_expr_of(w, s, node(w, e));
}

/// [`atomic_expr`] of the words `n`: those of `e`, or changed by a test.
fn atomic_expr_of(w: &mut Walk<'_>, s: &mut Stream, n: Node) {
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
    shuffle_vector_expr_of(w, s, node(w, e));
}

/// [`shuffle_vector_expr`] of the words `n`: those of `e`, or changed by a test.
fn shuffle_vector_expr_of(w: &mut Walk<'_>, s: &mut Stream, n: Node) {
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
    convert_vector_expr_of(w, s, node(w, e));
}

/// [`convert_vector_expr`] of the words `n`: those of `e`, or changed by a test.
fn convert_vector_expr_of(w: &mut Walk<'_>, s: &mut Stream, n: Node) {
    check_bits(s, &n, 0, Cause::Unaccounted("ExprNode/ConvertVectorExpr.bits"));
    child_word(w, s, n.a, Cause::Unaccounted("ExprNode/ConvertVectorExpr.a"));
}

/// `ExtVectorElementExpr`: the flag word (`bits-arrow`), `a` the vector,
/// `b` the accessor.
pub(crate) fn ext_vector_element_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    ext_vector_element_expr_of(w, s, node(w, e));
}

/// [`ext_vector_element_expr`] of the words `n`: those of `e`, or changed by a test.
fn ext_vector_element_expr_of(w: &mut Walk<'_>, s: &mut Stream, n: Node) {
    check_bits(
        s,
        &n,
        ExprFlags::ARROW.bits(),
        Cause::Unaccounted("ExprNode/ExtVectorElementExpr.bits"),
    );
    s.flags(FlagWord::new().bit(has(&n, ExprFlags::ARROW)));
    child_word(w, s, n.a, Cause::Unaccounted("ExprNode/ExtVectorElementExpr.a"));
    // `Interner::as_str` panics on an index it never issued (`kiln_source`
    // symbol.rs:242): the index is checked against the interner's length.
    write_issued_sym(w, s, n.b, Cause::Unaccounted("ExprNode/ExtVectorElementExpr.b"));
}

/// `sym` of the symbol with index `index` when the interner issued it
/// (`Interner::as_str` panics on another, `kiln_source` symbol.rs:242);
/// otherwise the empty string and `cause`.
fn write_issued_sym(w: &mut Walk<'_>, s: &mut Stream, index: u32, cause: Cause) {
    if (index as usize) < w.ast.symbols().len() {
        w.write_sym(s, Symbol::from_index(index));
    } else {
        s.bytes(b"").indeterminate(cause);
    }
}

/// `GenericSelectionExpr`: `a-control` (`enum GenericControl`, the first
/// two words of the list; `bits-arg-is-type`, `out`, says which),
/// `a-associations` (three words each: `ty?`, its location, `expr`),
/// then `b`: the position of the association selected, absent while
/// dependent (Kiln stores the selected expression, builder.rs:2098).
pub(crate) fn generic_selection_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    generic_selection_expr_of(w, s, node(w, e));
}

/// [`generic_selection_expr`] of the words `n`: those of `e`, or changed by a test.
fn generic_selection_expr_of(w: &mut Walk<'_>, s: &mut Stream, n: Node) {
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
    pseudo_object_expr_of(w, s, node(w, e));
}

/// [`pseudo_object_expr`] of the words `n`: those of `e`, or changed by a test.
fn pseudo_object_expr_of(w: &mut Walk<'_>, s: &mut Stream, n: Node) {
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

#[cfg(test)]
mod tests {
    //! Row-level tests: each kind's function, on a node Kiln built, against
    //! the rows written by hand from `crate::tables` on a fresh walk of the
    //! same AST. A pair test cannot isolate a row whose value moves with
    //! another (a stored result, a suspend kind); these tests can, and they
    //! cover the kinds whose pair tests wait on the statements' area.

    use slip_kiln::kiln::ast::decl::builtin::atomic;
    use slip_kiln::kiln::ast::expr::ExprKind;
    use slip_kiln::kiln::ast::{DeclId, ExprId, ListId, QualType, StmtId};
    use slip_model::keys::RuleTable;

    use super::*;
    use crate::expr::{KindFn, children};
    use crate::{Cause, Input};

    /// Runs `f` on the AST of `src` (`std` the language mode).
    fn with(src: &str, std: &str, f: impl FnOnce(&Input<'_>, &RuleTable)) {
        let files = [("main.cpp", src)];
        let snippet =
            slip_kiln::snippet::Snippet { files: &files, main: "main.cpp", include: &[""], std };
        slip_kiln::snippet::parse(&snippet, |p| {
            let path = |_| Vec::new();
            let input = Input { ast: p.ast, sm: p.sm, subject_path: &path };
            let rules = RuleTable::empty();
            f(&input, &rules);
        })
        .expect("parses");
    }

    /// The expressions of `kind`, in arena order.
    fn of_kind(input: &Input<'_>, kind: ExprKind) -> Vec<ExprId> {
        (1..=input.ast.exprs.len())
            .filter_map(|i| ExprId::try_new(u32::try_from(i).ok()?))
            .filter(|&e| input.ast.exprs.get(e).kind() == kind)
            .collect()
    }

    /// The bytes and the cause `f` writes for `e` on a fresh walk.
    fn written(
        input: &Input<'_>,
        rules: &RuleTable,
        e: ExprId,
        f: impl FnOnce(&mut Walk<'_>, &mut Stream, ExprId),
    ) -> (Vec<u8>, Option<Cause>) {
        let mut w = Walk::new(input, rules);
        let mut s = Stream::new();
        f(&mut w, &mut s, e);
        (s.as_bytes().to_vec(), s.cause())
    }

    /// `kind_fn` on the `nth` node of `kind` in `src` writes what
    /// `expected` writes.
    fn rows(
        src: &str,
        kind: ExprKind,
        nth: usize,
        kind_fn: KindFn,
        expected: impl FnOnce(&mut Walk<'_>, &mut Stream, ExprId),
    ) {
        rows_std(src, "c++20", kind, nth, kind_fn, expected);
    }

    fn rows_std(
        src: &str,
        std: &str,
        kind: ExprKind,
        nth: usize,
        kind_fn: KindFn,
        expected: impl FnOnce(&mut Walk<'_>, &mut Stream, ExprId),
    ) {
        with(src, std, |input, rules| {
            let all = of_kind(input, kind);
            let e = *all.get(nth).unwrap_or_else(|| panic!("{kind:?} #{nth} in {src}"));
            let got = written(input, rules, e, kind_fn);
            let want = written(input, rules, e, expected);
            assert_eq!(got, want, "{kind:?} #{nth} of {src}");
        });
    }

    fn expr(raw: u32) -> ExprId {
        ExprId::try_new(raw).expect("an expression")
    }

    fn ty(raw: u32) -> QualType {
        QualType::from_raw(raw).expect("a type")
    }

    fn list(w: &Walk<'_>, raw: u32) -> Vec<u32> {
        w.ast.lists.get(ListId::try_new(raw).expect("a list")).to_vec()
    }

    // -----------------------------------------------------------------
    // Names and builtins of a position.

    /// `PredefinedExpr.a`, `SourceLocExpr.a` and the `SourceLocValues`
    /// rows are symbolic: two functions of different names (of one length,
    /// so that `const char[N]` agrees) give the same bytes for the
    /// expression, on different lines too. The pair tests of these rows
    /// only move the expression, so they do not test this alone.
    #[test]
    fn position_names_are_not_written() {
        let src = "const char *fa() { return __func__; }\n\
                   const char *fb() {\n return __func__; }\n\
                   const char *ga() { return __builtin_FUNCTION(); }\n\
                   const char *gb() {\n\n return __builtin_FUNCTION(); }\n\
                   unsigned la() { return __builtin_LINE(); }\n\
                   unsigned lb() {\n return __builtin_LINE(); }\n";
        with(src, "c++20", |input, rules| {
            for kind in [ExprKind::PredefinedExpr, ExprKind::SourceLocExpr] {
                let all = of_kind(input, kind);
                for pair in all.chunks(2) {
                    let [x, y] = pair else { panic!("{kind:?}: two of each") };
                    assert_eq!(written(input, rules, *x, child), written(input, rules, *y, child));
                }
                assert_eq!(all.len(), if kind == ExprKind::PredefinedExpr { 2 } else { 4 });
            }
        });
    }

    #[test]
    fn predefined_aux() {
        let src = "const char *f(const char *s = __FUNCTION__);\n\
                   const char *g(const char *s = __PRETTY_FUNCTION__);\n\
                   const char *h(const char *s = __func__);\n";
        let k = ExprKind::PredefinedExpr;
        rows(src, k, 0, predefined_expr, |_, s, _| {
            s.tag(predefined_kind::FUNCTION).count(0);
        });
        rows(src, k, 1, predefined_expr, |_, s, _| {
            s.tag(predefined_kind::PRETTY_FUNCTION).count(0);
        });
        rows(src, k, 2, predefined_expr, |_, s, _| {
            s.tag(predefined_kind::FUNC).count(0);
        });
    }

    #[test]
    fn source_loc_aux() {
        let src = "unsigned a = __builtin_LINE();\nconst char *b = __builtin_FILE_NAME();\n";
        let k = ExprKind::SourceLocExpr;
        rows(src, k, 0, source_loc_expr, |_, s, _| {
            s.tag(source_loc_kind::LINE);
        });
        rows(src, k, 1, source_loc_expr, |_, s, _| {
            s.tag(source_loc_kind::FILE_NAME);
        });
    }

    // -----------------------------------------------------------------
    // noexcept and typeid.

    #[test]
    fn noexcept_rows() {
        let src = "int a; void g();\nbool x = noexcept(a);\nbool y = noexcept(g());\n";
        for (nth, result) in [(0, true), (1, false)] {
            rows(src, ExprKind::CXXNoexceptExpr, nth, cxx_noexcept_expr, |w, s, e| {
                let n = node(w, e);
                child(w, s, expr(n.a));
                s.bool(result);
            });
        }
    }

    #[test]
    fn typeid_rows() {
        let src = "namespace std { class type_info; }\n\
                   struct P { virtual void h(); }; extern P &p; int a;\n\
                   const std::type_info &x = typeid(int), &y = typeid(p), &z = typeid(a);\n";
        let k = ExprKind::CXXTypeidExpr;
        rows(src, k, 0, cxx_typeid_expr, |w, s, e| {
            let n = node(w, e);
            s.flags(FlagWord::new().bit(false)).tag(typeid_operand::TYPE);
            w.write_type(s, ty(n.a));
            s.count(0);
        });
        for (nth, evaluated) in [(1, true), (2, false)] {
            rows(src, k, nth, cxx_typeid_expr, |w, s, e| {
                let n = node(w, e);
                s.flags(FlagWord::new().bit(evaluated)).tag(typeid_operand::EXPR);
                child(w, s, expr(n.a));
                s.count(0);
            });
        }
    }

    // -----------------------------------------------------------------
    // Traits.

    #[test]
    fn unary_trait_rows() {
        let src = "int a;\nunsigned long x = sizeof(a), y = alignof(int), z = __datasizeof(int),\n\
                   u = __alignof(long);\n";
        let k = ExprKind::UnaryExprOrTypeTraitExpr;
        rows(src, k, 0, unary_expr_or_type_trait_expr, |w, s, e| {
            let n = node(w, e);
            s.flags(FlagWord::new().bit(false)).tag(trait_kind::SIZE_OF);
            child(w, s, expr(n.a));
        });
        for (nth, row) in [
            (1, trait_kind::ALIGN_OF),
            (2, trait_kind::DATA_SIZE_OF),
            (3, trait_kind::PREFERRED_ALIGN_OF),
        ] {
            rows(src, k, nth, unary_expr_or_type_trait_expr, |w, s, e| {
                let n = node(w, e);
                s.flags(FlagWord::new().bit(true)).tag(row);
                w.write_type(s, ty(n.a));
            });
        }
    }

    #[test]
    fn type_trait_rows() {
        let src = "bool x = __is_same(int, long);\nbool y = __is_class(int);\n\
                   bool z = __is_integral(int);\nstruct T { T(int, long); };\n\
                   bool c = __is_constructible(T, int, long);\n";
        let k = ExprKind::TypeTraitExpr;
        for (nth, row, result, len) in [
            (0, type_trait::IS_SAME, 0, 2),
            (1, type_trait::IS_CLASS, 0, 1),
            (2, type_trait::IS_INTEGRAL, 1, 1),
            (3, type_trait::IS_CONSTRUCTIBLE, 1, 3),
        ] {
            rows(src, k, nth, type_trait_expr, |w, s, e| {
                let n = node(w, e);
                s.tag(row);
                let types: Vec<QualType> = list(w, n.a).into_iter().map(ty).collect();
                assert_eq!(types.len(), len);
                if len == 3 {
                    // In the order written, which is not the order of ids.
                    assert!(!types.is_sorted_by_key(|t| t.raw()), "{types:?}");
                    assert!(!types.iter().rev().is_sorted_by_key(|t| t.raw()), "{types:?}");
                }
                w.write_types(s, &types);
                s.uv(result);
            });
        }
    }

    #[test]
    fn array_trait_rows() {
        let src = "unsigned long x = __array_rank(int[1][2]), y = __array_extent(int[3][4], 1);\n";
        let k = ExprKind::ArrayTypeTraitExpr;
        for (nth, row) in [(0, type_trait::ARRAY_RANK), (1, type_trait::ARRAY_EXTENT)] {
            rows(src, k, nth, array_type_trait_expr, |w, s, e| {
                let n = node(w, e);
                s.tag(row);
                w.write_type(s, ty(n.a));
            });
        }
        // The cold row: `dim` absent and the rank; `dim` and the extent.
        rows(src, k, 0, array_trait_extra, |_, s, _| {
            s.opt(false).uv(2);
        });
        rows(src, k, 1, array_trait_extra, |w, s, e| {
            let dim = w.ast.exprs.cold.array_trait.get(e).and_then(|x| x.dim);
            s.opt(true);
            child(w, s, dim.expect("the dimension"));
            s.uv(4);
        });
    }

    #[test]
    fn expression_trait_rows() {
        let src = "int a;\nbool x = __is_lvalue_expr(a), y = __is_rvalue_expr(a);\n";
        let k = ExprKind::ExpressionTraitExpr;
        for (nth, row, result) in
            [(0, type_trait::IS_LVALUE_EXPR, 1), (1, type_trait::IS_RVALUE_EXPR, 0)]
        {
            rows(src, k, nth, expression_trait_expr, |w, s, e| {
                let n = node(w, e);
                s.tag(row);
                child(w, s, expr(n.a));
                s.uv(result);
            });
        }
    }

    // -----------------------------------------------------------------
    // offsetof.

    const OFFSETOF: &str = "struct Q { int o; int p; }; struct S { int n; Q x[4]; }; int a;\n\
        unsigned long x = __builtin_offsetof(S, x[a].p), y = __builtin_offsetof(S, n),\n\
        z = __builtin_offsetof(S, x[1].p);\n\
        template <class T> struct A { unsigned long w = __builtin_offsetof(T, x[a].p); };\n";

    #[test]
    fn offsetof_rows() {
        let k = ExprKind::OffsetOfExpr;
        rows(OFFSETOF, k, 0, offset_of_expr, |w, s, e| {
            let n = node(w, e);
            s.bool(false);
            let indices = list(w, n.a);
            assert_eq!(indices.len(), 1);
            s.count(1);
            child(w, s, expr(indices[0]));
            s.uv(0);
        });
        rows(OFFSETOF, k, 1, offset_of_expr, |_, s, _| {
            s.bool(false).count(0).uv(0);
        });
        // A constant index is an element of `a` too.
        rows(OFFSETOF, k, 2, offset_of_expr, |w, s, e| {
            let n = node(w, e);
            s.bool(false).count(1);
            child(w, s, expr(list(w, n.a)[0]));
            s.uv(16);
        });
    }

    #[test]
    fn offsetof_layout_rows() {
        // `x[a].p`: 4 bytes to `x`, `a` elements of 8 bytes, 4 bytes to `p`.
        rows(OFFSETOF, ExprKind::OffsetOfExpr, 0, offsetof_layout, |w, s, e| {
            let n = node(w, e);
            assert_eq!(list(w, n.a).len(), 1);
            s.count(3).tag(offsetof_step::OFFSET).uv(4).tag(offsetof_step::INDEX);
            s.uv(0).uv(8).tag(offsetof_step::OFFSET).uv(4);
        });
    }

    #[test]
    fn offsetof_components_rows() {
        rows(OFFSETOF, ExprKind::OffsetOfExpr, 3, offsetof_components, |w, s, e| {
            let n = node(w, e);
            assert_eq!(list(w, n.a).len(), 1);
            s.count(3).tag(offsetof_component::FIELD).bytes(b"x").tag(offsetof_component::INDEX);
            s.uv(0).tag(offsetof_component::FIELD).bytes(b"p");
        });
    }

    /// `OffsetOfExpr.aux` set (an invalid class on the path): the bool and
    /// `Cause::Error` (the row is wrong about Kiln, WP-037 landed file).
    #[test]
    fn offsetof_invalid_record_is_an_error() {
        let src = "struct S { int n; Undeclared u; int m; };\n\
                   unsigned long x = __builtin_offsetof(S, m);\n";
        with(src, "c++20", |input, rules| {
            let invalid: Vec<ExprId> = of_kind(input, ExprKind::OffsetOfExpr)
                .into_iter()
                .filter(|&e| node_of(input, e).aux() != 0)
                .collect();
            assert_eq!(invalid.len(), 1, "Kiln marks the invalid record");
            let (_, cause) = written(input, rules, invalid[0], offset_of_expr);
            assert_eq!(cause, Some(Cause::Error));
        });
    }

    /// Each `Index` step and component names its index by position in
    /// `OffsetOfExpr.a`: `z[1][a]` and `z[a][b]` have two `Index` steps
    /// each (a constant index is a step too once the offset is not
    /// constant), the pattern `z[b][a]` two `Index` components.
    #[test]
    fn offsetof_index_positions() {
        let src = "struct S { int n; int z[4][4]; }; int a, b;\n\
            unsigned long u = __builtin_offsetof(S, z[1][a]), v = __builtin_offsetof(S, z[a][b]);\n\
            template <class T> struct A { unsigned long w = __builtin_offsetof(T, z[b][a]); };\n";
        let k = ExprKind::OffsetOfExpr;
        for nth in [0, 1] {
            let want = [0, 1];
            rows(src, k, nth, offsetof_layout, |w, s, e| {
                let steps = w.ast.exprs.cold.offsetof_layout.get(e).expect("a layout").clone();
                let mut want = want.into_iter();
                s.count(steps.len());
                for step in &*steps {
                    s.tag(offsetof_step::row(step));
                    match *step {
                        OffsetofStep::Offset(bytes) => {
                            s.uv(bytes);
                        }
                        OffsetofStep::Index { index: _, elem_size } => {
                            s.uv(want.next().expect("an index")).uv(elem_size);
                        }
                    }
                }
                assert_eq!(want.next(), None, "every index is a step");
            });
        }
        // `OffsetOfExpr.a` in the order written: `z[a][b]`.
        rows(src, k, 1, offset_of_expr, |w, s, e| {
            let es: Vec<ExprId> = list(w, node(w, e).a).into_iter().map(expr).collect();
            assert_eq!(es.len(), 2);
            let mut one = |x| {
                let mut t = Stream::new();
                child(w, &mut t, x);
                t.as_bytes().to_vec()
            };
            assert_ne!(one(es[0]), one(es[1]), "`a` and `b` write differently");
            s.bool(false);
            children(w, s, &es);
            s.uv(0);
        });
        rows(src, k, 2, offsetof_components, |_, s, _| {
            s.count(3).tag(offsetof_component::FIELD).bytes(b"z");
            s.tag(offsetof_component::INDEX).uv(0).tag(offsetof_component::INDEX).uv(1);
        });
    }

    /// An index that is not an element of `OffsetOfExpr.a`: `Unaccounted`,
    /// and `uv` of the list's length.
    #[test]
    fn offsetof_index_not_in_the_list() {
        let src = "struct S { int z[4]; }; int a, b;\n\
            unsigned long u = __builtin_offsetof(S, z[a]);\nint v = b;\n";
        with(src, "c++20", |input, rules| {
            let e = of_kind(input, ExprKind::OffsetOfExpr)[0];
            let other = *of_kind(input, ExprKind::DeclRefExpr).last().expect("`b`");
            let cause = Cause::Unaccounted("OffsetofStep/Index.index");
            let got = written(input, rules, e, |w, s, e| index_position(w, s, e, other, cause));
            let mut want = Stream::new();
            want.uv(1);
            assert_eq!(got, (want.as_bytes().to_vec(), Some(cause)));
        });
    }

    /// Runs `f` on a thread with the walk's stack (deep sources).
    fn on_big_stack(f: impl FnOnce() + Send) {
        std::thread::scope(|scope| {
            std::thread::Builder::new()
                .stack_size(crate::walk::STACK_SIZE)
                .spawn_scoped(scope, f)
                .expect("a thread")
                .join()
                .expect("no panic");
        });
    }

    /// The bytes `child` writes for the largest outermost node of `kind`
    /// in `src` (one no node of `kind` holds as a child or in `a`), and its
    /// cause.
    fn outermost(src: &str, kind: ExprKind) -> (usize, Option<Cause>) {
        let mut out = (0, None);
        on_big_stack(|| {
            with(src, "c++20", |input, rules| {
                let all = of_kind(input, kind);
                let inner: std::collections::HashSet<u32> = all
                    .iter()
                    .flat_map(|&e| {
                        let mut v = input.ast.exprs.children(e, &input.ast.lists);
                        if let Some(l) = ListId::try_new(node_of(input, e).a()) {
                            v.extend(
                                input.ast.lists.get(l).iter().filter_map(|&x| ExprId::try_new(x)),
                            );
                        }
                        v.into_iter().map(RawId::raw)
                    })
                    .collect();
                let tops: Vec<ExprId> =
                    all.into_iter().filter(|e| !inner.contains(&e.raw())).collect();
                out = tops
                    .into_iter()
                    .map(|e| {
                        let (bytes, cause) = written(input, rules, e, child);
                        (bytes.len(), cause)
                    })
                    .max_by_key(|&(n, _)| n)
                    .expect("an outermost node");
            });
        });
        out
    }

    /// `offsetof` nested in a non-constant index, 20 against 200 deep
    /// (Kiln's bracket nesting bounds the depth): the bytes grow by ten.
    #[test]
    fn nested_offsetof_is_linear() {
        let src = |n: usize| {
            let mut e = String::from("a");
            for _ in 0..n {
                e = format!("__builtin_offsetof(S, z[{e}])");
            }
            format!("struct S {{ int z[4]; }}; unsigned long a;\nunsigned long v = {e};\n")
        };
        let (small, c_small) = outermost(&src(20), ExprKind::OffsetOfExpr);
        let (large, c_large) = outermost(&src(200), ExprKind::OffsetOfExpr);
        assert_eq!((c_small, c_large), (None, None));
        assert!(large >= small * 9 && large <= small * 11, "bytes {small} -> {large}");
    }

    fn node_of<'a>(input: &Input<'a>, e: ExprId) -> &'a slip_kiln::kiln::ast::expr::ExprNode {
        input.ast.exprs.get(e)
    }

    // -----------------------------------------------------------------
    // Coroutines.

    const CORO: &str = "namespace std {\n\
        template <class R, class... A> struct coroutine_traits { using promise_type = typename R::promise_type; };\n\
        template <class P = void> struct coroutine_handle {\n\
          static coroutine_handle from_address(void *) noexcept; void *address() const noexcept;\n\
          template <class Q> coroutine_handle(coroutine_handle<Q>) noexcept; coroutine_handle() noexcept; };\n\
        template <> struct coroutine_handle<void> {\n\
          static coroutine_handle from_address(void *) noexcept; void *address() const noexcept;\n\
          coroutine_handle() noexcept; template <class Q> coroutine_handle(coroutine_handle<Q>) noexcept; };\n\
        }\n\
        struct Never { bool await_ready() noexcept; void await_suspend(std::coroutine_handle<>) noexcept; void await_resume() noexcept; };\n\
        struct Maybe { bool await_ready() noexcept; bool await_suspend(std::coroutine_handle<>) noexcept; int await_resume() noexcept; };\n\
        struct Hand { bool await_ready() noexcept; std::coroutine_handle<> await_suspend(std::coroutine_handle<>) noexcept; int await_resume() noexcept; };\n\
        struct Task { struct promise_type {\n\
          Task get_return_object(); Never initial_suspend() noexcept; Never final_suspend() noexcept;\n\
          void return_void(); void unhandled_exception(); Hand yield_value(int); }; };\n\
        Never never; Maybe maybe; Hand hand;\n\
        Task f() { co_await maybe; co_yield 1; }\n\
        template <class T> Task g(T t) { co_await t; co_yield t; }\n";

    /// The `nth` explicit (`aux` bit 0 clear) node of `kind` in [`CORO`].
    fn explicit(input: &Input<'_>, kind: ExprKind, nth: usize) -> usize {
        let all = of_kind(input, kind);
        let picks: Vec<usize> =
            (0..all.len()).filter(|&i| node_of(input, all[i]).aux() & 1 == 0).collect();
        picks[nth]
    }

    /// The rows of a resolved `co_await` / `co_yield`, by hand.
    fn resolved(w: &mut Walk<'_>, s: &mut Stream, e: ExprId, implicit: bool, row: Row) {
        let n = node(w, e);
        let es: Vec<ExprId> = list(w, n.a).into_iter().map(expr).collect();
        assert_eq!(es.len(), 6);
        assert_eq!(source_of(w, es[5]), Some(es[1]), "the opaque value stands for `common`");
        s.flags(FlagWord::new().bit(implicit)).tag(row).count(4);
        let unit = &mut w.unit.expr;
        unit.opaques.insert(es[1], unit.next_opaque);
        unit.next_opaque += 1;
        for &x in &es[1..5] {
            child(w, s, x);
        }
    }

    #[test]
    fn coawait_rows() {
        with(CORO, "c++20", |input, _| {
            let maybe = explicit(input, ExprKind::CoawaitExpr, 0);
            rows(CORO, ExprKind::CoawaitExpr, maybe, coawait_expr, |w, s, e| {
                resolved(w, s, e, false, suspend_kind::BOOL);
            });
            // The initial suspend point: implicit, `Never` (void).
            rows(CORO, ExprKind::CoawaitExpr, 0, coawait_expr, |w, s, e| {
                resolved(w, s, e, true, suspend_kind::VOID);
            });
        });
        // `co_yield 1`: `Hand` (a handle).
        rows(CORO, ExprKind::CoyieldExpr, 0, coyield_expr, |w, s, e| {
            resolved(w, s, e, false, suspend_kind::HANDLE);
        });
        // The template pattern: the operand alone.
        rows(CORO, ExprKind::CoyieldExpr, 1, coyield_expr, |w, s, e| {
            let n = node(w, e);
            s.flags(FlagWord::new().bit(false)).tag(suspend_kind::VOID).count(1);
            child(w, s, expr(list(w, n.a)[0]));
        });
        rows(CORO, ExprKind::DependentCoawaitExpr, 0, dependent_coawait_expr, |w, s, e| {
            let n = node(w, e);
            child(w, s, expr(n.a));
        });
    }

    /// `common` is written once: against the encoding that writes the four
    /// elements without numbering it first, the walk saves exactly the
    /// bytes of `common` (the first call's opaque value writes it there).
    #[test]
    fn coawait_writes_common_once() {
        with(CORO, "c++20", |input, rules| {
            for e in of_kind(input, ExprKind::CoawaitExpr) {
                let (got, _) = written(input, rules, e, coawait_expr);
                // Without back-references (WP-049), which would name the
                // operands of `common` at its second writing.
                let (naive, _) = crate::walk::without_back_refs(|| {
                    written(input, rules, e, |w, s, e| {
                        let n = node(w, e);
                        let es: Vec<ExprId> = list(w, n.a).into_iter().map(expr).collect();
                        s.flags(FlagWord::new().bit(n.aux & 1 != 0));
                        s.tag(suspend(n.aux).map(|k| suspend_kind::row(&k)).expect("a kind"));
                        children(w, s, &es[1..5]);
                    })
                });
                let common = expr(list_of(input, e)[1]);
                let (once, _) = written(input, rules, common, child);
                assert!(naive.len() > got.len());
                assert_eq!(naive.len() - got.len(), once.len(), "common written once");
            }
        });
    }

    fn list_of(input: &Input<'_>, e: ExprId) -> Vec<u32> {
        let raw = node_of(input, e).a();
        input.ast.lists.get(ListId::try_new(raw).expect("a list")).to_vec()
    }

    /// The forms `common` is built in (`kiln_sema` coroutine.rs
    /// `build_resolved_coawait`, `build_unresolved_coawait`,
    /// `build_coyield`, `build_suspend`).
    const AWAIT_FORMS: &str = "namespace std {\n\
        template <class R, class... A> struct coroutine_traits { using promise_type = typename R::promise_type; };\n\
        template <class P = void> struct coroutine_handle {\n\
          static coroutine_handle from_address(void *) noexcept; void *address() const noexcept;\n\
          template <class Q> coroutine_handle(coroutine_handle<Q>) noexcept; coroutine_handle() noexcept; };\n\
        template <> struct coroutine_handle<void> {\n\
          static coroutine_handle from_address(void *) noexcept; void *address() const noexcept;\n\
          coroutine_handle() noexcept; template <class Q> coroutine_handle(coroutine_handle<Q>) noexcept; };\n\
        }\n\
        struct Aw { bool await_ready() noexcept; void await_suspend(std::coroutine_handle<>) noexcept; void await_resume() noexcept; };\n\
        struct Mem { Aw operator co_await() noexcept; };\n\
        struct Free {};\n\
        Aw operator co_await(Free) noexcept;\n\
        struct Pair { int a, b; };\n\
        struct Task { struct promise_type {\n\
          Task get_return_object(); Aw initial_suspend() noexcept; Aw final_suspend() noexcept;\n\
          void return_void(); void unhandled_exception(); Aw yield_value(int); Aw yield_value(Pair); }; };\n\
        struct TaskT { struct promise_type {\n\
          TaskT get_return_object(); Aw initial_suspend() noexcept; Aw final_suspend() noexcept;\n\
          void return_void(); void unhandled_exception(); Aw await_transform(int); }; };\n\
        struct TaskB { struct promise_type {\n\
          TaskB get_return_object(); Mem initial_suspend() noexcept; Free final_suspend() noexcept;\n\
          void return_void(); void unhandled_exception(); Mem await_transform(int); }; };\n\
        Aw aw;\n\
        Task lvalue() { co_await aw; }\n\
        Task prvalue() { co_await Aw{}; }\n\
        Task member() { co_await Mem{}; }\n\
        Task free() { co_await Free{}; }\n\
        TaskT transform() { co_await 1; }\n\
        TaskB both() { co_await 2; }\n\
        Task yield() { co_yield 3; }\n\
        Task braced() { co_yield {1, 2}; }\n\
        template <class T> Task tbraced(T) { co_yield {1, T()}; }\n\
        template Task tbraced<int>(int);\n";

    /// The expressions reachable from `e` in Kiln's AST: its children
    /// and the sources of the opaque values among them.
    fn reachable(input: &Input<'_>, e: ExprId) -> std::collections::HashSet<u32> {
        let mut seen = std::collections::HashSet::new();
        let mut stack = vec![e];
        while let Some(x) = stack.pop() {
            if !seen.insert(x.raw()) {
                continue;
            }
            let n = node_of(input, x);
            if n.kind() == ExprKind::OpaqueValueExpr
                && let Some(src) = ExprId::try_new(n.a())
            {
                stack.push(src);
            }
            input.ast.exprs.children_into(x, &input.ast.lists, &mut stack);
        }
        seen
    }

    /// The fact the encoding rests on (§7.2 `CoawaitParts.operand`): in
    /// every form, the operand is a node of `common`. Seven functions,
    /// each with its explicit suspension point and the implicit initial
    /// and final ones (`TaskB`'s through a member and a non-member
    /// `operator co_await`).
    #[test]
    fn coawait_operand_is_in_common() {
        with(AWAIT_FORMS, "c++20", |input, _| {
            let (mut forms, mut braced) = (0, 0);
            for kind in [ExprKind::CoawaitExpr, ExprKind::CoyieldExpr] {
                for e in of_kind(input, kind) {
                    let es = list_of(input, e);
                    if es.len() == 1 {
                        continue; // `tbraced`'s pattern: the operand alone.
                    }
                    assert_eq!(es.len(), 6, "resolved");
                    let (operand, common) = (expr(es[0]), expr(es[1]));
                    let inside = reachable(input, common);
                    if node_of(input, operand).kind() == ExprKind::InitListExpr {
                        // `co_yield {…}`: the operand is the void placeholder
                        // of `act_on_braced_arg` (`kiln_sema` forms.rs:32),
                        // not a node of `common`; each of its elements is.
                        assert!(!inside.contains(&operand.raw()));
                        let elements = input.ast.exprs.children(operand, &input.ast.lists);
                        assert_eq!(elements.len(), 2);
                        for x in elements {
                            assert!(inside.contains(&x.raw()), "{e:?}: element {x:?} in `common`");
                        }
                        braced += 1;
                    } else {
                        assert!(
                            inside.contains(&operand.raw()),
                            "{kind:?} {e:?}: the operand is a node of `common`"
                        );
                    }
                    forms += 1;
                }
            }
            // Nine functions (`tbraced` by its instantiation), each with its
            // explicit point and the implicit initial and final ones.
            assert_eq!(forms, 27, "nine explicit and eighteen implicit points");
            assert_eq!(braced, 2, "`braced` and the instantiation of `tbraced`");
        });
    }

    /// A change in the operand alone changes what `co_await` writes,
    /// through `common`.
    #[test]
    fn coawait_operand_change_differs() {
        let src =
            |x: &str| format!("{AWAIT_FORMS}Aw &id(int);\nTask g() {{ co_await id({x}); }}\n");
        let bytes = |src: &str| {
            let mut out = Vec::new();
            with(src, "c++20", |input, rules| {
                let e = *of_kind(input, ExprKind::CoawaitExpr)
                    .iter()
                    .rfind(|&&e| node_of(input, e).aux() & 1 == 0)
                    .expect("`co_await id(…)`");
                out = written(input, rules, e, coawait_expr).0;
            });
            out
        };
        assert_ne!(bytes(&src("1")), bytes(&src("2")));
    }

    /// `co_await co_await … aw`, 200 against 2,000 deep: the bytes grow by
    /// ten (writing the operand besides `common` doubles them per level).
    #[test]
    fn nested_coawait_is_linear() {
        let src = |n: usize| {
            format!(
                "{AWAIT_FORMS}struct Re {{ bool await_ready() noexcept; \
                 void await_suspend(std::coroutine_handle<>) noexcept; Re &await_resume() noexcept; }};\n\
                 Re re;\nTask g() {{ {}re; }}\n",
                "co_await ".repeat(n)
            )
        };
        let (small, c_small) = outermost(&src(200), ExprKind::CoawaitExpr);
        let (large, c_large) = outermost(&src(2_000), ExprKind::CoawaitExpr);
        // Kinds of `expr_init.rs` / `expr_dep.rs` (other packages) are
        // still placeholders under the calls.
        let pending = Some(Cause::Pending(crate::Area::Expr));
        assert!(c_small.is_none() || c_small == pending, "{c_small:?}");
        assert!(c_large.is_none() || c_large == pending, "{c_large:?}");
        assert!(large >= small * 9 && large <= small * 11, "bytes {small} -> {large}");
    }

    // -----------------------------------------------------------------
    // GNU and C extensions.

    #[test]
    fn stmt_expr_rows() {
        rows("int f() { return ({ 1; }); }\n", ExprKind::StmtExpr, 0, stmt_expr, |w, s, e| {
            let n = node(w, e);
            crate::stmt::child(w, s, StmtId::try_new(n.a).expect("a statement"));
        });
    }

    #[test]
    fn addr_label_rows() {
        let src = "void f() { l: m: void *p = &&l, *q = &&m; }\n";
        for nth in [0, 1] {
            rows(src, ExprKind::AddrLabelExpr, nth, addr_label_expr, |w, s, e| {
                let n = node(w, e);
                w.write_ref(s, DeclId::try_new(n.a).expect("a label"));
            });
        }
    }

    #[test]
    fn choose_rows() {
        let src = "int a, b;\nint x = __builtin_choose_expr(1, a, b), y = __builtin_choose_expr(0, a, b);\n";
        for (nth, chosen) in [(0, 1), (1, 2)] {
            rows(src, ExprKind::ChooseExpr, nth, choose_expr, |w, s, e| {
                let n = node(w, e);
                s.uv(chosen);
                child(w, s, expr(n.a));
                let [first, second] = w.ast.exprs.choose_operands(e)[1..] else { unreachable!() };
                child(w, s, first);
                child(w, s, second);
            });
        }
    }

    #[test]
    fn va_arg_rows() {
        let src = "__builtin_va_list ap;\nlong v = __builtin_va_arg(ap, long);\n";
        rows(src, ExprKind::VAArgExpr, 0, va_arg_expr, |w, s, e| {
            let n = node(w, e);
            child(w, s, expr(n.a));
            w.write_type(s, ty(n.b));
        });
    }

    /// The numbers [`ATOMIC_OPS`] writes are Kiln's, and they are exactly
    /// the operations `is_op` names.
    #[test]
    fn atomic_numbering_is_kilns() {
        let kiln = [
            atomic::ADD_FETCH,
            atomic::AND_FETCH,
            atomic::CMPXCHG,
            atomic::CMPXCHG_N,
            atomic::EXCHANGE,
            atomic::EXCHANGE_N,
            atomic::FETCH_ADD,
            atomic::FETCH_AND,
            atomic::FETCH_MAX,
            atomic::FETCH_MIN,
            atomic::FETCH_NAND,
            atomic::FETCH_OR,
            atomic::FETCH_SUB,
            atomic::FETCH_XOR,
            atomic::LOAD,
            atomic::LOAD_N,
            atomic::MAX_FETCH,
            atomic::MIN_FETCH,
            atomic::NAND_FETCH,
            atomic::OR_FETCH,
            atomic::STORE,
            atomic::STORE_N,
            atomic::SUB_FETCH,
            atomic::XOR_FETCH,
            atomic::C11_CMPXCHG_STRONG,
            atomic::C11_CMPXCHG_WEAK,
            atomic::C11_EXCHANGE,
            atomic::C11_FETCH_ADD,
            atomic::C11_FETCH_AND,
            atomic::C11_FETCH_MAX,
            atomic::C11_FETCH_MIN,
            atomic::C11_FETCH_NAND,
            atomic::C11_FETCH_OR,
            atomic::C11_FETCH_SUB,
            atomic::C11_FETCH_XOR,
            atomic::C11_INIT,
            atomic::C11_LOAD,
            atomic::C11_STORE,
        ];
        for (&(name, ours), theirs) in ATOMIC_OPS.iter().zip(kiln) {
            assert_eq!(ours, theirs, "{name}");
        }
        for id in 0..=255 {
            assert_eq!(atomic::is_op(id), ATOMIC_OPS.iter().any(|&(_, v)| v == id), "{id}");
        }
    }

    #[test]
    fn atomic_rows() {
        let src = "int a;\nint x = __atomic_load_n(&a, 0), y = __atomic_fetch_add(&a, 1, 0);\n";
        for (nth, op) in [(0, atomic::LOAD_N), (1, atomic::FETCH_ADD)] {
            rows(src, ExprKind::AtomicExpr, nth, atomic_expr, |w, s, e| {
                let n = node(w, e);
                s.uv(u64::from(op));
                let es: Vec<ExprId> = list(w, n.a).into_iter().map(expr).collect();
                children(w, s, &es);
            });
        }
    }

    const VEC: &str = "typedef int v4 __attribute__((ext_vector_type(4)));\n\
        typedef float f4 __attribute__((ext_vector_type(4)));\nv4 vv; v4 *pv;\n";

    #[test]
    fn shuffle_rows() {
        let src = format!(
            "{VEC}v4 x = __builtin_shufflevector(vv, vv, 3, 2, 1, 0);\n\
             template <int N> struct B {{ v4 w = __builtin_shufflevector(vv, vv, N, 1, 2, 3); }};\n"
        );
        rows(&src, ExprKind::ShuffleVectorExpr, 0, shuffle_vector_expr, |w, s, e| {
            let n = node(w, e);
            s.flags(FlagWord::new().bit(true));
            let es: Vec<ExprId> = list(w, n.a).into_iter().map(expr).collect();
            children(w, s, &es);
            s.count(4).uv(3).uv(2).uv(1).uv(0);
        });
        rows(&src, ExprKind::ShuffleVectorExpr, 1, shuffle_vector_expr, |w, s, e| {
            let n = node(w, e);
            s.flags(FlagWord::new().bit(false));
            let es: Vec<ExprId> = list(w, n.a).into_iter().map(expr).collect();
            children(w, s, &es);
        });
    }

    #[test]
    fn vector_rows() {
        let src =
            format!("{VEC}f4 c = __builtin_convertvector(vv, f4);\nint x = vv.y, y = pv->z;\n");
        rows(&src, ExprKind::ConvertVectorExpr, 0, convert_vector_expr, |w, s, e| {
            let n = node(w, e);
            child(w, s, expr(n.a));
        });
        for (nth, arrow, name) in [(0, false, b"y"), (1, true, b"z")] {
            rows(&src, ExprKind::ExtVectorElementExpr, nth, ext_vector_element_expr, |w, s, e| {
                let n = node(w, e);
                s.flags(FlagWord::new().bit(arrow));
                child(w, s, expr(n.a));
                s.bytes(name);
            });
        }
    }

    #[test]
    fn generic_rows() {
        let src = "int a;\nint x = _Generic(a, int: 1, default: 2);\n\
                   int y = _Generic(long, int: 1, long: 2);\n\
                   template <class T> struct G { T t; int w = _Generic(t, int: 1, default: 2); };\n";
        let k = ExprKind::GenericSelectionExpr;
        rows(src, k, 0, generic_selection_expr, |w, s, e| {
            let words = list(w, node(w, e).a);
            s.tag(generic_control::EXPR);
            child(w, s, expr(words[0]));
            s.count(2);
            w.write_type_opt(s, QualType::from_raw(words[2]));
            child(w, s, expr(words[4]));
            w.write_type_opt(s, None);
            child(w, s, expr(words[7]));
            s.opt(true).uv(0);
        });
        rows(src, k, 1, generic_selection_expr, |w, s, e| {
            let words = list(w, node(w, e).a);
            s.tag(generic_control::TYPE);
            w.write_type(s, ty(words[0]));
            s.count(2);
            for i in [2, 5] {
                w.write_type_opt(s, QualType::from_raw(words[i]));
                child(w, s, expr(words[i + 2]));
            }
            s.opt(true).uv(1);
        });
        // Dependent: no association selected.
        rows(src, k, 2, generic_selection_expr, |w, s, e| {
            let words = list(w, node(w, e).a);
            s.tag(generic_control::EXPR);
            child(w, s, expr(words[0]));
            s.count(2);
            w.write_type_opt(s, QualType::from_raw(words[2]));
            child(w, s, expr(words[4]));
            w.write_type_opt(s, None);
            child(w, s, expr(words[7]));
            s.opt(false);
        });
    }

    // -----------------------------------------------------------------
    // The pseudo-object rewrite.

    const CMP: &str = "namespace std { struct strong_ordering { int v;\n\
        static const strong_ordering less, equal, greater;\n\
        friend constexpr bool operator==(strong_ordering, int) { return true; }\n\
        friend constexpr bool operator!=(strong_ordering, int) { return true; } };\n\
        inline constexpr strong_ordering strong_ordering::less{-1}, strong_ordering::equal{0},\n\
        strong_ordering::greater{1}; }\n\
        struct M { bool operator==(const M &) const; bool operator<(const M &) const; };\n\
        struct X { M m; std::strong_ordering operator<=>(const X &) const = default; };\n\
        auto r = X{} <=> X{};\n";

    /// The sources the syntactic form holds of the semantic opaque values.
    fn sources(w: &Walk<'_>, es: &[ExprId]) -> Vec<ExprId> {
        let direct = w.ast.exprs.children(es[0], &w.ast.lists);
        es[1..].iter().filter_map(|&x| source_of(w, x)).filter(|x| direct.contains(x)).collect()
    }

    #[test]
    fn pseudo_object_rows() {
        with(CMP, "c++20", |input, _| {
            assert!(!of_kind(input, ExprKind::PseudoObjectExpr).is_empty(), "Kiln builds one");
        });
        rows(CMP, ExprKind::PseudoObjectExpr, 0, pseudo_object_expr, |w, s, e| {
            let n = node(w, e);
            let es: Vec<ExprId> = list(w, n.a).into_iter().map(expr).collect();
            for src in sources(w, &es) {
                let unit = &mut w.unit.expr;
                unit.opaques.insert(src, unit.next_opaque);
                unit.next_opaque += 1;
            }
            children(w, s, &es);
            s.uv(u64::from(n.b));
        });
    }

    /// Each operand of the comparison is written once: against the
    /// encoding that writes the list without numbering the sources first,
    /// the walk saves exactly the bytes of the two operands.
    #[test]
    fn pseudo_object_writes_operands_once() {
        with(CMP, "c++20", |input, rules| {
            let all = of_kind(input, ExprKind::PseudoObjectExpr);
            assert!(!all.is_empty());
            for e in all {
                let (got, _) = written(input, rules, e, pseudo_object_expr);
                // Without back-references (WP-049), which would name the
                // operands' own operands at their second writing.
                let (naive, _) = crate::walk::without_back_refs(|| {
                    written(input, rules, e, |w, s, e| {
                        let n = node(w, e);
                        let es: Vec<ExprId> = list(w, n.a).into_iter().map(expr).collect();
                        children(w, s, &es);
                        s.uv(u64::from(n.b));
                    })
                });
                let es: Vec<ExprId> = list_of(input, e).into_iter().map(expr).collect();
                let w = Walk::new(input, rules);
                let srcs = sources(&w, &es);
                assert_eq!(srcs.len(), 2, "both operands are bound");
                let once: usize =
                    srcs.iter().map(|&x| written(input, rules, x, child).0.len()).sum();
                assert_eq!(naive.len() - got.len(), once, "the operands written once");
            }
        });
    }

    // -----------------------------------------------------------------
    // Words Kiln does not build: well formed, `Unaccounted`, no panic.

    /// The cause `f` gives `e`'s words changed by `change` (the guards
    /// against words Kiln never writes), after checking that the words as
    /// Kiln wrote them give none of their own.
    fn crafted(
        input: &Input<'_>,
        rules: &RuleTable,
        e: ExprId,
        change: impl Fn(&Walk<'_>, Node) -> Node,
        f: impl Fn(&mut Walk<'_>, &mut Stream, Node),
    ) -> Option<Cause> {
        let (_, clean) = written(input, rules, e, |w, s, e| f(w, s, node(w, e)));
        // Pending: kinds of other packages under the node, met after the
        // guards (the first cause is kept).
        assert!(
            matches!(clean, None | Some(Cause::Pending(_))),
            "{e:?} {:?} as built: {clean:?}",
            node_of(input, e).kind()
        );
        written(input, rules, e, |w, s, e| {
            let n = change(w, node(w, e));
            f(w, s, n);
        })
        .1
    }

    /// Every guard against a word Kiln never writes: a crafted word makes
    /// the unit `Unaccounted` with the guard's row (kills the guard
    /// removals of the WP-037 review: M22, M46, M51, M57, M61, M68, M72,
    /// M81, M82, M85, M86, M87, M88).
    #[test]
    fn crafted_words_are_unaccounted() {
        let un = |r: &'static str| Some(Cause::Unaccounted(r));
        let src = format!(
            "{VEC}v4 sh = __builtin_shufflevector(vv, vv, 0, 1, 2, 3);\n\
             f4 cv = __builtin_convertvector(vv, f4);\nint ex = vv.y;\nint a;\n\
             int ch = __builtin_choose_expr(1, a, a);\nint at = __atomic_load_n(&a, 0);\n\
             int g = _Generic(a, int: 1, default: 2);\nbool tt = __is_same(int, long);\n\
             unsigned l = __builtin_LINE();\n"
        );
        with(&src, "c++20", |input, rules| {
            let one = |k| of_kind(input, k)[0];
            let sh = one(ExprKind::ShuffleVectorExpr);
            let mask = node_of(input, sh).b();
            let operands = node_of(input, sh).a();
            assert_eq!(list_of(input, sh).len(), 6, "two vectors and four indices");
            // `TypeTraitExpr.a` naming the mask list `[0, 1, 2, 3]`: a 0 word.
            let c = crafted(
                input,
                rules,
                one(ExprKind::TypeTraitExpr),
                |_, n| Node { a: mask, ..n },
                type_trait_expr_of,
            );
            assert_eq!(c, un("ExprNode/TypeTraitExpr.a"));
            let c = crafted(
                input,
                rules,
                one(ExprKind::ChooseExpr),
                |_, n| Node { aux: 3, ..n },
                |w, s, n| choose_expr_of(w, s, one(ExprKind::ChooseExpr), n),
            );
            assert_eq!(c, un("ExprNode/ChooseExpr.aux"));
            let c = crafted(
                input,
                rules,
                one(ExprKind::AtomicExpr),
                |_, n| Node { aux: 99, ..n },
                atomic_expr_of,
            );
            assert_eq!(c, un("ExprNode/AtomicExpr.aux"));
            let c = crafted(input, rules, sh, |_, n| Node { aux: 3, ..n }, shuffle_vector_expr_of);
            assert_eq!(c, un("ExprNode/ShuffleVectorExpr.aux"));
            let c = crafted(input, rules, sh, |_, n| Node { b: 0, ..n }, shuffle_vector_expr_of);
            assert_eq!(c, un("ExprNode/ShuffleVectorExpr.b"), "a list word of 0");
            let c = crafted(
                input,
                rules,
                one(ExprKind::ConvertVectorExpr),
                |_, n| Node { a: 0, ..n },
                convert_vector_expr_of,
            );
            assert_eq!(c, un("ExprNode/ConvertVectorExpr.a"), "a child word of 0");
            let c = crafted(
                input,
                rules,
                one(ExprKind::ExtVectorElementExpr),
                |_, n| Node { flags: n.flags | 0x20, ..n },
                ext_vector_element_expr_of,
            );
            assert_eq!(c, un("ExprNode/ExtVectorElementExpr.bits"));
            let gs = one(ExprKind::GenericSelectionExpr);
            let c = crafted(
                input,
                rules,
                gs,
                |_, n| Node { b: gs.raw(), ..n },
                generic_selection_expr_of,
            );
            assert_eq!(c, un("ExprNode/GenericSelectionExpr.b"), "no association's expression");
            // The shuffle's operand list, six expressions: four words after
            // the control and its location are not triples.
            let c = crafted(
                input,
                rules,
                gs,
                |_, n| Node { a: operands, ..n },
                generic_selection_expr_of,
            );
            assert_eq!(c, un("ExprNode/GenericSelectionExpr.a-associations"));
            let c = crafted(
                input,
                rules,
                one(ExprKind::SourceLocExpr),
                |_, n| Node { flags: 0x10, ..n },
                |_, s, n| source_loc_expr_of(s, &n),
            );
            assert_eq!(c, un("ExprNode/SourceLocExpr.bits"));
        });
    }

    /// [`crafted_words_are_unaccounted`], for the coroutines and the
    /// pseudo-object rewrite.
    #[test]
    fn crafted_words_are_unaccounted_2() {
        let un = |r: &'static str| Some(Cause::Unaccounted(r));
        with(CORO, "c++20", |input, rules| {
            let aw = of_kind(input, ExprKind::CoawaitExpr)[0];
            let c = crafted(
                input,
                rules,
                aw,
                |_, n| Node { aux: n.aux | 8, ..n },
                |w, s, n| coawait(w, s, n, false),
            );
            assert_eq!(c, un("ExprNode/CoawaitExpr.aux"));
            // The pattern's list of one, for a `co_await`: a wrong length.
            let pattern = node_of(input, of_kind(input, ExprKind::CoyieldExpr)[1]).a();
            let c = crafted(
                input,
                rules,
                aw,
                |_, n| Node { a: pattern, ..n },
                |w, s, n| coawait(w, s, n, false),
            );
            assert_eq!(c, un("ExprNode/CoawaitExpr.a"));
        });
        with(CMP, "c++20", |input, rules| {
            let po = of_kind(input, ExprKind::PseudoObjectExpr)[0];
            let c = crafted(input, rules, po, |_, n| Node { b: 99, ..n }, pseudo_object_expr_of);
            assert_eq!(c, un("ExprNode/PseudoObjectExpr.b"));
        });
    }

    #[test]
    fn unknown_values_are_unaccounted() {
        assert!(predefined(3).is_none() && source_loc(6).is_none() && suspend(6).is_none());
        let mut s = Stream::new();
        tag_or(&mut s, None, Cause::Unaccounted("ExprNode/x"));
        assert_eq!(s.cause(), Some(Cause::Unaccounted("ExprNode/x")));
        let mut s = Stream::new();
        bool_word(&mut s, 2, Cause::Unaccounted("ExprNode/y"));
        assert_eq!(s.cause(), Some(Cause::Unaccounted("ExprNode/y")));
        // `ChooseExpr`: a condition word of 0, an arm index past the
        // table; a readable node.
        assert_eq!(choose_unreadable(4, 0, 1), Some("ExprNode/ChooseExpr.a"));
        assert_eq!(choose_unreadable(4, 7, 4), Some("ExprNode/ChooseExpr.b-first"));
        assert_eq!(choose_unreadable(4, 7, 3), None);
        // An accessor index the interner never issued: no panic, the
        // empty string and the cause; an issued one is written.
        with("int a;\n", "c++20", |input, rules| {
            let cause = Cause::Unaccounted("ExprNode/ExtVectorElementExpr.b");
            let never = u32::try_from(input.ast.symbols().len()).expect("fits");
            let got = written(input, rules, expr(1), |w, s, _| {
                write_issued_sym(w, s, never, cause);
            });
            let mut want = Stream::new();
            want.bytes(b"");
            assert_eq!(got, (want.as_bytes().to_vec(), Some(cause)));
            let a = input.ast.symbols().get("a").expect("interned");
            let got = written(input, rules, expr(1), |w, s, _| {
                write_issued_sym(w, s, a.index(), cause);
            });
            let mut want = Stream::new();
            want.bytes(b"a");
            assert_eq!(got, (want.as_bytes().to_vec(), None));
        });
    }
}
