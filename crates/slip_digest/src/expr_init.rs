//! Construction, initialisation, `new` and `delete`
//! (`spec/digest-kinds.md` §7, WP-035): one function per kind.
//!
//! The rows of a kind's function (`expr::KindFn`): its kind's rows only,
//! in table order; `expr::child` writes the common rows before the call
//! and the `ColdExprData` rows after it (`lib.rs` "How to fill an
//! expression kind").
//!
//! Each kind is two steps: a `*_rows` function reads the node and lists
//! its rows as [`R`]s (a cause it finds goes on the stream at once), and
//! [`emit`] writes them, walking each child through `expr::child`. The
//! tests at the end of this file read the lists, so every flag, tag and
//! optional of a kind is checked against the row the table names.
//!
//! Decisions and their Kiln sites: `spec/landed/digest-kinds/WP-035.md`.

use slip_kiln::kiln::ast::expr::{
    BinaryOp, ConstructInfo, ConstructionKind, Designator, ExprFlags, InitListExtra, NewExtra,
    NewInitStyle,
};
use slip_kiln::kiln::ast::{DeclId, ExprId, ListId, QualType, RawId};
use slip_kiln::kiln::source::symbol::Symbol;

use crate::Cause;
use crate::expr::{Node, check_aux, check_bits, child, expr_list, node};
use crate::stream::{FlagWord, Stream};
use crate::tables::{Row, binary_op, construction_kind, designator, new_init_style};
use crate::walk::Walk;

/// One primitive of a kind's rows, written by [`emit`].
#[derive(Clone, Copy, Debug)]
enum R {
    /// `child`: the expression, walked here.
    Child(ExprId),
    Uv(u64),
    Tag(Row),
    Flags(FlagWord),
    Count(usize),
    Opt(bool),
    Bool(bool),
    Ref(DeclId),
    RefOpt(Option<DeclId>),
    Type(QualType),
    Sym(Symbol),
}

/// Writes `rows` in order.
fn emit(w: &mut Walk<'_>, s: &mut Stream, rows: &[R]) {
    for &r in rows {
        match r {
            R::Child(e) => child(w, s, e),
            R::Uv(v) => {
                s.uv(v);
            }
            R::Tag(row) => {
                s.tag(row);
            }
            R::Flags(f) => {
                s.flags(f);
            }
            R::Count(n) => {
                s.count(n);
            }
            R::Opt(p) => {
                s.opt(p);
            }
            R::Bool(b) => {
                s.bool(b);
            }
            R::Ref(d) => w.write_ref(s, d),
            R::RefOpt(d) => w.write_ref_opt(s, d),
            R::Type(q) => w.write_type(s, q),
            R::Sym(sym) => w.write_sym(s, sym),
        }
    }
}

/// Reads `e`'s node, lists its rows with `rows` and writes them.
fn run(
    w: &mut Walk<'_>,
    s: &mut Stream,
    e: ExprId,
    rows: fn(&Walk<'_>, &mut Stream, ExprId, &Node, &mut Vec<R>),
) {
    let n = node(w, e);
    let mut out = Vec::new();
    rows(w, s, e, &n, &mut out);
    emit(w, s, &out);
}

/// The bit of a per-kind flag, in place (bits 4 to 7).
const fn bit(f: ExprFlags) -> u8 {
    f.bits()
}

/// `true` when the per-kind flag `f` is set.
const fn has(n: &Node, f: ExprFlags) -> bool {
    n.flags & f.bits() != 0
}

/// `child` of a word: the expression, or a placeholder and `cause` when
/// the word is 0.
fn child_word(s: &mut Stream, out: &mut Vec<R>, raw: u32, cause: Cause) {
    if let Some(e) = ExprId::try_new(raw) {
        out.push(R::Child(e));
    } else {
        out.push(R::Uv(0));
        s.indeterminate(cause);
    }
}

/// `child?` of a word: absent when it is 0.
fn child_opt_word(out: &mut Vec<R>, raw: u32) {
    child_opt(out, ExprId::try_new(raw));
}

/// `child?`.
fn child_opt(out: &mut Vec<R>, e: Option<ExprId>) {
    out.push(R::Opt(e.is_some()));
    if let Some(e) = e {
        out.push(R::Child(e));
    }
}

/// `child*` of the `ListId` in a word.
fn child_list(w: &Walk<'_>, s: &mut Stream, out: &mut Vec<R>, raw: u32, cause: Cause) {
    let es = expr_list(w, s, raw, cause);
    out.push(R::Count(es.len()));
    out.extend(es.into_iter().map(R::Child));
}

/// `InitListExpr.a`: the list as runs of one node, the count of runs,
/// then for each its length (`uv`) and the node once. A GNU range
/// designator's initialiser is one node in every slot it fills (`kiln_sema`
/// `init.rs:3109`, the loop over the range's slots), so `[0 ... N] = x`
/// writes `x` once whatever `N` is.
fn child_runs(w: &Walk<'_>, s: &mut Stream, out: &mut Vec<R>, raw: u32, cause: Cause) {
    let es = expr_list(w, s, raw, cause);
    let at = out.len();
    out.push(R::Count(0));
    let mut runs = 0;
    let mut i = 0;
    while let Some(&e) = es.get(i) {
        let len = es[i..].iter().take_while(|&&x| x == e).count();
        out.push(R::Uv(len as u64));
        out.push(R::Child(e));
        runs += 1;
        i += len;
    }
    out[at] = R::Count(runs);
}

/// `child*` of a list.
fn child_list_id(w: &Walk<'_>, s: &mut Stream, out: &mut Vec<R>, l: ListId, cause: Cause) {
    child_list(w, s, out, l.raw(), cause);
}

/// `ref` of a word that must name an entity: `Ref::none` and `cause`
/// when it is 0.
fn entity(s: &mut Stream, out: &mut Vec<R>, raw: u32, cause: Cause) {
    if let Some(d) = DeclId::try_new(raw) {
        out.push(R::Ref(d));
    } else {
        out.push(R::RefOpt(None));
        s.indeterminate(cause);
    }
}

/// `enum ConstructionKind` of two bits.
fn construction_kind_row(s: &mut Stream, out: &mut Vec<R>, bits: u8, cause: Cause) {
    if let Some(k) = ConstructionKind::from_u8(bits & 3) {
        out.push(R::Tag(construction_kind::row(&k)));
    } else {
        out.push(R::Uv(0));
        s.indeterminate(cause);
    }
}

// ---------------------------------------------------------------------
// Lists and aggregates.

/// `InitListExpr`: `a` (`child*`), then `b`, the `InitListExtra`
/// (`array_filler`, `union_field`; `rbrace` is `out`).
pub(crate) fn init_list_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    run(w, s, e, init_list_rows);
}

fn init_list_rows(w: &Walk<'_>, s: &mut Stream, e: ExprId, n: &Node, out: &mut Vec<R>) {
    // `bits-designated` is `out` (Kiln WP-2350).
    check_bits(s, n, bit(ExprFlags::DESIGNATED), Cause::Unaccounted("ExprNode/InitListExpr.bits"));
    check_aux(s, n, 0, Cause::Unaccounted("ExprNode/InitListExpr.aux"));
    child_runs(w, s, out, n.a, Cause::Unaccounted("ExprNode/InitListExpr.a"));
    let &InitListExtra { array_filler, union_field, rbrace: _ } = w.ast.exprs.init_list(e);
    child_opt(out, array_filler);
    out.push(R::RefOpt(union_field));
}

/// `ImplicitValueInitExpr`: the common rows alone.
pub(crate) fn implicit_value_init_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    run(w, s, e, implicit_value_init_rows);
}

fn implicit_value_init_rows(_: &Walk<'_>, s: &mut Stream, _: ExprId, n: &Node, _: &mut Vec<R>) {
    no_rows(s, n, "ExprNode/ImplicitValueInitExpr.bits", "ExprNode/ImplicitValueInitExpr.aux");
}

/// A kind with no rows of its own: any bit or `aux` is unaccounted, and
/// so is a word (`a`, `b`) that is not 0.
fn no_rows(s: &mut Stream, n: &Node, bits: &'static str, aux: &'static str) {
    check_bits(s, n, 0, Cause::Unaccounted(bits));
    check_aux(s, n, 0, Cause::Unaccounted(aux));
    if n.a != 0 || n.b != 0 {
        s.indeterminate(Cause::Unaccounted(aux));
    }
}

/// `DesignatedInitExpr`: `a`, the initialiser; the designators are the
/// `ColdExprData` row (`designators`).
pub(crate) fn designated_init_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    run(w, s, e, designated_init_rows);
}

fn designated_init_rows(_: &Walk<'_>, s: &mut Stream, _: ExprId, n: &Node, out: &mut Vec<R>) {
    check_bits(s, n, 0, Cause::Unaccounted("ExprNode/DesignatedInitExpr.bits"));
    check_aux(s, n, 0, Cause::Unaccounted("ExprNode/DesignatedInitExpr.aux"));
    child_word(s, out, n.a, Cause::Unaccounted("ExprNode/DesignatedInitExpr.a"));
    if n.b != 0 {
        s.indeterminate(Cause::Unaccounted("ExprNode/DesignatedInitExpr.b"));
    }
}

/// `CXXParenListInitExpr`: `a` (`child*`), `b` the array filler
/// (`child?`). A union's member is `ColdExprData` `union_field`.
pub(crate) fn cxx_paren_list_init_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    run(w, s, e, paren_list_init_rows);
}

fn paren_list_init_rows(w: &Walk<'_>, s: &mut Stream, _: ExprId, n: &Node, out: &mut Vec<R>) {
    check_bits(s, n, 0, Cause::Unaccounted("ExprNode/CXXParenListInitExpr.bits"));
    check_aux(s, n, 0, Cause::Unaccounted("ExprNode/CXXParenListInitExpr.aux"));
    child_list(w, s, out, n.a, Cause::Unaccounted("ExprNode/CXXParenListInitExpr.a"));
    child_opt_word(out, n.b);
}

