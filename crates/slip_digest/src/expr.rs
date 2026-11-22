//! Expressions (`spec/digest-kinds.md` §7): the walk, the common rows of
//! `ExprNode`, the rows of `ColdExprData`, and the dispatch over every
//! `ExprKind` (WP-031). The core kinds are `expr_core.rs`; the other kinds
//! are functions of `expr_init.rs`, `expr_dep.rs` and `expr_misc.rs`.
//!
//! # The walk
//!
//! `child` writes one expression and everything under it without Rust
//! recursion for the kinds it writes itself: a node is turned into a list
//! of [`Item`]s (the primitives of its rows, in table order, a
//! [`Item::Node`] for each child), pushed in reverse on one heap stack and
//! executed from the top. A child is expanded when it is popped, so the
//! stream is written in exactly the order a recursive walk would write it
//! (pre-order, rows in table order), and an operator chain, a call chain
//! or a cast chain of any length uses one Rust frame. A kind written by
//! another file is an [`Item::Exec`] of its function, which writes its own
//! rows and reaches its children through [`child`]: that call is the only
//! recursion, and it enters one level of `walk::NODE_LIMIT`.
//!
//! # The rules at the head of §7
//!
//! - Conditional rows are written by the kind's own code, from a fact an
//!   earlier row wrote.
//! - Parentheses: [`child`] skips every `ParenExpr` and writes its operand
//!   in its place; [`child_keep_paren`] writes an outermost `ParenExpr`
//!   with its rows, for the one `DependentDecltype` profile that needs it
//!   (§4.2 rule 7), and skips the parentheses inside it.
//! - Shared nodes: the default argument behind a `CXXDefaultArgExpr` and
//!   the initialiser behind a `CXXDefaultInitExpr` are not children; the
//!   walk never reaches them, the kinds' own functions write what the use
//!   writes.
//! - Opaque values: [`Item::Opaque`] numbers sources per unit
//!   (`walk::UnitState::opaques`) in the order the walk first reaches them.
//! - Overloaded member access: `MemberExpr.b` is the first candidate, and
//!   is written like any member.
//! - Literals from builtin macros: written by value, like any literal.
//!
//! # What a kind's function may use
//!
//! [`node`] reads a node's words, [`expr_id`], [`expr_list`] and
//! [`decl_id`] turn raw words into ids (an absent or bad word is an
//! `Unaccounted` cause, never a panic), [`children`] writes `child*`,
//! [`check_bits`] and [`check_aux`] make a set bit no row accounts for
//! `Unaccounted`. `lib.rs` "How to fill an expression kind" says the rest.

use slip_kiln::kiln::ast::expr::{ExprDeps, ExprKind, ObjectKind, ValueCategory};
use slip_kiln::kiln::ast::{
    ArgListId, ConstValueId, DeclId, ExprId, ListId, QualType, RawId, StringLitId,
};

use crate::Cause;
use crate::stream::{FlagWord, Stream};
use crate::tables::{Row, expr_kind, fp_round, object_kind, value_category};
use crate::walk::{TOO_DEEP, Walk};

/// The function that writes the rows of one kind (`expr_init.rs`,
/// `expr_dep.rs`, `expr_misc.rs`): the rows of that kind only, in table
/// order; `child` wrote the common rows before it and writes the cold
/// rows after it.
pub(crate) type KindFn = fn(&mut Walk<'_>, &mut Stream, ExprId);

/// One step of the walk: a primitive of a row, or a node to expand.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Item {
    /// The node, its rows and the rows of its maps; `true`: an outermost
    /// `ParenExpr` is written, not skipped.
    Node(ExprId, bool),
    Uv(u64),
    Bool(bool),
    Tag(Row),
    Flags(FlagWord),
    Count(usize),
    Opt(bool),
    Ref(DeclId),
    RefOpt(Option<DeclId>),
    Type(QualType),
    TypeOpt(Option<QualType>),
    /// `type*` of a cast's base path: the base class of each `BaseSpec`.
    BasePath(Option<ListId>),
    Value(ConstValueId),
    String(StringLitId),
    Args(ArgListId),
    /// `decl` (§2.4).
    Decl(DeclId),
    /// The stream is indeterminate.
    Cause(Cause),
    /// `OpaqueValueExpr.a`: the number of the source, and the source the
    /// first time it is reached.
    Opaque(ExprId),
    /// A kind's function, or a row written by another file's function.
    Exec(KindFn, ExprId),
}