/// `ArrayInitLoopExpr`: `a`, the common expression (an
/// `OpaqueValueExpr`), then `b`, the element initialiser. `b` reaches the
/// same `OpaqueValueExpr` again (`kiln_sema` class/special/define.rs:1059,
/// `struct_bind.rs:446`): by the opaque-value rule its source is written
/// the first time only (in `a`), so nested loops stay linear.
pub(crate) fn array_init_loop_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    run(w, s, e, array_init_loop_rows);
}

fn array_init_loop_rows(_: &Walk<'_>, s: &mut Stream, _: ExprId, n: &Node, out: &mut Vec<R>) {
    check_bits(s, n, 0, Cause::Unaccounted("ExprNode/ArrayInitLoopExpr.bits"));
    check_aux(s, n, 0, Cause::Unaccounted("ExprNode/ArrayInitLoopExpr.aux"));
    child_word(s, out, n.a, Cause::Unaccounted("ExprNode/ArrayInitLoopExpr.a"));
    child_word(s, out, n.b, Cause::Unaccounted("ExprNode/ArrayInitLoopExpr.b"));
}

/// `ArrayInitIndexExpr`: the common rows alone.
pub(crate) fn array_init_index_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    run(w, s, e, array_init_index_rows);
}

fn array_init_index_rows(_: &Walk<'_>, s: &mut Stream, _: ExprId, n: &Node, _: &mut Vec<R>) {
    no_rows(s, n, "ExprNode/ArrayInitIndexExpr.bits", "ExprNode/ArrayInitIndexExpr.aux");
}

/// `CXXStdInitializerListExpr`: `a`.
pub(crate) fn cxx_std_initializer_list_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    run(w, s, e, std_initializer_list_rows);
}

fn std_initializer_list_rows(_: &Walk<'_>, s: &mut Stream, _: ExprId, n: &Node, out: &mut Vec<R>) {
    check_bits(s, n, 0, Cause::Unaccounted("ExprNode/CXXStdInitializerListExpr.bits"));
    check_aux(s, n, 0, Cause::Unaccounted("ExprNode/CXXStdInitializerListExpr.aux"));
    child_word(s, out, n.a, Cause::Unaccounted("ExprNode/CXXStdInitializerListExpr.a"));
    if n.b != 0 {
        s.indeterminate(Cause::Unaccounted("ExprNode/CXXStdInitializerListExpr.b"));
    }
}

/// `CompoundLiteralExpr`: `a`. A non-zero `aux` is a file-scope compound
/// literal (`kiln_sema` gnu.rs:761), whose implicit variable (`b`) has no
/// key: `unsupported`.
pub(crate) fn compound_literal_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    run(w, s, e, compound_literal_rows);
}

fn compound_literal_rows(_: &Walk<'_>, s: &mut Stream, _: ExprId, n: &Node, out: &mut Vec<R>) {
    check_bits(s, n, 0, Cause::Unaccounted("ExprNode/CompoundLiteralExpr.bits"));
    if n.aux != 0 {
        s.indeterminate(Cause::Unsupported("ExprNode/CompoundLiteralExpr.aux"));
    } else if n.b != 0 {
        s.indeterminate(Cause::Unaccounted("ExprNode/CompoundLiteralExpr.b"));
    }
    child_word(s, out, n.a, Cause::Unaccounted("ExprNode/CompoundLiteralExpr.a"));
}

/// `NoInitExpr`: the common rows alone.
pub(crate) fn no_init_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    run(w, s, e, no_init_rows);
}

fn no_init_rows(_: &Walk<'_>, s: &mut Stream, _: ExprId, n: &Node, _: &mut Vec<R>) {
    no_rows(s, n, "ExprNode/NoInitExpr.bits", "ExprNode/NoInitExpr.aux");
}

/// `DesignatedInitUpdateExpr`: `a`, the base, then `b`, the update list.
pub(crate) fn designated_init_update_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    run(w, s, e, designated_init_update_rows);
}

fn designated_init_update_rows(
    _: &Walk<'_>,
    s: &mut Stream,
    _: ExprId,
    n: &Node,
    out: &mut Vec<R>,
) {
    check_bits(s, n, 0, Cause::Unaccounted("ExprNode/DesignatedInitUpdateExpr.bits"));
    check_aux(s, n, 0, Cause::Unaccounted("ExprNode/DesignatedInitUpdateExpr.aux"));
    child_word(s, out, n.a, Cause::Unaccounted("ExprNode/DesignatedInitUpdateExpr.a"));
    child_word(s, out, n.b, Cause::Unaccounted("ExprNode/DesignatedInitUpdateExpr.b"));
}

/// `ParenListExpr` (a template's `(a, b)` initialiser): `a` (`child*`).
pub(crate) fn paren_list_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    run(w, s, e, paren_list_rows);
}

fn paren_list_rows(w: &Walk<'_>, s: &mut Stream, _: ExprId, n: &Node, out: &mut Vec<R>) {
    check_bits(s, n, 0, Cause::Unaccounted("ExprNode/ParenListExpr.bits"));
    check_aux(s, n, 0, Cause::Unaccounted("ExprNode/ParenListExpr.aux"));
    child_list(w, s, out, n.a, Cause::Unaccounted("ExprNode/ParenListExpr.a"));
    if n.b != 0 {
        s.indeterminate(Cause::Unaccounted("ExprNode/ParenListExpr.b"));
    }
}

// ---------------------------------------------------------------------
// Constructors and default initialisers.

/// `CXXConstructExpr`: `aux` (`ConstructInfo`), `a` the constructor, `b`
/// the arguments. `bits-had-multiple-candidates` is `out`: the
/// constructor chosen is the fact (digest §6).
pub(crate) fn cxx_construct_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    run(w, s, e, construct_rows_complete);
}

fn construct_rows_complete(w: &Walk<'_>, s: &mut Stream, _: ExprId, n: &Node, out: &mut Vec<R>) {
    construct_rows(w, s, n, out, CONSTRUCT_CAUSES);
}

/// `CXXTemporaryObjectExpr`: the rows of `CXXConstructExpr`.
pub(crate) fn cxx_temporary_object_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    run(w, s, e, construct_rows_temporary);
}

fn construct_rows_temporary(w: &Walk<'_>, s: &mut Stream, _: ExprId, n: &Node, out: &mut Vec<R>) {
    construct_rows(w, s, n, out, TEMPORARY_CAUSES);
}

/// The causes of one constructing kind: `bits`, `aux`, `a`, `b`.
type ConstructCauses = [&'static str; 4];

const CONSTRUCT_CAUSES: ConstructCauses = [
    "ExprNode/CXXConstructExpr.bits",
    "ExprNode/CXXConstructExpr.aux",
    "ExprNode/CXXConstructExpr.a",
    "ExprNode/CXXConstructExpr.b",
];

const TEMPORARY_CAUSES: ConstructCauses = [
    "ExprNode/CXXTemporaryObjectExpr.bits",
    "ExprNode/CXXTemporaryObjectExpr.aux",
    "ExprNode/CXXTemporaryObjectExpr.a",
    "ExprNode/CXXTemporaryObjectExpr.b",
];

fn construct_rows(w: &Walk<'_>, s: &mut Stream, n: &Node, out: &mut Vec<R>, c: ConstructCauses) {
    let [on_bits, on_aux, on_a, on_b] = c;
    check_bits(s, n, bit(ExprFlags::HAD_MULTIPLE_CANDIDATES), Cause::Unaccounted(on_bits));
    check_aux(s, n, 0x3F, Cause::Unaccounted(on_aux));
    let ConstructInfo { kind, list, std_init_list, zeroing, immediate_escalating } =
        ConstructInfo::from_aux(n.aux & 0x3F);
    out.push(R::Tag(construction_kind::row(&kind)));
    out.push(R::Flags(
        FlagWord::new().bit(list).bit(std_init_list).bit(zeroing).bit(immediate_escalating),
    ));
    entity(s, out, n.a, Cause::Unaccounted(on_a));
    child_list(w, s, out, n.b, Cause::Unaccounted(on_b));
}

/// `CXXInheritedCtorInitExpr`: `aux-kind`, `aux-virtual-base`, `a` the
/// inherited constructor.
pub(crate) fn cxx_inherited_ctor_init_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    run(w, s, e, inherited_ctor_init_rows);
}

fn inherited_ctor_init_rows(_: &Walk<'_>, s: &mut Stream, _: ExprId, n: &Node, out: &mut Vec<R>) {
    check_bits(s, n, 0, Cause::Unaccounted("ExprNode/CXXInheritedCtorInitExpr.bits"));
    check_aux(s, n, 7, Cause::Unaccounted("ExprNode/CXXInheritedCtorInitExpr.aux"));
    construction_kind_row(
        s,
        out,
        n.aux,
        Cause::Unaccounted("ExprNode/CXXInheritedCtorInitExpr.aux-kind"),
    );
    out.push(R::Flags(FlagWord::new().bit(n.aux & 4 != 0)));
    entity(s, out, n.a, Cause::Unaccounted("ExprNode/CXXInheritedCtorInitExpr.a"));
    if n.b != 0 {
        s.indeterminate(Cause::Unaccounted("ExprNode/CXXInheritedCtorInitExpr.b"));
    }
}

/// `CXXDefaultArgExpr`: `a`, the parameter by its position (`ParamData`
/// `index`); its function is the callee or constructor of the enclosing
/// call, written there. The default argument itself is the function's
/// (§7 "Shared nodes"); `b`, the context of the use, is `out`.
pub(crate) fn cxx_default_arg_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    run(w, s, e, default_arg_rows);
    crate::expr_misc::default_use_names(w, s, e);
}