/// The words of a node, read once.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Node {
    pub kind: ExprKind,
    /// Bits 4 to 7 of `ExprNode.bits`, the per-kind flags, in place.
    pub flags: u8,
    pub aux: u8,
    pub a: u32,
    pub b: u32,
    pub ty: QualType,
}

/// Reads the words of `e`.
pub(crate) fn node(w: &Walk<'_>, e: ExprId) -> Node {
    let n = w.ast.exprs.get(e);
    Node { kind: n.kind(), flags: n.raw_bits() & 0xF0, aux: n.aux(), a: n.a(), b: n.b(), ty: n.ty() }
}

/// The expression a word names; `None` (and `cause` on the stream) when
/// the word is 0.
#[allow(dead_code, reason = "for the kinds of expr_init.rs, expr_dep.rs and expr_misc.rs")]
pub(crate) fn expr_id(s: &mut Stream, raw: u32, cause: Cause) -> Option<ExprId> {
    let e = ExprId::try_new(raw);
    if e.is_none() {
        s.indeterminate(cause);
    }
    e
}

/// The declaration a word names; `None` (and `cause`) when it is 0.
#[allow(dead_code, reason = "for the kinds of expr_init.rs, expr_dep.rs and expr_misc.rs")]
pub(crate) fn decl_id(s: &mut Stream, raw: u32, cause: Cause) -> Option<DeclId> {
    let d = DeclId::try_new(raw);
    if d.is_none() {
        s.indeterminate(cause);
    }
    d
}

/// The expressions of the `ListId` in `raw`, in stored order; the
/// elements that are 0, or a list word of 0, are left out with `cause`.
pub(crate) fn expr_list(w: &Walk<'_>, s: &mut Stream, raw: u32, cause: Cause) -> Vec<ExprId> {
    let Some(l) = ListId::try_new(raw) else {
        s.indeterminate(cause);
        return Vec::new();
    };
    let words = w.ast.lists.get(l);
    let mut out = Vec::with_capacity(words.len());
    for &word in words {
        match ExprId::from_raw(word) {
            Some(e) => out.push(e),
            None => {
                s.indeterminate(cause);
            }
        }
    }
    out
}

/// `child*`, in stored order.
#[allow(dead_code, reason = "for the kinds of expr_init.rs, expr_dep.rs and expr_misc.rs")]
pub(crate) fn children(w: &mut Walk<'_>, s: &mut Stream, es: &[ExprId]) {
    s.count(es.len());
    for &e in es {
        child(w, s, e);
    }
}

/// `cause` when a per-kind flag outside `known` (bits 4 to 7, in place)
/// is set: the bits the kind's rows account for, `out` rows included.
pub(crate) fn check_bits(s: &mut Stream, n: &Node, known: u8, cause: Cause) {
    if n.flags & !known != 0 {
        s.indeterminate(cause);
    }
}

/// `cause` when a bit of `aux` outside `known` is set.
pub(crate) fn check_aux(s: &mut Stream, n: &Node, known: u8, cause: Cause) {
    if n.aux & !known != 0 {
        s.indeterminate(cause);
    }
}

/// `child`: the expression `e`, walked here (§7).
pub(crate) fn child(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    walk(w, s, e, false);
}

/// `child`, writing an outermost `ParenExpr` with its rows: the whole
/// operand of a `DependentDecltype` profile (§7 "Parentheses", §4.2
/// rule 7), for `expr_dep::profile`.
#[allow(dead_code, reason = "for the profile package (expr_dep.rs)")]
pub(crate) fn child_keep_paren(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    walk(w, s, e, true);
}

/// `child?`.
pub(crate) fn child_opt(w: &mut Walk<'_>, s: &mut Stream, e: Option<ExprId>) {
    s.opt(e.is_some());
    if let Some(e) = e {
        child(w, s, e);
    }
}

/// A requires-clause (a template head's, a function's trailing one), as
/// the `child` its row names.
pub(crate) fn requires_clause(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    child(w, s, e);
}