fn default_arg_rows(w: &Walk<'_>, s: &mut Stream, _: ExprId, n: &Node, out: &mut Vec<R>) {
    check_bits(s, n, 0, Cause::Unaccounted("ExprNode/CXXDefaultArgExpr.bits"));
    check_aux(s, n, 0, Cause::Unaccounted("ExprNode/CXXDefaultArgExpr.aux"));
    let index = DeclId::try_new(n.a).and_then(|d| w.ast.decls.param(d)).map(|p| p.index);
    if let Some(index) = index {
        out.push(R::Uv(u64::from(index)));
    } else {
        out.push(R::Uv(0));
        s.indeterminate(Cause::Unaccounted("ExprNode/CXXDefaultArgExpr.a"));
    }
}

/// `CXXDefaultInitExpr`: `a`, the field. The initialiser is the field's
/// (§7 "Shared nodes"); `aux` (always 1) and `b` (the context of the
/// use) are `out`. `ColdExprData` `default_imm` is `demand`.
pub(crate) fn cxx_default_init_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    run(w, s, e, default_init_rows);
    crate::expr_misc::default_use_names(w, s, e);
}

fn default_init_rows(_: &Walk<'_>, s: &mut Stream, _: ExprId, n: &Node, out: &mut Vec<R>) {
    check_bits(s, n, 0, Cause::Unaccounted("ExprNode/CXXDefaultInitExpr.bits"));
    if n.aux != 1 {
        s.indeterminate(Cause::Unaccounted("ExprNode/CXXDefaultInitExpr.aux"));
    }
    entity(s, out, n.a, Cause::Unaccounted("ExprNode/CXXDefaultInitExpr.a"));
}

/// `CXXScalarValueInitExpr`: the common rows alone.
pub(crate) fn cxx_scalar_value_init_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    run(w, s, e, scalar_value_init_rows);
}

fn scalar_value_init_rows(_: &Walk<'_>, s: &mut Stream, _: ExprId, n: &Node, _: &mut Vec<R>) {
    no_rows(s, n, "ExprNode/CXXScalarValueInitExpr.bits", "ExprNode/CXXScalarValueInitExpr.aux");
}

// ---------------------------------------------------------------------
// `new`, `delete`, destruction and `throw`.

/// `CXXNewExpr`: the flag word (`bits-global`, `bits-array-form`,
/// `bits-pass-align`), `aux` (`NewInitStyle`), then `a`, the `NewExtra`.
pub(crate) fn cxx_new_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    run(w, s, e, new_rows);
}

fn new_rows(w: &Walk<'_>, s: &mut Stream, e: ExprId, n: &Node, out: &mut Vec<R>) {
    let known = bit(ExprFlags::GLOBAL) | bit(ExprFlags::ARRAY_FORM) | bit(ExprFlags::PASS_ALIGN);
    check_bits(s, n, known, Cause::Unaccounted("ExprNode/CXXNewExpr.bits"));
    out.push(R::Flags(
        FlagWord::new()
            .bit(has(n, ExprFlags::GLOBAL))
            .bit(has(n, ExprFlags::ARRAY_FORM))
            .bit(has(n, ExprFlags::PASS_ALIGN)),
    ));
    if let Some(style) = NewInitStyle::from_u8(n.aux) {
        out.push(R::Tag(new_init_style::row(&style)));
    } else {
        out.push(R::Uv(0));
        s.indeterminate(Cause::Unaccounted("ExprNode/CXXNewExpr.aux"));
    }
    let &NewExtra {
        allocated_ty,
        array_size,
        init,
        placement,
        operator_new,
        operator_delete,
        type_loc: _,
    } = w.ast.exprs.new_extra(e);
    out.push(R::Type(allocated_ty));
    child_opt(out, array_size);
    child_opt(out, init);
    out.push(R::Opt(placement.is_some()));
    if let Some(l) = placement {
        child_list_id(w, s, out, l, Cause::Unaccounted("NewExtra/placement"));
    }
    out.push(R::RefOpt(operator_new));
    out.push(R::RefOpt(operator_delete));
    if n.b != 0 {
        s.indeterminate(Cause::Unaccounted("ExprNode/CXXNewExpr.b"));
    }
}

/// `CXXDeleteExpr`: one flag word (`bits-global`, `bits-array-form`,
/// `bits-array-as-written`, `aux-sized`, `aux-aligned`), `a` the operand,
/// `b` the deallocation function. Kiln looks the function up at the
/// expression from the operand's static type (`kiln_sema`
/// `new_delete.rs:2333`), so it is a fact of the expression; a virtual
/// destructor's own deallocation function is the class's
/// (`dtor_delete`), not this row.
pub(crate) fn cxx_delete_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    run(w, s, e, delete_rows);
}

fn delete_rows(_: &Walk<'_>, s: &mut Stream, _: ExprId, n: &Node, out: &mut Vec<R>) {
    let known =
        bit(ExprFlags::GLOBAL) | bit(ExprFlags::ARRAY_FORM) | bit(ExprFlags::ARRAY_AS_WRITTEN);
    check_bits(s, n, known, Cause::Unaccounted("ExprNode/CXXDeleteExpr.bits"));
    check_aux(s, n, 3, Cause::Unaccounted("ExprNode/CXXDeleteExpr.aux"));
    out.push(R::Flags(
        FlagWord::new()
            .bit(has(n, ExprFlags::GLOBAL))
            .bit(has(n, ExprFlags::ARRAY_FORM))
            .bit(has(n, ExprFlags::ARRAY_AS_WRITTEN))
            .bit(n.aux & 1 != 0)
            .bit(n.aux & 2 != 0),
    ));
    child_word(s, out, n.a, Cause::Unaccounted("ExprNode/CXXDeleteExpr.a"));
    out.push(R::RefOpt(DeclId::try_new(n.b)));
}

/// `CXXPseudoDestructorExpr`: the flag word (`aux-arrow`,
/// `aux-qualified`), `a` the object, `b` the destroyed type.
/// `aux-qualified` is written only inside an instantiation-dependent
/// expression, as `CXXThisExpr.aux-implicit` is: only substitution
/// (`kiln_sema` templ/subst/exprs.rs:834), the mangling of dependent
/// expressions and their profiles read it.
pub(crate) fn cxx_pseudo_destructor_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    run(w, s, e, pseudo_destructor_rows);
    // Rule 11 of §4.4 "Spellings": a qualifier Kiln did not keep.
    crate::own_spelling::pseudo_destructor(w, s, e);
}

fn pseudo_destructor_rows(w: &Walk<'_>, s: &mut Stream, _: ExprId, n: &Node, out: &mut Vec<R>) {
    check_bits(s, n, 0, Cause::Unaccounted("ExprNode/CXXPseudoDestructorExpr.bits"));
    check_aux(s, n, 3, Cause::Unaccounted("ExprNode/CXXPseudoDestructorExpr.aux"));
    let dependent = w.unit.expr.dependent > 0;
    out.push(R::Flags(FlagWord::new().bit(n.aux & 1 != 0).bit(dependent && n.aux & 2 != 0)));
    child_word(s, out, n.a, Cause::Unaccounted("ExprNode/CXXPseudoDestructorExpr.a"));
    if let Some(q) = QualType::from_raw(n.b) {
        out.push(R::Type(q));
    } else {
        out.push(R::Uv(0));
        s.indeterminate(Cause::Unaccounted("ExprNode/CXXPseudoDestructorExpr.b"));
    }
}

/// `CXXThrowExpr`: `a`, the operand (`child?`: absent for a rethrow).
pub(crate) fn cxx_throw_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    run(w, s, e, throw_rows);
}

fn throw_rows(_: &Walk<'_>, s: &mut Stream, _: ExprId, n: &Node, out: &mut Vec<R>) {
    check_bits(s, n, 0, Cause::Unaccounted("ExprNode/CXXThrowExpr.bits"));
    check_aux(s, n, 0, Cause::Unaccounted("ExprNode/CXXThrowExpr.aux"));
    child_opt_word(out, n.a);
    if n.b != 0 {
        s.indeterminate(Cause::Unaccounted("ExprNode/CXXThrowExpr.b"));
    }
}

// ---------------------------------------------------------------------
// Rewritten comparisons.

/// `CXXRewrittenBinaryOperator`: `aux` the operator as written, `a` the
/// semantic form, then `b` as one `bool`: whether the two operands as
/// written are in the reverse of their order in `a`.
///
/// The operands are the very nodes the semantic form holds (`kiln_sema`
/// overload/oper.rs:468 and :1184; the unit test
/// `rewritten_operands_are_in_the_semantic_form` pins it), written there,
/// so writing them again would double the walk at every nested rewritten
/// comparison. Their written order is still a fact: instantiation rebuilds
/// the comparison from it (templ/subst/exprs.rs:717), and a reversed
/// `x != y` against `y != x` can have the same semantic form. The order is
/// found by [`written_reversed`]; an operand it does not find under `a`
/// is `Unaccounted`.
pub(crate) fn cxx_rewritten_binary_operator(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    run(w, s, e, rewritten_rows);
}

fn rewritten_rows(w: &Walk<'_>, s: &mut Stream, _: ExprId, n: &Node, out: &mut Vec<R>) {
    check_bits(s, n, 0, Cause::Unaccounted("ExprNode/CXXRewrittenBinaryOperator.bits"));
    if let Some(op) = BinaryOp::from_u8(n.aux) {
        out.push(R::Tag(binary_op::row(&op)));
    } else {
        out.push(R::Uv(0));
        s.indeterminate(Cause::Unaccounted("ExprNode/CXXRewrittenBinaryOperator.aux"));
    }
    child_word(s, out, n.a, Cause::Unaccounted("ExprNode/CXXRewrittenBinaryOperator.a"));
    let cause = Cause::Unaccounted("ExprNode/CXXRewrittenBinaryOperator.b");
    let written = expr_list(w, s, n.b, cause);
    let reversed = match (ExprId::try_new(n.a), &written[..]) {
        (Some(semantic), &[left, right]) => written_reversed(w, semantic, [left, right]),
        _ => None,
    };
    if reversed.is_none() {
        s.indeterminate(cause);
    }
    out.push(R::Bool(reversed.unwrap_or(false)));
}