/// The walk of one expression ("The walk" above).
fn walk(w: &mut Walk<'_>, s: &mut Stream, root: ExprId, keep_paren: bool) {
    if !w.enter_node() {
        s.uv(0).indeterminate(TOO_DEEP);
        return;
    }
    let mut stack: Vec<Item> = vec![Item::Node(root, keep_paren)];
    let mut items: Vec<Item> = Vec::new();
    while let Some(item) = stack.pop() {
        match item {
            Item::Node(e, keep) => {
                write_node(w, s, e, keep, &mut items);
                stack.extend(items.drain(..).rev());
            }
            Item::Opaque(e) => {
                opaque(w, s, e, &mut stack);
            }
            Item::Uv(v) => {
                s.uv(v);
            }
            Item::Bool(v) => {
                s.bool(v);
            }
            Item::Tag(r) => {
                s.tag(r);
            }
            Item::Flags(f) => {
                s.flags(f);
            }
            Item::Count(n) => {
                s.count(n);
            }
            Item::Opt(p) => {
                s.opt(p);
            }
            Item::Ref(d) => w.write_ref(s, d),
            Item::RefOpt(d) => w.write_ref_opt(s, d),
            Item::Type(q) => w.write_type(s, q),
            Item::TypeOpt(q) => w.write_type_opt(s, q),
            Item::BasePath(l) => base_path(w, s, l),
            Item::Value(v) => crate::value::value(w, s, v),
            Item::String(l) => crate::value::string(w, s, l),
            Item::Args(l) => w.write_args(s, l),
            Item::Decl(d) => crate::local::write_decl(w, s, d),
            Item::Cause(c) => {
                s.indeterminate(c);
            }
            Item::Exec(f, e) => f(w, s, e),
        }
    }
    w.leave_node();
}

/// `type*` of a base path (`ImplicitCastExpr.b`): the base class type of
/// each `BaseSpec` step, in order.
fn base_path(w: &mut Walk<'_>, s: &mut Stream, l: Option<ListId>) {
    let ast = w.ast;
    let steps: Vec<QualType> = match l {
        Some(l) => ast.lists.get(l).iter().map(|&i| ast.decls.base_spec(i).ty).collect(),
        None => Vec::new(),
    };
    w.write_types(s, &steps);
}

/// `OpaqueValueExpr.a` (§7 "Opaque values"): `uv` the number of the
/// source, then `child?`: the source the first time the walk reaches it,
/// absent after; a node without a source takes the next number.
fn opaque(w: &mut Walk<'_>, s: &mut Stream, e: ExprId, stack: &mut Vec<Item>) {
    let source = ExprId::try_new(w.ast.exprs.get(e).a());
    let unit = &mut w.unit;
    let (n, first) = match source.and_then(|src| unit.opaques.get(&src).copied()) {
        Some(n) => (n, false),
        None => {
            let n = unit.next_opaque;
            unit.next_opaque += 1;
            if let Some(src) = source {
                unit.opaques.insert(src, n);
            }
            (n, true)
        }
    };
    s.uv(n);
    match source {
        Some(src) if first => {
            s.opt(true);
            stack.push(Item::Node(src, false));
        }
        Some(_) | None => {
            s.opt(false);
        }
    }
}

/// Which `ColdExprData` rows a kind can have an entry in, where a row
/// depends on the kind (§7.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tail {
    /// A `MemberExpr`: `member_extra`.
    Member,
    /// An explicit cast: `cast_written` is `cast_written-cast`.
    ExplicitCast,
    /// A `CXXUnresolvedConstructExpr`: `cast_written` is
    /// `cast_written-unresolved-construct`.
    UnresolvedConstruct,
    Other,
}