/// How many nodes of the semantic form, outside the two operands,
/// [`written_reversed`] looks at before giving up. The nodes Kiln builds
/// around the operands (the call, its callee, the conversions of its
/// arguments, a `!` or a comparison with `0`) are a handful.
const REWRITTEN_FRAME: usize = 256;

/// Whether `written`, the operands as written, meet in the reverse order
/// in a pre-order walk of `a` over Kiln's children
/// (`ExprArena::children`, an opaque value's source included) that does
/// not enter either operand. `None` when one is not found within
/// [`REWRITTEN_FRAME`] nodes.
fn written_reversed(w: &Walk<'_>, a: ExprId, written: [ExprId; 2]) -> Option<bool> {
    let exprs = &w.ast.exprs;
    let mut todo = vec![a];
    let mut first = None;
    let mut frame = 0;
    let mut kids = Vec::new();
    while let Some(e) = todo.pop() {
        if written.contains(&e) {
            match first {
                None => first = Some(e),
                Some(f) if f != e => return Some(f == written[1]),
                Some(_) => {}
            }
            continue;
        }
        frame += 1;
        if frame > REWRITTEN_FRAME {
            return None;
        }
        kids.clear();
        exprs.children_into(e, &w.ast.lists, &mut kids);
        if exprs.get(e).kind() == slip_kiln::kiln::ast::expr::ExprKind::OpaqueValueExpr {
            kids.extend(exprs.opaque_source(e));
        }
        todo.extend(kids.iter().rev());
    }
    None
}

// ---------------------------------------------------------------------
// `ColdExprData` rows of this file.

/// `ColdExprData` `designators` (§7.4) of `e`, which has an entry:
/// `enum Designator*`, outermost first. Called by `expr::cold` for an
/// expression with an entry; with none it writes the empty list itself.
pub(crate) fn designators(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let mut out = Vec::new();
    designator_rows(w, s, e, &mut out);
    emit(w, s, &out);
}

fn designator_rows(w: &Walk<'_>, s: &mut Stream, e: ExprId, out: &mut Vec<R>) {
    let ds: &[Designator] = w.ast.exprs.cold.designators.get(e).map_or(&[], |d| &d[..]);
    if w.ast.exprs.get(e).kind() != slip_kiln::kiln::ast::expr::ExprKind::DesignatedInitExpr {
        s.indeterminate(Cause::Unaccounted("ColdExprData/designators"));
    }
    out.push(R::Count(ds.len()));
    for d in ds {
        out.push(R::Tag(designator::row(d)));
        match *d {
            Designator::Field { name, loc: _ } => out.push(R::Sym(name)),
            Designator::Index { index, loc: _ } => out.push(R::Child(index)),
            Designator::Range { first, last, loc: _, ellipsis: _ } => {
                out.push(R::Child(first));
                out.push(R::Child(last));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! Row-level tests: the pair tests' own sources (`tests/<root>/<dir>/`)
    //! are parsed on both sides, the rows of every node of a kind are
    //! rendered one string per primitive (`render`), and the node the
    //! directory is about must render as written here on its side and not
    //! on the other. So a flag, a tag or an optional that the walk skipped
    //! or wrote as a constant fails here even where the pair test's sides
    //! differ in other rows too, and a node that no unit reaches yet (a
    //! function body, a constructor's initialisers) is checked all the
    //! same.

    use std::collections::HashSet;
    use std::path::Path;

    use slip_kiln::kiln::ast::decl::DeclKind;
    use slip_kiln::kiln::ast::dump::AstNames;
    use slip_kiln::kiln::ast::expr::ExprKind;
    use slip_kiln::kiln::ast::types::{PrintOptions, print_type};
    use slip_kiln::kiln::ast::{DeclId, ExprId, QualType};
    use slip_model::keys::RuleTable;

    use super::{
        Node, R, array_init_index_rows, array_init_loop_rows, compound_literal_rows,
        construct_rows_complete, construct_rows_temporary, default_arg_rows, default_init_rows,
        delete_rows, designated_init_rows, designated_init_update_rows, designator_rows,
        implicit_value_init_rows, inherited_ctor_init_rows, init_list_rows, new_rows, no_init_rows,
        paren_list_init_rows, paren_list_rows, pseudo_destructor_rows, rewritten_rows,
        scalar_value_init_rows, std_initializer_list_rows, throw_rows,
    };
    use crate::Input;
    use crate::expr::{child, node};
    use crate::stream::Stream;
    use crate::walk::{STACK_SIZE, Walk};

    type RowsFn = fn(&Walk<'_>, &mut Stream, ExprId, &Node, &mut Vec<R>);

    fn designators_of(w: &Walk<'_>, s: &mut Stream, e: ExprId, _: &Node, out: &mut Vec<R>) {
        designator_rows(w, s, e, out);
    }

    /// The rows functions of each kind of this file; a
    /// `DesignatedInitExpr` also renders its `designators`.
    const KINDS: &[(ExprKind, RowsFn)] = &[
        (ExprKind::InitListExpr, init_list_rows),
        (ExprKind::ImplicitValueInitExpr, implicit_value_init_rows),
        (ExprKind::DesignatedInitExpr, designated_init_rows),
        (ExprKind::DesignatedInitExpr, designators_of),
        (ExprKind::CXXConstructExpr, construct_rows_complete),
        (ExprKind::CXXTemporaryObjectExpr, construct_rows_temporary),
        (ExprKind::CXXInheritedCtorInitExpr, inherited_ctor_init_rows),
        (ExprKind::CXXDefaultArgExpr, default_arg_rows),
        (ExprKind::CXXDefaultInitExpr, default_init_rows),
        (ExprKind::CXXScalarValueInitExpr, scalar_value_init_rows),
        (ExprKind::CXXParenListInitExpr, paren_list_init_rows),
        (ExprKind::ArrayInitLoopExpr, array_init_loop_rows),
        (ExprKind::ArrayInitIndexExpr, array_init_index_rows),
        (ExprKind::CXXStdInitializerListExpr, std_initializer_list_rows),
        (ExprKind::CompoundLiteralExpr, compound_literal_rows),
        (ExprKind::NoInitExpr, no_init_rows),
        (ExprKind::DesignatedInitUpdateExpr, designated_init_update_rows),
        (ExprKind::CXXNewExpr, new_rows),
        (ExprKind::CXXDeleteExpr, delete_rows),
        (ExprKind::CXXPseudoDestructorExpr, pseudo_destructor_rows),
        (ExprKind::CXXThrowExpr, throw_rows),
        (ExprKind::CXXRewrittenBinaryOperator, rewritten_rows),
        (ExprKind::ParenListExpr, paren_list_rows),
    ];

    fn ty(w: &Walk<'_>, q: QualType) -> String {
        print_type(&w.ast.types, w.ast, q, PrintOptions::CANONICAL)
    }

    /// One string for the rows of `e`: its primitives, `; `-separated,
    /// and the cause they found, if any.
    fn render(w: &Walk<'_>, e: ExprId) -> String {
        let n = node(w, e);
        let mut stream = Stream::new();
        let mut out = Vec::new();
        for &(k, f) in KINDS {
            if k == n.kind {
                f(w, &mut stream, e, &n, &mut out);
            }
        }
        let mut parts: Vec<String> = out
            .iter()
            .map(|r| match *r {
                R::Child(c) => format!("child {:?}", w.ast.exprs.get(c).kind()),
                R::Uv(v) => format!("uv {v}"),
                R::Tag(row) => format!("tag {}", row.tag),
                R::Flags(f) => format!("flags {}", f.value()),
                R::Count(c) => format!("count {c}"),
                R::Opt(p) => format!("opt {p}"),
                R::Bool(b) => format!("bool {b}"),
                R::Ref(d) | R::RefOpt(Some(d)) => {
                    let mut name = String::new();
                    AstNames::new(w.ast).write_qualified_name(d, &mut name);
                    format!("ref {name}")
                }
                R::RefOpt(None) => "ref none".to_owned(),
                R::Type(q) => format!("type {}", ty(w, q)),
                R::Sym(sym) => format!("sym {}", w.ast.symbols().as_str(sym)),
            })
            .collect();
        if let Some(c) = stream.cause() {
            parts.push(format!("cause {c}"));
        }
        parts.join("; ")
    }

    /// Parses `src` (a `main.cpp` with the given extra files) and runs `f`
    /// on a walk over it.
    fn with_walk<T>(files: &[(&str, &str)], std: &str, f: impl FnOnce(&mut Walk<'_>) -> T) -> T {
        let snippet = slip_kiln::snippet::Snippet { files, main: "main.cpp", include: &[""], std };
        slip_kiln::snippet::parse_with_tables(
            &snippet,
            &slip_kiln::snippet::Extra::default(),
            |p| {
                let path = |_| Vec::new();
                let input = Input { ast: p.ast, sm: p.sm, subject_path: &path };
                let rules = RuleTable::empty();
                let mut w = Walk::new(&input, &rules);
                f(&mut w)
            },
        )
        .expect("parses")
    }

    /// Every expression of the walk's AST.
    fn exprs(w: &Walk<'_>) -> Vec<ExprId> {
        (1..=w.ast.exprs.len()).filter_map(|i| ExprId::try_new(u32::try_from(i).ok()?)).collect()
    }

    /// The renderings of the nodes of `kind` on each side of the pair test
    /// `tests/<dir>` (`t.cpp` with `side.h`, or `a.cpp` and `b.cpp`).
    fn sides(dir: &str, kind: ExprKind) -> [Vec<String>; 2] {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join(dir);
        let read = |n: &str| std::fs::read_to_string(root.join(n)).ok();
        let mut out = [Vec::new(), Vec::new()];
        for (i, b) in [false, true].into_iter().enumerate() {
            let side = if b { "#define B 1\n" } else { "" };
            let main = read("t.cpp").or_else(|| read(if b { "b.cpp" } else { "a.cpp" }));
            let main = main.unwrap_or_else(|| panic!("{dir}: no source"));
            let std = main
                .lines()
                .take(4)
                .find_map(|l| l.strip_prefix("// std:"))
                .map_or("c++20", str::trim)
                .to_owned();
            let files = [("main.cpp", main.as_str()), ("side.h", side)];
            out[i] = with_walk(&files, &std, |w| {
                exprs(w)
                    .into_iter()
                    .filter(|&e| w.ast.exprs.get(e).kind() == kind)
                    .map(|e| render(w, e))
                    .collect()
            });
        }
        out
    }

    /// The node of `kind` that `dir` is about renders as `a` on side A
    /// and as `b` on side B, and neither rendering is on the other side.
    #[track_caller]
    fn check(dir: &str, kind: ExprKind, a: &str, b: Option<&str>) {
        let [on_a, on_b] = sides(dir, kind);
        assert!(on_a.iter().any(|r| r == a), "{dir}: side A has no {kind:?} `{a}`: {on_a:#?}");
        assert!(!on_b.iter().any(|r| r == a), "{dir}: side B has {kind:?} `{a}` too: {on_b:#?}");
        if let Some(b) = b {
            assert!(on_b.iter().any(|r| r == b), "{dir}: side B has no {kind:?} `{b}`: {on_b:#?}");
            assert!(
                !on_a.iter().any(|r| r == b),
                "{dir}: side A has {kind:?} `{b}` too: {on_a:#?}"
            );
        }
    }

    /// The node of `kind` in the source of `dir` renders as `r` on side A
    /// (an indeterminate test has one side).
    #[track_caller]
    fn check_a(dir: &str, kind: ExprKind, r: &str) {
        let [on_a, _] = sides(dir, kind);
        assert!(on_a.iter().any(|x| x == r), "{dir}: side A has no {kind:?} `{r}`: {on_a:#?}");
    }

    /// The node of `kind` in the source of `dir` renders as `r` on both
    /// sides: the pair differs elsewhere (a child's own rows).
    #[track_caller]
    fn check_both(dir: &str, kind: ExprKind, r: &str) {
        let [on_a, on_b] = sides(dir, kind);
        assert!(on_a.iter().any(|x| x == r), "{dir}: side A has no {kind:?} `{r}`: {on_a:#?}");
        assert!(on_b.iter().any(|x| x == r), "{dir}: side B has no {kind:?} `{r}`: {on_b:#?}");
    }

    /// Every `in` row of this file that is an enumeration, a flag or an
    /// optional: the pair test's node renders with the row's value on
    /// each side (module documentation).
    #[test]
    #[allow(clippy::too_many_lines, reason = "one check per row")]
    fn enumerations_flags_and_optionals() {
        use ExprKind as K;
        // InitListExpr.b, InitListExtra.
        let filler =
            "count 1; uv 1; child CXXConstructExpr; opt true; child CXXConstructExpr; ref none";
        check_both("differ/ExprNode/InitListExpr.b", K::InitListExpr, filler);
        check_both("differ/InitListExtra/array_filler", K::InitListExpr, filler);
        let filler_ctor = (
            "tag 1; flags 1; ref E::E; count 0",
            "tag 1; flags 1; ref E::E; count 1; child CXXDefaultArgExpr",
        );
        check(
            "differ/InitListExtra/array_filler",
            K::CXXConstructExpr,
            filler_ctor.0,
            Some(filler_ctor.1),
        );
        check(
            "differ/InitListExtra/union_field",
            K::InitListExpr,
            "count 1; uv 1; child IntegerLiteral; opt false; ref U::i",
            Some("count 1; uv 1; child IntegerLiteral; opt false; ref U::j"),
        );
        // InitListExpr.a as runs: a range is one run of its shared node.
        check(
            "differ/_/init-list-range-length",
            K::InitListExpr,
            "count 2; uv 2; child IntegerLiteral; uv 2; child IntegerLiteral; opt false; ref none",
            Some(
                "count 2; uv 3; child IntegerLiteral; uv 1; child IntegerLiteral; opt false; ref none",
            ),
        );
        check(
            "differ/_/init-list-range-against-written-out",
            K::InitListExpr,
            "count 1; uv 3; child IntegerLiteral; opt false; ref none",
            Some(
                "count 3; uv 1; child IntegerLiteral; uv 1; child IntegerLiteral; uv 1; \
                 child IntegerLiteral; opt false; ref none",
            ),
        );
        // ConstructInfo, ConstructionKind.
        let list = (
            "tag 1; flags 0; ref S::S; count 1; child IntegerLiteral",
            "tag 1; flags 1; ref S::S; count 1; child IntegerLiteral",
        );
        check("differ/ExprNode/CXXConstructExpr.aux", K::CXXConstructExpr, list.0, Some(list.1));
        check("differ/ConstructInfo/list", K::CXXConstructExpr, list.0, Some(list.1));
        check("differ/_/parens-against-braces", K::CXXConstructExpr, list.0, Some(list.1));
        check(
            "differ/ExprNode/CXXTemporaryObjectExpr.aux",
            K::CXXTemporaryObjectExpr,
            "tag 1; flags 0; ref S2::S2; count 2; child IntegerLiteral; child IntegerLiteral",
            Some("tag 1; flags 1; ref S2::S2; count 2; child IntegerLiteral; child IntegerLiteral"),
        );
        check(
            "differ/ConstructInfo/std_init_list",
            K::CXXConstructExpr,
            "tag 1; flags 3; ref L::L; count 1; child CXXStdInitializerListExpr",
            Some("tag 1; flags 1; ref L::L; count 1; child CXXStdInitializerListExpr"),
        );
        check(
            "differ/_/init-list-constructor-added",
            K::CXXConstructExpr,
            "tag 1; flags 1; ref T::T; count 1; child IntegerLiteral",
            Some("tag 1; flags 3; ref T::T; count 1; child CXXStdInitializerListExpr"),
        );
        check(
            "differ/ConstructInfo/zeroing",
            K::CXXTemporaryObjectExpr,
            "tag 1; flags 4; ref Z::Z; count 0",
            Some("tag 1; flags 0; ref Z::Z; count 0"),
        );
        check(
            "differ/ConstructInfo/immediate_escalating",
            K::CXXConstructExpr,
            "tag 1; flags 8; ref I::I; count 1; child ImplicitCastExpr",
            Some("tag 1; flags 0; ref I::I; count 1; child IntegerLiteral"),
        );
        let base = "flags 0; ref B1::B1; count 1; child IntegerLiteral";
        let kind = |tag: u32| format!("tag {tag}; {base}");
        check("differ/ConstructInfo/kind", K::CXXConstructExpr, &kind(2), Some(&kind(1)));
        check("differ/ConstructionKind/Complete", K::CXXConstructExpr, &kind(1), Some(&kind(2)));
        check(
            "differ/ConstructionKind/NonVirtualBase",
            K::CXXConstructExpr,
            &kind(2),
            Some(&kind(3)),
        );
        check("differ/ConstructionKind/VirtualBase", K::CXXConstructExpr, &kind(3), Some(&kind(2)));
        check(
            "differ/ConstructionKind/Delegating",
            K::CXXConstructExpr,
            "tag 4; flags 0; ref D::D; count 1; child IntegerLiteral",
            None,
        );
        // CXXInheritedCtorInitExpr.
        check(
            "differ/ExprNode/CXXInheritedCtorInitExpr.aux-kind",
            K::CXXInheritedCtorInitExpr,
            "tag 2; flags 0; ref B1::B1",
            Some("tag 3; flags 0; ref B1::B1"),
        );
        check(
            "differ/ExprNode/CXXInheritedCtorInitExpr.aux-virtual-base",
            K::CXXInheritedCtorInitExpr,
            "tag 2; flags 0; ref M::B1",
            Some("tag 2; flags 1; ref M::B1"),
        );
        // CXXDefaultArgExpr.a.
        // Side A passes the first argument from its default, side B the
        // second only.
        check("differ/ExprNode/CXXDefaultArgExpr.a", K::CXXDefaultArgExpr, "uv 0", None);
        // CXXParenListInitExpr.b.
        check_both(
            "differ/ExprNode/CXXParenListInitExpr.b",
            K::CXXParenListInitExpr,
            "count 1; child CXXConstructExpr; opt true; child CXXConstructExpr",
        );
        // CXXNewExpr, NewInitStyle, NewExtra.
        let new = |flags: u32, rest: &str| format!("flags {flags}; {rest}");
        let plain = "tag 1; type int; opt false; opt false; opt false; ref operator new; ref operator delete";
        check(
            "differ/ExprNode/CXXNewExpr.bits-global",
            K::CXXNewExpr,
            &new(1, plain),
            Some(&new(0, plain)),
        );
        let deduced = |flags: u32| {
            format!(
                "flags {flags}; tag 3; type int; opt false; opt true; child InitListExpr; opt false; ref none; ref none"
            )
        };
        check(
            "differ/ExprNode/CXXNewExpr.bits-array-form",
            K::CXXNewExpr,
            &deduced(2),
            Some(&deduced(0)),
        );
        check(
            "differ/ExprNode/CXXNewExpr.bits-pass-align",
            K::CXXNewExpr,
            "flags 4; tag 1; type Al; opt false; opt false; opt false; ref operator new; ref operator delete",
            Some(
                "flags 0; tag 1; type Pl; opt false; opt false; opt false; ref operator new; ref operator delete",
            ),
        );
        let parens = "flags 0; tag 2; type int; opt false; opt true; child IntegerLiteral; opt false; \
                      ref operator new; ref operator delete";
        let braces = "flags 0; tag 3; type int; opt false; opt true; child InitListExpr; opt false; \
                      ref operator new; ref operator delete";
        let s_new = |tag: u32| {
            format!(
                "flags 0; tag {tag}; type S; opt false; opt true; child CXXConstructExpr; opt false; ref operator new; ref operator delete"
            )
        };
        check("differ/ExprNode/CXXNewExpr.aux", K::CXXNewExpr, &s_new(2), Some(&s_new(1)));
        check("differ/NewInitStyle/Parens", K::CXXNewExpr, parens, Some(braces));
        check("differ/NewInitStyle/Braces", K::CXXNewExpr, braces, Some(parens));
        check(
            "differ/NewInitStyle/None",
            K::CXXNewExpr,
            &new(0, plain),
            Some(
                "flags 0; tag 2; type int; opt false; opt true; child ImplicitValueInitExpr; opt false; \
                 ref operator new; ref operator delete",
            ),
        );
        check_both(
            "differ/NewExtra/placement",
            K::CXXNewExpr,
            "flags 0; tag 1; type int; opt false; opt false; opt true; count 1; child ImplicitCastExpr; ref operator new; ref none",
        );
        check(
            "differ/NewExtra/operator_new",
            K::CXXNewExpr,
            "flags 0; tag 1; type N; opt false; opt false; opt false; ref operator new; ref operator delete",
            Some(
                "flags 0; tag 1; type N; opt false; opt false; opt false; ref N::operator new; \
                 ref operator delete",
            ),
        );
        check(
            "differ/NewExtra/operator_delete",
            K::CXXNewExpr,
            "flags 0; tag 1; type N; opt false; opt true; child CXXConstructExpr; opt false; \
             ref N::operator new; ref operator delete",
            Some(
                "flags 0; tag 1; type N; opt false; opt true; child CXXConstructExpr; opt false; \
                 ref N::operator new; ref N::operator delete",
            ),
        );
        // CXXDeleteExpr: bit 0 global, 1 array form, 2 array as written,
        // 3 sized, 4 aligned.
        let del = |flags: u32, f: &str| format!("flags {flags}; child ImplicitCastExpr; ref {f}");
        check(
            "differ/ExprNode/CXXDeleteExpr.bits-global",
            K::CXXDeleteExpr,
            &del(9, "operator delete"),
            Some(&del(8, "operator delete")),
        );
        check(
            "differ/ExprNode/CXXDeleteExpr.bits-array-form",
            K::CXXDeleteExpr,
            &del(6, "operator delete[]"),
            Some(&del(8, "operator delete")),
        );
        check(
            "differ/ExprNode/CXXDeleteExpr.bits-array-as-written",
            K::CXXDeleteExpr,
            &del(6, "operator delete[]"),
            Some(&del(2, "operator delete[]")),
        );
        check(
            "differ/ExprNode/CXXDeleteExpr.aux-sized",
            K::CXXDeleteExpr,
            &del(8, "C::operator delete"),
            Some(&del(0, "D::operator delete")),
        );
        check(
            "differ/_/sized-delete",
            K::CXXDeleteExpr,
            &del(0, "C::operator delete"),
            Some(&del(8, "C::operator delete")),
        );
        check(
            "differ/ExprNode/CXXDeleteExpr.aux-aligned",
            K::CXXDeleteExpr,
            &del(24, "operator delete"),
            Some(&del(8, "operator delete")),
        );
        check(
            "differ/ExprNode/CXXDeleteExpr.b",
            K::CXXDeleteExpr,
            &del(0, "D1::operator delete"),
            Some(&del(0, "D2::operator delete")),
        );
        // CXXPseudoDestructorExpr: bit 0 arrow, 1 qualified.
        check(
            "differ/ExprNode/CXXPseudoDestructorExpr.aux-arrow",
            K::CXXPseudoDestructorExpr,
            "flags 1; child ImplicitCastExpr; type int",
            Some("flags 0; child ParenExpr; type int"),
        );
        // `aux-qualified` is written inside a dependent expression only,
        // where a qualified pseudo-destructor whose qualifier Kiln did not
        // keep is indeterminate (rule 11 of §4.4 "Spellings"): its test is
        // `indeterminate/_/pseudo-destructor-qualified-in-template`.
        check(
            "differ/ExprNode/CXXPseudoDestructorExpr.b",
            K::CXXPseudoDestructorExpr,
            "flags 1; child ImplicitCastExpr; type int",
            Some("flags 1; child ImplicitCastExpr; type const int"),
        );
        // CXXThrowExpr.a.
        check(
            "differ/ExprNode/CXXThrowExpr.a",
            K::CXXThrowExpr,
            "opt true; child IntegerLiteral",
            Some("opt false"),
        );
        // CXXRewrittenBinaryOperator.aux (`BinaryOp` 11 `<`, 12 `>`).
        let rewritten = |tag: u32| format!("tag {tag}; child CXXOperatorCallExpr; bool false");
        check(
            "differ/ExprNode/CXXRewrittenBinaryOperator.aux",
            K::CXXRewrittenBinaryOperator,
            &rewritten(11),
            Some(&rewritten(12)),
        );
        check(
            "differ/_/rewritten-against-real-operator",
            K::CXXRewrittenBinaryOperator,
            &rewritten(11),
            None,
        );
        check(
            "differ/_/rewritten-ne-reversed-same-semantic-form",
            K::CXXRewrittenBinaryOperator,
            "tag 16; child UnaryOperator; bool true",
            Some("tag 16; child UnaryOperator; bool false"),
        );
        check(
            "differ/_/rewritten-written-order-in-pattern",
            K::CXXRewrittenBinaryOperator,
            "tag 16; child UnaryOperator; bool true",
            Some("tag 16; child UnaryOperator; bool false"),
        );
        // Designator.
        let field = |n: &str| format!("child IntegerLiteral; count 1; tag 1; sym {n}");
        let index = "child IntegerLiteral; count 1; tag 2; child ConstantExpr";
        let range = "child IntegerLiteral; count 1; tag 3; child ConstantExpr; child ConstantExpr";
        check("differ/Designator/Field", K::DesignatedInitExpr, &field("x"), Some(index));
        check(
            "differ/Designator/Field.name",
            K::DesignatedInitExpr,
            &field("x"),
            Some(&field("y")),
        );
        check(
            "differ/ColdExprData/designators",
            K::DesignatedInitExpr,
            &field("x"),
            Some(&field("y")),
        );
        check("differ/Designator/Index", K::DesignatedInitExpr, index, Some(range));
        check("differ/Designator/Range", K::DesignatedInitExpr, range, Some(index));
        // ColdExprData syntactic_list: side A's list keeps its written form.
        check(
            "differ/ColdExprData/syntactic_list",
            K::InitListExpr,
            "count 2; uv 1; child DesignatedInitExpr; uv 1; child DesignatedInitExpr; opt false; ref none",
            None,
        );
        // CompoundLiteralExpr.aux, .b: unsupported when non-zero.
        let file_scope = "child InitListExpr; cause unsupported ExprNode/CompoundLiteralExpr.aux";
        check_a(
            "indeterminate/ExprNode/CompoundLiteralExpr.aux",
            K::CompoundLiteralExpr,
            file_scope,
        );
        check_a("indeterminate/ExprNode/CompoundLiteralExpr.b", K::CompoundLiteralExpr, file_scope);
        check(
            "differ/ExprKind/CompoundLiteralExpr",
            K::CompoundLiteralExpr,
            "child InitListExpr",
            None,
        );
    }

    /// `ColdExprData` `default_arg_rewritten`: side A of its test has an
    /// entry (written by `expr::cold`), side B none.
    #[test]
    fn default_arg_rewritten_has_an_entry() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/differ/ColdExprData/default_arg_rewritten/t.cpp");
        let main = std::fs::read_to_string(root).expect("the test");
        for (side, want) in [("", true), ("#define B 1\n", false)] {
            let files = [("main.cpp", main.as_str()), ("side.h", side)];
            let got = with_walk(&files, "c++20", |w| {
                exprs(w).into_iter().any(|e| w.ast.exprs.cold.default_arg_rewritten.contains(e))
            });
            assert_eq!(got, want, "side {side:?}");
        }
    }

    /// `b` of `CXXStdInitializerListExpr`, `DesignatedInitExpr` and
    /// `ParenListExpr` is no row's: a value other than 0 is
    /// `Unaccounted` with the row's name (digest-kinds §2.1).
    #[test]
    fn unaccounted_b_words() {
        let cases: [(&str, ExprKind, RowsFn, &str); 3] = [
            (
                "differ/ConstructInfo/std_init_list",
                ExprKind::CXXStdInitializerListExpr,
                std_initializer_list_rows,
                "ExprNode/CXXStdInitializerListExpr.b",
            ),
            (
                "differ/Designator/Field",
                ExprKind::DesignatedInitExpr,
                designated_init_rows,
                "ExprNode/DesignatedInitExpr.b",
            ),
            (
                "differ/ExprNode/ParenListExpr.a",
                ExprKind::ParenListExpr,
                paren_list_rows,
                "ExprNode/ParenListExpr.b",
            ),
        ];
        for (dir, kind, rows, row) in cases {
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join(dir).join("t.cpp");
            let main = std::fs::read_to_string(root).expect("the test");
            let files = [("main.cpp", main.as_str()), ("side.h", "")];
            with_walk(&files, "c++20", |w| {
                let e = exprs(w)
                    .into_iter()
                    .find(|&e| w.ast.exprs.get(e).kind() == kind)
                    .unwrap_or_else(|| panic!("{dir}: no {kind:?}"));
                let mut n = node(w, e);
                assert_eq!(n.b, 0, "{dir}: Kiln leaves `b` 0");
                let mut s = Stream::new();
                rows(w, &mut s, e, &n, &mut Vec::new());
                assert_eq!(s.cause(), None, "{dir}");
                n.b = 1;
                let mut s = Stream::new();
                rows(w, &mut s, e, &n, &mut Vec::new());
                assert_eq!(s.cause(), Some(crate::Cause::Unaccounted(row)), "{dir}");
            });
        }
    }

    /// A block-scope redeclaration's own default arguments are written by
    /// `local::write_decl` (WP-026): the declaration Kiln takes the
    /// default from at a call in that block (`kiln_sema` expr.rs:6189).
    #[test]
    fn block_scope_redeclaration_writes_its_default() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/differ/_/default-arg-block-scope-redeclaration/t.cpp");
        let main = std::fs::read_to_string(root).expect("the test");
        let mut out = Vec::new();
        for side in ["", "#define B 1\n"] {
            let files = [("main.cpp", main.as_str()), ("side.h", side)];
            out.push(with_walk(&files, "c++20", |w| {
                let ast = w.ast;
                let local = (1..=ast.decls.len())
                    .filter_map(|i| DeclId::try_new(u32::try_from(i).ok()?))
                    .find(|&d| {
                        ast.decls.get(d).kind() == DeclKind::Function
                            && ast
                                .decls
                                .get(d)
                                .ctx()
                                .is_some_and(|c| ast.decls.get(c).kind() == DeclKind::Function)
                    })
                    .expect("the block-scope redeclaration");
                let mut s = Stream::new();
                crate::local::write_decl(w, &mut s, local);
                (s.as_bytes().to_vec(), s.cause())
            }));
        }
        assert_eq!((out[0].1, out[1].1), (None, None), "no cause");
        assert_ne!(out[0].0, out[1].0, "the default `= 2` against `= 3` is written");
    }

    /// The bytes and the cause of the walk of the last expression of
    /// `kind` in `src` (C++20), on a thread of [`STACK_SIZE`].
    fn walk_last(src: String, kind: ExprKind) -> (usize, Option<crate::Cause>) {
        std::thread::Builder::new()
            .stack_size(STACK_SIZE)
            .spawn(move || {
                let files = [("main.cpp", src.as_str())];
                with_walk(&files, "c++20", |w| {
                    let root = exprs(w)
                        .into_iter()
                        .filter(|&e| w.ast.exprs.get(e).kind() == kind)
                        .max()
                        .expect("an expression of the kind");
                    let mut s = Stream::new();
                    child(w, &mut s, root);
                    assert_eq!(w.node_depth, 0, "every level left");
                    (s.as_bytes().len(), s.cause())
                })
            })
            .expect("a thread")
            .join()
            .expect("no panic")
    }

    /// An initialiser list of 100 000 elements is written in time and
    /// bytes linear in its length: one level of recursion for the list.
    #[test]
    fn a_long_list_is_linear() {
        let list = |n: usize| {
            let elems = vec!["1"; n].join(", ");
            format!("int v[{n}] = {{{elems}}};\n")
        };
        let (half, c1) = walk_last(list(50_000), ExprKind::InitListExpr);
        let (full, c2) = walk_last(list(100_000), ExprKind::InitListExpr);
        assert_eq!((c1, c2), (None, None));
        let per = (full - half) / 50_000;
        assert!(per > 0 && full <= 2 * half + 16, "50 000: {half} bytes; 100 000: {full}");
    }

    /// 2 000 nested braces (a dependent list keeps them as written) are
    /// written, linearly, well inside `NODE_LIMIT`.
    #[test]
    fn nested_braces_are_written() {
        let nested = |n: usize| {
            format!(
                "template <class T> struct A {{ static inline T w = {}1{}; }};\n",
                "{".repeat(n),
                "}".repeat(n)
            )
        };
        let (one, c1) = walk_last(nested(1_000), ExprKind::InitListExpr);
        let (two, c2) = walk_last(nested(2_000), ExprKind::InitListExpr);
        assert_eq!((c1, c2), (None, None));
        assert!(two <= 2 * one + 16, "1 000: {one} bytes; 2 000: {two}");
    }

    /// Nested array-init loops (the copy of a many-dimensional array in a
    /// structured binding) write each common expression once: every
    /// dimension more adds the same bytes.
    #[test]
    fn nested_array_init_loops_are_linear() {
        let decomposition =
            |dims: usize| format!("int a[2]{};\nauto [x, y] = a;\n", "[2]".repeat(dims - 1));
        let bytes: Vec<usize> = (2..=8)
            .map(|d| {
                let (b, c) = walk_last(decomposition(d), ExprKind::ArrayInitLoopExpr);
                assert_eq!(c, None);
                b
            })
            .collect();
        let steps: Vec<usize> = bytes.windows(2).map(|w| w[1] - w[0]).collect();
        assert!(steps.windows(2).all(|s| s[0] == s[1]), "bytes by dimension: {bytes:?}");
    }

    /// Every expression reachable from `root` in Kiln's AST: its children
    /// (`ExprArena::children`) and an opaque value's source.
    fn reachable(w: &Walk<'_>, root: ExprId) -> HashSet<ExprId> {
        let mut seen = HashSet::new();
        let mut todo = vec![root];
        while let Some(e) = todo.pop() {
            if seen.insert(e) {
                todo.extend(w.ast.exprs.children(e, &w.ast.lists));
                if w.ast.exprs.get(e).kind() == ExprKind::OpaqueValueExpr {
                    todo.extend(w.ast.exprs.opaque_source(e));
                }
            }
        }
        seen
    }

    /// `CXXRewrittenBinaryOperator.b` is `out` because both operands as
    /// written are nodes of the semantic form `a` (`kiln_sema`
    /// overload/oper.rs:536, :1184): checked for every rewrite Kiln makes.
    #[test]
    fn rewritten_operands_are_in_the_semantic_form() {
        const R: &str = "struct R { int r; };\n\
            bool operator<(R, int); bool operator>(R, int);\n\
            bool operator<=(R, int); bool operator>=(R, int);\n\
            bool operator<(int, R); bool operator>(int, R);\n\
            bool operator<=(int, R); bool operator>=(int, R);\n\
            R operator<=>(int, R);\n";
        let non_member = "struct X { int i; }; R operator<=>(X, int); X a;\n";
        let member = "struct X { int i; R operator<=>(int) const; }; X a;\n";
        let mut forms: Vec<(String, String)> = vec![
            (
                "`!=` from a non-member `==`".into(),
                "struct X { int i; }; bool operator==(X, X); X a, b;\nbool v = a != b;\n".into(),
            ),
            (
                "`!=` from a member `==`".into(),
                "struct X { bool operator==(const X &) const; }; X a, b;\nbool v = a != b;\n"
                    .into(),
            ),
            (
                "reversed `==`".into(),
                "struct X { bool operator==(int) const; }; X a;\nbool v = 1 == a;\n".into(),
            ),
            (
                "reversed `!=`".into(),
                "struct X { bool operator==(int) const; }; X a;\nbool v = 1 != a;\n".into(),
            ),
            (
                "`!=` from a defaulted `==`".into(),
                "struct D { int i; bool operator==(const D &) const = default; }; D a, b;\n\
                 bool v = a != b;\n"
                    .into(),
            ),
        ];
        for (which, decl) in [("non-member", non_member), ("member", member)] {
            for op in ["<", ">", "<=", ">=", "<=>"] {
                if op != "<=>" {
                    forms.push((
                        format!("`{op}` from a {which} `<=>`"),
                        format!("{R}{decl}auto v = a {op} 1;\n"),
                    ));
                }
                forms.push((
                    format!("reversed `{op}` from a {which} `<=>`"),
                    format!("{R}{decl}auto v = 1 {op} a;\n"),
                ));
            }
        }
        // A defaulted secondary comparison is defined by a rewrite of
        // `<=>` ([class.compare.secondary]); its body is built when used.
        let defaulted = "struct F { int i; R operator<=>(const F &) const;\n\
                         bool operator<(const F &) const = default; }; F a, b;\n\
                         bool v = a < b;\n";
        forms.push(("a defaulted `<` (its body)".into(), format!("{R}{defaulted}")));
        let mut none = Vec::new();
        for (form, src) in &forms {
            let files = [("main.cpp", src.as_str())];
            let found = with_walk(&files, "c++20", |w| {
                let mut found = 0;
                for e in exprs(w) {
                    if w.ast.exprs.get(e).kind() != ExprKind::CXXRewrittenBinaryOperator {
                        continue;
                    }
                    found += 1;
                    let semantic = reachable(w, w.ast.exprs.rewritten_semantic(e));
                    let (_, written) = w.ast.exprs.rewritten_written(e);
                    let written: Vec<ExprId> = w.ast.lists.iter(written).collect();
                    assert_eq!(written.len(), 2, "{form}: two operands");
                    for o in written {
                        assert!(semantic.contains(&o), "{form}: operand {o:?} not in `a`");
                    }
                }
                found
            });
            if found == 0 {
                none.push(form.as_str());
            }
        }
        // Every form builds the node, the defaulted `<`'s body too.
        assert!(none.is_empty(), "no node for: {none:?}");
    }

    /// Rewritten comparisons nested 200 and 2 000 deep (`a < a < …`, each
    /// `<` from `<=>`, its result compared again) are written in bytes
    /// linear in the depth, with no cause: each node writes its operands
    /// once, in the semantic form.
    #[test]
    fn nested_rewritten_comparisons_are_linear() {
        let chain = |n: usize| {
            format!(
                "struct R {{ int r; }};\nstruct X {{ int i; }};\nX operator<(R, int);\n\
                 R operator<=>(X, X);\nX a;\nX v = a{};\n",
                " < a".repeat(n)
            )
        };
        let (short, c1) = walk_last(chain(200), ExprKind::CXXRewrittenBinaryOperator);
        let (long, c2) = walk_last(chain(2_000), ExprKind::CXXRewrittenBinaryOperator);
        assert_eq!((c1, c2), (None, None));
        let per = short / 200;
        assert!(long >= 9 * short && long <= 10 * short + per, "200: {short} bytes; 2 000: {long}");
    }

    /// A GNU range designator `[0 ... N - 1] = x` writes `x` once: the
    /// list is one run, and only the run's length depends on `N`.
    #[test]
    fn a_range_is_one_run() {
        let range = |n: usize| format!("int v[{n}] = {{[0 ... {}] = 7}};\n", n - 1);
        let (small, c1) = walk_last(range(1_000), ExprKind::InitListExpr);
        let (big, c2) = walk_last(range(1_000_000), ExprKind::InitListExpr);
        assert_eq!((c1, c2), (None, None));
        // The length's `uv` grows from 2 to 3 bytes.
        assert!(big - small <= 2, "1 000: {small} bytes; 1 000 000: {big}");
    }

    /// Nested ranges: Kiln builds one sub-list per slot of the outer
    /// range (`kiln_sema` init.rs, `descend` per slot), each one run of
    /// the shared node; so bytes grow with the outer length only, and the
    /// inner length costs nothing.
    #[test]
    fn nested_ranges_grow_with_the_outer_length_only() {
        let nested = |outer: usize, inner: usize| {
            format!(
                "int v[{outer}][{inner}] = {{[0 ... {}][0 ... {}] = 7}};\n",
                outer - 1,
                inner - 1
            )
        };
        let walk = |o, i| {
            let (b, c) = walk_last(nested(o, i), ExprKind::InitListExpr);
            assert_eq!(c, None);
            b
        };
        let (a, b, c) = (walk(10, 10), walk(10, 1_000), walk(20, 10));
        eprintln!("10x10: {a}; 10x1000: {b}; 20x10: {c}");
        assert!(b - a <= 2 * 10 + 2, "the inner length costs its `uv`: {a} against {b}");
        assert!(c > a, "another outer slot is another sub-list: {a} against {c}");
    }

    /// A designated list in a template keeps its written form
    /// (`ColdExprData` `syntactic_list`, written by `expr::cold`). Kiln
    /// records one for every level of a nested list; since Kiln WP-2478
    /// an inner written list is the very node of the inner level's own
    /// written form (`docs/wp/WP-049.md` Handback, "For Kiln"), so `n`
    /// levels are `n + 1` semantic lists and `n + 1` written ones, and the
    /// bytes grow linearly in the nesting (they were quadratic: every
    /// level copied all those below it).
    #[test]
    fn nested_designated_lists() {
        let nested = |n: usize| {
            let mut lines = vec!["struct N0 { int x; };".to_owned()];
            lines.extend((1..=n).map(|i| format!("struct N{i} {{ N{} a; }};", i - 1)));
            let mut e = String::from("{.x = 1}");
            for _ in 1..=n {
                e = format!("{{.a = {e}}}");
            }
            lines.push(format!("template <class T> struct A {{ static inline N{n} w = {e}; }};"));
            lines.join("\n")
        };
        let mut sizes = Vec::new();
        for n in [1, 2, 3, 4, 8, 12] {
            let src = nested(n);
            let files = [("main.cpp", src.as_str())];
            let (lists, bytes) = with_walk(&files, "c++20", |w| {
                let all = exprs(w);
                let lists = all
                    .iter()
                    .filter(|&&e| w.ast.exprs.get(e).kind() == ExprKind::InitListExpr)
                    .count();
                let root = all
                    .into_iter()
                    .filter(|&e| w.ast.exprs.syntactic_list(e).is_some())
                    .max()
                    .expect("a list with a written form");
                let mut s = Stream::new();
                child(w, &mut s, root);
                (lists, s.as_bytes().len())
            });
            println!("{n} nested: {lists} lists; outermost {bytes} bytes");
            assert_eq!(lists, 2 * (n + 1), "{n} nested");
            sizes.push(bytes);
        }
        // Per level, from 8 to 12 levels against from 1 to 4: linear.
        let (low, high) = ((sizes[3] - sizes[0]) / 3, (sizes[5] - sizes[4]) / 4);
        assert!(high <= 2 * low, "bytes per level {low} (1-4) against {high} (8-12): {sizes:?}");
    }

    /// Scans the sources of every pair test for a node the walk reaches
    /// twice through a kind of this file: a node with two parents (the
    /// rows of this file's kinds, Kiln's children for the others; an
    /// opaque value excepted) one of which is ours, and an initialiser
    /// list repeating a node other than in a run. A tool: `--ignored
    /// --nocapture`.
    #[test]
    #[ignore = "a tool: --ignored --nocapture"]
    #[allow(clippy::too_many_lines, reason = "one scan")]
    fn shared_nodes() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
        let mut dirs = Vec::new();
        let mut todo = vec![root.clone()];
        while let Some(d) = todo.pop() {
            for entry in std::fs::read_dir(&d).expect("a directory").flatten() {
                let p = entry.path();
                if p.is_dir() {
                    todo.push(p);
                } else if p.file_name().is_some_and(|n| n == "t.cpp" || n == "a.cpp") {
                    dirs.push(d.strip_prefix(&root).expect("under tests").to_owned());
                }
            }
        }
        dirs.sort();
        let ours = |k: ExprKind| KINDS.iter().any(|&(x, _)| x == k);
        for dir in dirs {
            let dir = dir.to_string_lossy().into_owned();
            let full = root.join(&dir);
            let read = |n: &str| std::fs::read_to_string(full.join(n)).ok();
            for b in [false, true] {
                let side = if b { "#define B 1\n" } else { "" };
                let Some(main) = read("t.cpp").or_else(|| read(if b { "b.cpp" } else { "a.cpp" }))
                else {
                    continue;
                };
                let std = main
                    .lines()
                    .take(4)
                    .find_map(|l| l.strip_prefix("// std:"))
                    .map_or("c++20", str::trim)
                    .to_owned();
                let files = [("main.cpp", main.as_str()), ("side.h", side)];
                let found = slip_kiln::snippet::parse_with_tables(
                    &slip_kiln::snippet::Snippet {
                        files: &files,
                        main: "main.cpp",
                        include: &[""],
                        std: &std,
                    },
                    &slip_kiln::snippet::Extra::default(),
                    |p| {
                        let path = |_| Vec::new();
                        let input = Input { ast: p.ast, sm: p.sm, subject_path: &path };
                        let rules = RuleTable::empty();
                        let w = Walk::new(&input, &rules);
                        let mut parents: std::collections::HashMap<
                            ExprId,
                            Vec<(ExprKind, ExprId)>,
                        > = std::collections::HashMap::new();
                        let mut found = Vec::new();
                        for e in exprs(&w) {
                            let k = w.ast.exprs.get(e).kind();
                            let kids: Vec<ExprId> = if ours(k) {
                                let n = node(&w, e);
                                let mut out = Vec::new();
                                let mut s = Stream::new();
                                for &(x, f) in KINDS {
                                    if x == k {
                                        f(&w, &mut s, e, &n, &mut out);
                                    }
                                }
                                out.iter()
                                    .filter_map(
                                        |r| if let R::Child(c) = *r { Some(c) } else { None },
                                    )
                                    .collect()
                            } else {
                                w.ast.exprs.children(e, &w.ast.lists)
                            };
                            if k == ExprKind::InitListExpr {
                                let mut runs: Vec<ExprId> = kids.clone();
                                runs.dedup();
                                let mut set = HashSet::new();
                                if runs.iter().any(|x| !set.insert(*x)) {
                                    found.push(format!("list {e:?} repeats a node apart"));
                                }
                            }
                            let mut kids = kids;
                            kids.dedup();
                            for c in kids {
                                parents.entry(c).or_default().push((k, e));
                            }
                        }
                        for (c, ps) in parents {
                            let kind = w.ast.exprs.get(c).kind();
                            if ps.len() > 1
                                && ps.iter().any(|&(k, _)| ours(k))
                                && kind != ExprKind::OpaqueValueExpr
                            {
                                found.push(format!("{kind:?} {c:?} under {ps:?}"));
                            }
                        }
                        found
                    },
                );
                if let Ok(found) = found {
                    for f in found {
                        println!("{dir} {}: {f}", if b { "B" } else { "A" });
                    }
                }
            }
        }
    }

    /// Prints the renderings of every node of this file's kinds in the
    /// pair test `$SLIP_ROWS_DIR` (relative to `tests/`): a tool for
    /// writing [`check`]s.
    #[test]
    #[ignore = "a tool: SLIP_ROWS_DIR=<root>/<Short>/<row> --ignored --nocapture"]
    fn print_rows() {
        let Some(dir) = std::env::var_os("SLIP_ROWS_DIR") else { return };
        let dir = dir.to_string_lossy().into_owned();
        for dir in dir.split(',') {
            println!("== {dir}");
            let mut seen = Vec::new();
            for &(k, _) in KINDS {
                if seen.contains(&k) {
                    continue;
                }
                seen.push(k);
                let [a, b] = sides(dir, k);
                for r in a {
                    println!("A {k:?}: {r}");
                }
                for r in b {
                    println!("B {k:?}: {r}");
                }
            }
        }
    }
}