/// Writes the common rows of `e` now and puts the rest of its rows in
/// `out`: its kind's, then `ColdExprData`'s, then `templ::expr_maps`.
fn write_node(w: &mut Walk<'_>, s: &mut Stream, e: ExprId, keep_paren: bool, out: &mut Vec<Item>) {
    crate::walk::stack_probe();
    let Some(e) = strip_parens(w, s, e, keep_paren) else {
        return;
    };
    let ast = w.ast;
    let x = ast.exprs.get(e);
    let n = node(w, e);
    let row = expr_kind::row(&n.kind);
    s.tag(row);
    let bits = x.raw_bits();
    if bits & 3 == 3 {
        s.indeterminate(Cause::Unaccounted("ExprNode/bits-value-category"));
    }
    s.tag(value_category_row(x.value_category()));
    if (bits >> 2) & 3 == 3 {
        s.indeterminate(Cause::Unaccounted("ExprNode/bits-object-kind"));
    }
    s.tag(object_kind_row(x.object_kind()));
    deps(s, x.deps());
    w.write_type(s, n.ty);
    let tail = dispatch(w, s, e, &n, out);
    cold(w, e, &n, tail, out);
    out.push(Item::Exec(crate::templ::expr_maps, e));
}

/// `e` with its parentheses skipped (§7 "Parentheses"); with `keep`, an
/// outermost `ParenExpr` is kept. `None` when a `ParenExpr` has no
/// operand (a placeholder is written).
fn strip_parens(w: &Walk<'_>, s: &mut Stream, e: ExprId, keep: bool) -> Option<ExprId> {
    let mut e = e;
    if keep {
        return Some(e);
    }
    loop {
        let x = w.ast.exprs.get(e);
        if x.kind() != ExprKind::ParenExpr {
            return Some(e);
        }
        if cold_entry(w, e) {
            // A map row of a skipped node would be lost.
            s.indeterminate(Cause::Unaccounted("ExprNode/ParenExpr"));
        }
        match ExprId::try_new(x.a()) {
            Some(a) => e = a,
            None => {
                s.uv(0).indeterminate(Cause::Unaccounted("ExprNode/ParenExpr.a"));
                return None;
            }
        }
    }
}

/// Whether `e` has an entry in a map whose row is `in`.
fn cold_entry(w: &Walk<'_>, e: ExprId) -> bool {
    let c = &w.ast.exprs.cold;
    c.member_extra.contains(e)
        || c.designators.contains(e)
        || c.syntactic_list.contains(e)
        || c.array_trait.contains(e)
        || c.union_field.contains(e)
        || c.cast_written.contains(e)
        || c.offsetof_pattern.contains(e)
        || c.offsetof_layout.contains(e)
        || c.splice_args.contains(e)
        || c.default_arg_rewritten.contains(e)
        || c.fp_round.contains(e)
}

fn value_category_row(v: ValueCategory) -> Row {
    value_category::row(&v)
}

fn object_kind_row(v: ObjectKind) -> Row {
    object_kind::row(&v)
}

/// `record ExprDeps` (§7.1): its four `in` bits; the error bit makes the
/// unit indeterminate (digest §10).
fn deps(s: &mut Stream, d: ExprDeps) {
    s.flags(
        FlagWord::new()
            .bit(d.contains(ExprDeps::TYPE))
            .bit(d.contains(ExprDeps::VALUE))
            .bit(d.contains(ExprDeps::INSTANTIATION))
            .bit(d.contains(ExprDeps::UNEXPANDED_PACK)),
    );
    if d.contains(ExprDeps::ERROR) {
        s.indeterminate(Cause::Error);
    }
    if d.bits() & !0x1F != 0 {
        s.indeterminate(Cause::Unaccounted("ExprDeps/0"));
    }
}

/// The rows of `e`'s kind, into `out`; which cold rows depend on the
/// kind. Every variant is listed (digest §13 obligation 2).
#[allow(clippy::too_many_lines, reason = "one arm per ExprKind variant")]
fn dispatch(w: &mut Walk<'_>, s: &mut Stream, e: ExprId, n: &Node, out: &mut Vec<Item>) -> Tail {
    use crate::{expr_core as core, expr_dep as dep, expr_init as init, expr_misc as misc};
    let mut exec = |f: KindFn| out.push(Item::Exec(f, e));
    match n.kind {
        // The core kinds (expr_core.rs).
        ExprKind::IntegerLiteral => core::integer_literal(w, s, n, out),
        ExprKind::FloatingLiteral => core::floating_literal(s, n, out),
        ExprKind::CharacterLiteral => core::character_literal(s, n, out),
        ExprKind::StringLiteral => core::string_literal(s, n, out),
        ExprKind::CXXBoolLiteralExpr => core::bool_literal(s, n, out),
        ExprKind::CXXNullPtrLiteralExpr => {
            check_bits(s, n, 0, Cause::Unaccounted("ExprNode/CXXNullPtrLiteralExpr.bits"));
        }
        ExprKind::ImaginaryLiteral => core::imaginary_literal(s, n, out),
        ExprKind::GNUNullExpr => {
            check_bits(s, n, 0, Cause::Unaccounted("ExprNode/GNUNullExpr.bits"));
        }
        ExprKind::DeclRefExpr => core::decl_ref(w, s, e, n, out),
        ExprKind::MemberExpr => {
            core::member(s, n, out);
            return Tail::Member;
        }
        ExprKind::CXXThisExpr => core::this(s, n, out),
        ExprKind::UnaryOperator => core::unary(s, n, out),
        ExprKind::BinaryOperator => core::binary(s, n, out),
        ExprKind::CompoundAssignOperator => core::compound_assign(w, s, e, n, out),
        ExprKind::ConditionalOperator => core::conditional(w, s, e, n, out),
        ExprKind::BinaryConditionalOperator => core::binary_conditional(w, s, n, out),
        ExprKind::ArraySubscriptExpr => core::array_subscript(s, n, out),
        ExprKind::ParenExpr => core::paren(s, n, out),
        ExprKind::CallExpr => core::call(w, s, n, out),
        ExprKind::CXXMemberCallExpr => core::member_call(w, s, n, out),
        ExprKind::CXXOperatorCallExpr => core::operator_call(w, s, n, out),
        ExprKind::ImplicitCastExpr => core::cast(s, n, true, out),
        ExprKind::CStyleCastExpr
        | ExprKind::CXXFunctionalCastExpr
        | ExprKind::CXXStaticCastExpr
        | ExprKind::CXXConstCastExpr
        | ExprKind::CXXReinterpretCastExpr
        | ExprKind::CXXDynamicCastExpr
        | ExprKind::BuiltinBitCastExpr => {
            core::cast(s, n, false, out);
            return Tail::ExplicitCast;
        }
        ExprKind::MaterializeTemporaryExpr => core::materialize(s, n, out),
        ExprKind::CXXBindTemporaryExpr => core::bind_temporary(s, n, out),
        ExprKind::ExprWithCleanups => core::cleanups(s, n, out),
        ExprKind::OpaqueValueExpr => core::opaque_value(s, e, n, out),
        ExprKind::ConstantExpr => core::constant(s, n, out),
        // `unsupported` (§7 ExprKind).
        ExprKind::UserDefinedLiteral => {
            s.indeterminate(Cause::Unsupported("ExprKind/UserDefinedLiteral"));
        }
        ExprKind::EmbedExpr => {
            s.indeterminate(Cause::Unsupported("ExprKind/EmbedExpr"));
        }
        ExprKind::UnresolvedMemberExpr => {
            s.indeterminate(Cause::Unsupported("ExprKind/UnresolvedMemberExpr"));
        }
        ExprKind::CXXReflectExpr => {
            s.indeterminate(Cause::Unsupported("ExprKind/CXXReflectExpr"));
        }
        ExprKind::CXXSpliceExpr => {
            s.indeterminate(Cause::Unsupported("ExprKind/CXXSpliceExpr"));
        }
        ExprKind::RecoveryExpr => {
            s.indeterminate(Cause::Error);
        }
        // Construction, initialisation, `new` and `delete` (expr_init.rs).
        ExprKind::InitListExpr => exec(init::init_list_expr),
        ExprKind::ImplicitValueInitExpr => exec(init::implicit_value_init_expr),
        ExprKind::DesignatedInitExpr => exec(init::designated_init_expr),
        ExprKind::CXXConstructExpr => exec(init::cxx_construct_expr),
        ExprKind::CXXTemporaryObjectExpr => exec(init::cxx_temporary_object_expr),
        ExprKind::CXXInheritedCtorInitExpr => exec(init::cxx_inherited_ctor_init_expr),
        ExprKind::CXXDefaultArgExpr => exec(init::cxx_default_arg_expr),
        ExprKind::CXXDefaultInitExpr => exec(init::cxx_default_init_expr),
        ExprKind::CXXScalarValueInitExpr => exec(init::cxx_scalar_value_init_expr),
        ExprKind::CXXParenListInitExpr => exec(init::cxx_paren_list_init_expr),
        ExprKind::ArrayInitLoopExpr => exec(init::array_init_loop_expr),
        ExprKind::ArrayInitIndexExpr => exec(init::array_init_index_expr),
        ExprKind::CXXStdInitializerListExpr => exec(init::cxx_std_initializer_list_expr),
        ExprKind::CompoundLiteralExpr => exec(init::compound_literal_expr),
        ExprKind::NoInitExpr => exec(init::no_init_expr),
        ExprKind::DesignatedInitUpdateExpr => exec(init::designated_init_update_expr),
        ExprKind::CXXNewExpr => exec(init::cxx_new_expr),
        ExprKind::CXXDeleteExpr => exec(init::cxx_delete_expr),
        ExprKind::CXXPseudoDestructorExpr => exec(init::cxx_pseudo_destructor_expr),
        ExprKind::CXXThrowExpr => exec(init::cxx_throw_expr),
        ExprKind::CXXRewrittenBinaryOperator => exec(init::cxx_rewritten_binary_operator),
        ExprKind::ParenListExpr => exec(init::paren_list_expr),
        // Names and constructs of templates (expr_dep.rs).
        ExprKind::UnresolvedLookupExpr => exec(dep::unresolved_lookup_expr),
        ExprKind::DependentScopeDeclRefExpr => exec(dep::dependent_scope_decl_ref_expr),
        ExprKind::CXXDependentScopeMemberExpr => exec(dep::cxx_dependent_scope_member_expr),
        ExprKind::SubstNonTypeTemplateParmExpr => exec(dep::subst_non_type_template_parm_expr),
        ExprKind::SubstNonTypeTemplateParmPackExpr => {
            exec(dep::subst_non_type_template_parm_pack_expr);
        }
        ExprKind::FunctionParmPackExpr => exec(dep::function_parm_pack_expr),
        ExprKind::SizeOfPackExpr => exec(dep::size_of_pack_expr),
        ExprKind::RequiresExpr => exec(dep::requires_expr),
        ExprKind::ConceptSpecializationExpr => exec(dep::concept_specialization_expr),
        ExprKind::LambdaExpr => exec(dep::lambda_expr),
        ExprKind::PackExpansionExpr => exec(dep::pack_expansion_expr),
        ExprKind::CXXFoldExpr => exec(dep::cxx_fold_expr),
        ExprKind::PackIndexingExpr => exec(dep::pack_indexing_expr),
        ExprKind::CXXUnresolvedConstructExpr => {
            exec(dep::cxx_unresolved_construct_expr);
            return Tail::UnresolvedConstruct;
        }
        ExprKind::CXXExpansionSelectExpr => exec(dep::cxx_expansion_select_expr),
        ExprKind::IntegerPackExpr => exec(dep::integer_pack_expr),
        // Every other kind (expr_misc.rs).
        ExprKind::PredefinedExpr => exec(misc::predefined_expr),
        ExprKind::SourceLocExpr => exec(misc::source_loc_expr),
        ExprKind::CXXNoexceptExpr => exec(misc::cxx_noexcept_expr),
        ExprKind::CXXTypeidExpr => exec(misc::cxx_typeid_expr),
        ExprKind::UnaryExprOrTypeTraitExpr => exec(misc::unary_expr_or_type_trait_expr),
        ExprKind::TypeTraitExpr => exec(misc::type_trait_expr),
        ExprKind::ArrayTypeTraitExpr => exec(misc::array_type_trait_expr),
        ExprKind::ExpressionTraitExpr => exec(misc::expression_trait_expr),
        ExprKind::OffsetOfExpr => exec(misc::offset_of_expr),
        ExprKind::CoawaitExpr => exec(misc::coawait_expr),
        ExprKind::DependentCoawaitExpr => exec(misc::dependent_coawait_expr),
        ExprKind::CoyieldExpr => exec(misc::coyield_expr),
        ExprKind::StmtExpr => exec(misc::stmt_expr),
        ExprKind::AddrLabelExpr => exec(misc::addr_label_expr),
        ExprKind::ChooseExpr => exec(misc::choose_expr),
        ExprKind::VAArgExpr => exec(misc::va_arg_expr),
        ExprKind::AtomicExpr => exec(misc::atomic_expr),
        ExprKind::ShuffleVectorExpr => exec(misc::shuffle_vector_expr),
        ExprKind::ConvertVectorExpr => exec(misc::convert_vector_expr),
        ExprKind::ExtVectorElementExpr => exec(misc::ext_vector_element_expr),
        ExprKind::PseudoObjectExpr => exec(misc::pseudo_object_expr),
        ExprKind::GenericSelectionExpr => exec(misc::generic_selection_expr),
    }
    Tail::Other
}

/// The rows of `ColdExprData` (§7.4) for `e`, into `out`: written for
/// every expression, the empty form when the map has no entry.
fn cold(w: &Walk<'_>, e: ExprId, n: &Node, tail: Tail, out: &mut Vec<Item>) {
    let ast = w.ast;
    let c = &ast.exprs.cold;
    // member_extra-qualified, member_extra-record.
    match c.member_extra.get(e) {
        Some(x) => {
            if tail != Tail::Member {
                out.push(Item::Cause(Cause::Unaccounted("ColdExprData/member_extra-record")));
            }
            let &slip_kiln::kiln::ast::expr::DeclRefExtra { qualifier, template_args, found_decl: _ } =
                x;
            out.push(Item::Bool(qualifier.is_some()));
            let member = DeclId::try_new(n.b);
            crate::expr_core::decl_ref_extra(w, member, template_args, out);
        }
        None => {
            out.push(Item::Bool(false));
            let member = if tail == Tail::Member { DeclId::try_new(n.b) } else { None };
            crate::expr_core::decl_ref_extra(w, member, None, out);
        }
    }
    // designators.
    if c.designators.contains(e) {
        out.push(Item::Exec(crate::expr_init::designators, e));
    } else {
        out.push(Item::Count(0));
    }
    // syntactic_list.
    let syntactic = c.syntactic_list.get(e).copied();
    out.push(Item::Opt(syntactic.is_some()));
    if let Some(l) = syntactic {
        out.push(Item::Node(l, false));
    }
    // array_trait.
    let array_trait = c.array_trait.contains(e);
    out.push(Item::Opt(array_trait));
    if array_trait {
        out.push(Item::Exec(crate::expr_misc::array_trait_extra, e));
    }
    // source_loc (symbolic) and default_imm (demand) write nothing.
    // union_field.
    out.push(Item::RefOpt(c.union_field.get(e).copied()));
    // cast_written-unresolved-construct, cast_written-cast.
    let written = c.cast_written.get(e).copied();
    let (unresolved, cast) = match tail {
        Tail::UnresolvedConstruct => (written, None),
        Tail::ExplicitCast => (None, written),
        Tail::Member | Tail::Other => {
            if written.is_some() {
                out.push(Item::Cause(Cause::Unaccounted("ColdExprData/cast_written-cast")));
            }
            (None, None)
        }
    };
    out.push(Item::TypeOpt(unresolved));
    out.push(Item::TypeOpt(cast));
    // offsetof_pattern-type, offsetof_pattern-components.
    match c.offsetof_pattern.get(e) {
        Some((ty, _)) => {
            out.push(Item::TypeOpt(Some(*ty)));
            out.push(Item::Exec(crate::expr_misc::offsetof_components, e));
        }
        None => {
            out.push(Item::TypeOpt(None));
            out.push(Item::Count(0));
        }
    }
    // offsetof_layout.
    if c.offsetof_layout.contains(e) {
        out.push(Item::Exec(crate::expr_misc::offsetof_layout, e));
    } else {
        out.push(Item::Count(0));
    }
    // splice_args: unsupported.
    if c.splice_args.contains(e) {
        out.push(Item::Cause(Cause::Unsupported("ColdExprData/splice_args")));
    }
    // default_arg_rewritten.
    let rewritten = c.default_arg_rewritten.get(e).copied();
    out.push(Item::Opt(rewritten.is_some()));
    if let Some(r) = rewritten {
        out.push(Item::Node(r, false));
    }
    // fp_round.
    match c.fp_round.get(e) {
        Some(r) => {
            out.push(Item::Opt(true));
            out.push(Item::Tag(fp_round::row(r)));
        }
        None => out.push(Item::Opt(false)),
    }
}
