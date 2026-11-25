//! Operand units (`spec/digest.md` §2.4, `spec/digest-kinds.md` §11.11,
//! WP-156; the design is `spec/landed/digest/WP-153.md` §3.7 as ruled in
//! its §13.2).
//!
//! The digest writes a type by its canonical form and a constant by its
//! value, so a name inside an expression that only such a type or value
//! holds (`decltype(make(x))`, `arr<char, sizeof(make(x))>`) is written
//! nowhere. The operand unit of a unit writes those expressions: key kind
//! `operands` (35), parent the unit's key, name `operands` (17), no
//! discriminator; derived, in the derived group of the parent's file.
//!
//! # What is collected
//!
//! The **holders** of a written type or argument list (B4): a
//! declaration's written type (return, each parameter before adjustment,
//! variable, field, typedef and alias, base specifier, friend), a default
//! type template argument, the argument list as written of an explicit or
//! partial specialisation and of an explicit instantiation; in an
//! expression, the template arguments and qualifier of a reference to a
//! declaration, of a member expression and of a concept-id, and the
//! written type of a cast, `sizeof`, `alignof`, `typeid`, a type trait,
//! `new`, a compound literal; a nested-name-specifier that names a type.
//! Inside a holder's written type ([`scan`]) the **operands** are: site 1
//! `decltype(e)`, site 2 `typeof(e)`, site 4 an expression template
//! argument, site 6 `noexcept(e)` of a function type, site 8 the argument
//! list as written of an alias specialisation. The walk of a written type
//! stops at what a name denotes (a typedef's or alias's underlying type, a
//! substituted parameter's replacement, a deduced type, a class): only
//! what is written is read, never the type of an expression (S3).
//!
//! # Groups
//!
//! The operand unit of an entity's unit reads every declaration of the
//! entity in declaration order (B2): each declaration's signature
//! ([`signature`]) is a group; the declaration the unit's stream is read
//! from also holds, after its signature, what the stream's walk met in
//! expressions and in the declarations it writes in place (locals, the
//! members of a local class, a lambda's parameters), in walk order
//! ([`expr_node`], [`decl`]). A part (a resolved or `default_argument`
//! unit) has one group: what its stream met. A group with no written
//! operand is not written; a unit with none has no operand unit.
//!
//! # An entry
//!
//! An operand is written in the parent's state (S6), after the parent's
//! stream: `uv` its site, then the expression walk (`expr::child`), or for
//! site 8 the canonical form of each argument (`Walk::write_args`). An
//! operand of sites 1, 2, 4 and 6 is written only if its walk writes a
//! reference to an entity (S5: `Walk::op_refs` moved) or is
//! indeterminate; site 8 always (B5). An instantiation-dependent operand
//! is never written (a pattern's own spellings hold it). A holder met
//! while an entry is walked gives its operands as entries of their own,
//! right after that entry (S7).

use std::collections::{HashMap, HashSet, VecDeque};

use slip_kiln::kiln::ast::decl::{DeclKind, FriendTarget, VarPlaceholder};
use slip_kiln::kiln::ast::expr::{ExprDeps, ExprFlags, ExprKind as K, TypeidOperand};
use slip_kiln::kiln::ast::types::{ArgView, ExcSpec, Nns, TypeOfOperand, TypeView};
use slip_kiln::kiln::ast::{ArgListId, DeclId, ExprId, NnsId, QualType};
use slip_model::Digest;
use slip_model::digests::GroupClass;
use slip_model::keys::{Key, KeyKind, Name, Parent};

use crate::Cause;
use crate::stream::{Hashed, Stream, UNIT_TAG};
use crate::units::{Written, merge};
use crate::walk::Walk;

/// The sites (digest-kinds §11.11). 3 (array bound), 5 (pack index) and 7
/// (`explicit(e)`) are reserved.
pub(crate) const DECLTYPE: u8 = 1;
pub(crate) const TYPEOF: u8 = 2;
pub(crate) const EXPR_ARG: u8 = 4;
pub(crate) const NOEXCEPT: u8 = 6;
pub(crate) const ALIAS_ARGS: u8 = 8;

/// How deep a written type is followed (a written type is never this
/// deep; the bound keeps a malformed one finite).
const MAX_DEPTH: u32 = 512;

/// How many entries one operand unit writes at most: past it the unit is
/// indeterminate (never met: an entry's nested entries are its own
/// subexpressions' holders).
const MAX_ENTRIES: usize = 1 << 20;

/// The cause of an operand unit past [`MAX_ENTRIES`] or a written type past
/// [`MAX_DEPTH`].
const TOO_MANY: Cause = Cause::Unaccounted("operands/limit");

/// One operand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Op {
    /// Sites 1, 2, 4, 6: the expression.
    Expr(u8, ExprId),
    /// Site 8: the argument list as written of an alias specialisation.
    Alias(ArgListId),
}

/// The operand state of the unit being written (`UnitState::operands`).
#[derive(Debug, Default)]
pub(crate) struct State {
    /// The unit has an operand unit to collect ([`begin`]).
    on: bool,
    /// The first declarations whose declarations the groups read (the
    /// entity's, and its templates' patterns'): [`decl`] leaves them out.
    covered: HashSet<DeclId>,
    /// The groups, in declaration order.
    groups: Vec<Vec<Op>>,
    /// The group the stream's own operands join.
    own: usize,
    /// While an entry is walked: the operands it holds (S7).
    nested: Option<Vec<Op>>,
    /// Above 0 while an entry is walked (`Walk::write_ref`).
    pub(crate) writing: u32,
    /// Collected for the numbering of an owner (`local::compute`), whose
    /// streams are no unit's: the entries are walked so that the local
    /// types and objects in them are numbered ([`number`]).
    numbering: bool,
}

/// The counters of `spec/digest.md` §12 for operand units (WP-156 item 9).
#[derive(Debug, Default)]
pub(crate) struct Counts {
    pub units: u64,
    /// Written operands by site (index: the site).
    pub sites: [u64; 9],
    /// Operands of sites 1, 2, 4, 6 whose walk wrote no reference (S5).
    pub unreferenced: u64,
    /// Instantiation-dependent operands, not written.
    pub dependent: u64,
    /// Declarations whose type a placeholder deduced: not read (S3).
    pub deduced: u64,
}

/// Whether the walk collects operands now: the unit has an operand unit
/// and what is written goes into its own stream (while an owner is
/// numbered, into the owner's stream).
fn collecting(w: &Walk<'_>) -> bool {
    let st = &w.unit.operands;
    st.on
        && if st.numbering {
            w.unit_level()
                && w.unit.expr.profile.is_none()
                && !w.unit.local.detached
                && w.dropped == 0
        } else {
            w.in_own_stream()
        }
}

/// Whether the counters count now: not while an owner is numbered.
fn counting(w: &Walk<'_>) -> bool {
    !crate::local::numbering_mode(w)
}

/// Starts the operand unit of the unit of entity `e` being written (its
/// state fresh): `from`, the declaration an entity's or content-keyed
/// unit's stream is read from, gives the groups of every declaration of
/// `e` ([`signature`]); `None` for a part, whose one group is what its
/// stream meets.
pub(crate) fn begin(w: &mut Walk<'_>, e: DeclId, from: Option<DeclId>) {
    let ast = w.ast;
    let mut decls = w.redecls(e);
    decls.reverse();
    let mut covered = HashSet::new();
    covered.insert(ast.first(e));
    for &x in &decls {
        covered.insert(ast.first(x));
        if let Some(p) = ast.template_info(x).and_then(|i| i.pattern) {
            covered.insert(ast.first(p));
        }
    }
    let mut groups = Vec::new();
    let mut own = None;
    if let Some(from) = from {
        for &x in &decls {
            let mut g = Vec::new();
            signature(w, x, true, &mut g);
            if x == from {
                own = Some(groups.len());
            }
            groups.push(g);
        }
    }
    let own = own.unwrap_or_else(|| {
        groups.push(Vec::new());
        groups.len() - 1
    });
    let numbering = crate::local::numbering_mode(w);
    w.unit.operands = State { on: true, covered, groups, own, nested: None, writing: 0, numbering };
}

/// Walks the entries [`begin`] and the owner's stream collected while an
/// owner is numbered (`local::compute`, digest §4.3): the local types and
/// objects they declare are the owner's, numbered after those of its own
/// stream. Nothing is hashed or counted.
pub(crate) fn number(w: &mut Walk<'_>) {
    if !w.unit.operands.on {
        return;
    }
    let groups = std::mem::take(&mut w.unit.operands.groups);
    let mut entries = 0usize;
    for g in groups {
        let mut queue: VecDeque<Op> = g.into();
        while let Some(op) = queue.pop_front() {
            entries += 1;
            if entries > MAX_ENTRIES {
                break;
            }
            let (_, _, nested) = entry(w, op);
            for n in nested.into_iter().rev() {
                queue.push_front(n);
            }
        }
    }
    w.unit.operands.on = false;
}

/// The operands of declaration `d`'s signature, in order, into `out`: its
/// written type (for a function the return type and `noexcept(e)`, the
/// latter unless the resolved unit `exception_spec` holds it, S4), with
/// `params` each parameter's type before adjustment and, for a template,
/// its pattern's; a template head's default type arguments; a class
/// definition's base specifiers; a friend's type; the argument list as
/// written of a specialisation or explicit instantiation. A variable whose
/// type a placeholder deduced has no written type (S3).
fn signature(w: &mut Walk<'_>, d: DeclId, params: bool, out: &mut Vec<Op>) {
    let ast = w.ast;
    let n = ast.decls.get(d);
    match n.kind() {
        DeclKind::Function
        | DeclKind::CXXMethod
        | DeclKind::CXXConstructor
        | DeclKind::CXXDestructor
        | DeclKind::CXXConversion
        | DeclKind::CXXDeductionGuide => {
            if let Some(q) = n.ty() {
                function_type(w, d, q, out, 0);
            }
            if params && let Some(f) = ast.decls.function(d) {
                let ps: Vec<DeclId> = ast.lists.iter::<DeclId>(f.params).collect();
                for p in ps {
                    param(w, p, out);
                }
            }
        }
        DeclKind::ParmVar => param(w, d, out),
        DeclKind::Var => {
            if ast.decls.var(d).is_some_and(|v| v.spec.placeholder() != VarPlaceholder::None) {
                if counting(w) {
                    w.op_counts.deduced += 1;
                }
            } else if let Some(q) = n.ty() {
                scan(w, q, out, 0);
            }
        }
        DeclKind::Field => {
            if let Some(q) = n.ty() {
                scan(w, q, out, 0);
            }
        }
        DeclKind::Typedef | DeclKind::TypeAlias => {
            if let Some(t) = ast.decls.typedef(d) {
                scan(w, t.underlying, out, 0);
            }
        }
        DeclKind::Friend => {
            if let FriendTarget::Type { ty, owned_tag: _ } = slip_kiln::written::friend_target(ast, d)
            {
                scan(w, ty, out, 0);
            }
        }
        // The bases of a class whose definition is its resolved unit
        // `definition` (digest-kinds §11.9) are that unit's ([`bases`]).
        DeclKind::CXXRecord
        | DeclKind::ClassTemplateSpecialization
        | DeclKind::ClassTemplatePartialSpecialization
            if !crate::demand::definition_class(w, d) =>
        {
            bases_of(w, d, out);
        }
        _ => {}
    }
    if let Some(i) = ast.template_info(d).copied() {
        let list = ast.templates.param_list(i.params);
        let ps: Vec<DeclId> = ast.lists.iter::<DeclId>(list.params).collect();
        for p in ps {
            // A default non-type argument is walked by the unit's own
            // stream (`templ::head`); a default type argument is a type.
            if let Some(a) = ast.templates.defaults.get(p).copied()
                && let ArgView::Type(t) = a.view()
            {
                scan(w, t, out, 0);
            }
        }
        if params && let Some(p) = i.pattern {
            signature(w, p, true, out);
        }
    }
    if let Some(l) = ast.templates.args_as_written(d) {
        args(w, l, out, 0);
    }
}

/// The base specifiers of class definition `d`.
fn bases_of(w: &mut Walk<'_>, d: DeclId, out: &mut Vec<Op>) {
    let ast = w.ast;
    let bases: Vec<QualType> = ast.decls.bases(d, &ast.lists).map(|b| b.ty).collect();
    for b in bases {
        scan(w, b, out, 0);
    }
}

/// The operands of the base specifiers of `def`, the definition of a
/// class whose resolved unit `definition` holds them
/// (`decl_class::record_definition`).
pub(crate) fn bases(w: &mut Walk<'_>, def: DeclId) {
    if !collecting(w) {
        return;
    }
    let mut out = Vec::new();
    bases_of(w, def, &mut out);
    if !out.is_empty() {
        target(w).extend(out);
    }
}

/// A parameter's written type: before adjustment ([dcl.fct]/5).
fn param(w: &mut Walk<'_>, p: DeclId, out: &mut Vec<Op>) {
    let ast = w.ast;
    let q = ast.decls.param(p).and_then(|x| x.original_ty).or_else(|| ast.decls.get(p).ty());
    if let Some(q) = q {
        scan(w, q, out, 0);
    }
}

/// The type `q` of function declaration `d` as written: its return type
/// and `noexcept(e)` (not its parameters, read from their declarations,
/// S7); nothing when the type is a typedef's (`F f;`).
fn function_type(w: &mut Walk<'_>, d: DeclId, q: QualType, out: &mut Vec<Op>, depth: u32) {
    if depth > MAX_DEPTH {
        return;
    }
    let types = &w.ast.types;
    match types.view(q.ty()) {
        TypeView::Paren(x) | TypeView::Attributed { modified: x, .. } => {
            function_type(w, d, x, out, depth + 1);
        }
        TypeView::Function(p) => {
            scan(w, p.ret(), out, depth + 1);
            if let ExcSpec::NoexceptExpr { expr, .. } = types.exc_spec(&p) {
                // S4: the resolved unit `exception_spec` holds it
                // (`Walk::own_exception_spec`).
                if w.is_local(d) || !crate::demand::deferrable(w, d) {
                    push(w, Op::Expr(NOEXCEPT, expr), out);
                }
            }
        }
        _ => {}
    }
}

/// The operands of a resolved unit `exception_spec` (S4): its
/// `noexcept(e)`.
pub(crate) fn exception_spec(w: &mut Walk<'_>, spec: ExcSpec) {
    if !w.unit.operands.on {
        return;
    }
    if let ExcSpec::NoexceptExpr { expr, .. } = spec {
        let mut out = Vec::new();
        push(w, Op::Expr(NOEXCEPT, expr), &mut out);
        target(w).extend(out);
    }
}

/// Appends `op` to `out` unless it is instantiation-dependent (counted).
fn push(w: &mut Walk<'_>, op: Op, out: &mut Vec<Op>) {
    let ast = w.ast;
    let dependent = match op {
        Op::Expr(_, e) => ast.exprs.get(e).deps().contains(ExprDeps::INSTANTIATION),
        Op::Alias(_) => false,
    };
    if dependent {
        if counting(w) {
            w.op_counts.dependent += 1;
        }
    } else {
        out.push(op);
    }
}

/// The operands written in type `q` as written, in pre-order, into `out`
/// (the module documentation; modelled on Kiln's
/// `TypeContext::written_exprs`, which also follows a deduced type and
/// reads array bounds).
pub(crate) fn scan(w: &mut Walk<'_>, q: QualType, out: &mut Vec<Op>, depth: u32) {
    if depth > MAX_DEPTH {
        return;
    }
    let d = depth + 1;
    let types = &w.ast.types;
    match types.view(q.ty()) {
        TypeView::Pointer(x)
        | TypeView::LValueRef(x)
        | TypeView::RValueRef(x)
        | TypeView::IncompleteArray(x)
        | TypeView::Paren(x)
        | TypeView::Complex(x)
        | TypeView::Atomic(x)
        | TypeView::Vector { elem: x, .. }
        | TypeView::ExtQual { base: x, .. }
        | TypeView::Attributed { modified: x, .. }
        | TypeView::Decayed { original: x, .. }
        | TypeView::PackExpansion { pattern: x, .. }
        | TypeView::UnaryTransform { base: x, .. } => scan(w, x, out, d),
        TypeView::QualifiedArray { base, .. } => scan(w, QualType::unqual(base), out, d),
        TypeView::MemberPointer { pointee, class } => {
            scan(w, pointee, out, d);
            scan(w, QualType::unqual(class), out, d);
        }
        // The bound (site 3) is not read: Kiln keeps it as a value only
        // at the pin this was built for (B1; stage e).
        TypeView::ConstantArray(a) => scan(w, a.elem, out, d),
        TypeView::VariableArray { elem, .. } => scan(w, elem, out, d),
        TypeView::Function(p) => {
            // A function type that no function declaration holds (a
            // pointer to function): its parameters are read here (S7).
            let ret = p.ret();
            let params: Vec<QualType> = types.params(&p).to_vec();
            let exc = types.exc_spec(&p);
            scan(w, ret, out, d);
            for t in params {
                scan(w, t, out, d);
            }
            if let ExcSpec::NoexceptExpr { expr, .. } = exc {
                push(w, Op::Expr(NOEXCEPT, expr), out);
            }
        }
        TypeView::Elaborated { nns, named, .. } => {
            if let Some(n) = nns {
                scan_nns(w, n, out, d);
            }
            scan(w, named, out, d);
        }
        TypeView::TemplateSpecialization { args: l, alias, .. } => {
            if alias {
                // B5: an alias specialisation's written arguments, then
                // the operands inside them. Instantiation-dependent: the
                // pattern's own spellings hold it.
                if types.flags(q.ty()).is_instantiation_dependent() {
                    if counting(w) {
                        w.op_counts.dependent += 1;
                    }
                    return;
                }
                out.push(Op::Alias(l));
            }
            args(w, l, out, d);
        }
        TypeView::Auto { args: Some(l), deduced: _, .. } => args(w, l, out, d),
        TypeView::Decltype { expr, .. } => push(w, Op::Expr(DECLTYPE, expr), out),
        TypeView::TypeOf { operand: TypeOfOperand::Expr(e), .. } => {
            push(w, Op::Expr(TYPEOF, e), out);
        }
        TypeView::TypeOf { operand: TypeOfOperand::Type(t), .. } => scan(w, t, out, d),
        // A typedef's, alias's or using-declaration's type, a substituted
        // parameter's replacement, a deduced type, a class, a builtin, a
        // dependent type (its operands are a pattern's), a pack index
        // (site 5, B7), a splice: nothing is read.
        _ => {}
    }
}

/// The operands written in template argument list `l` as written: an
/// expression argument is site 4, a type argument is scanned.
fn args(w: &mut Walk<'_>, l: ArgListId, out: &mut Vec<Op>, depth: u32) {
    if depth > MAX_DEPTH {
        return;
    }
    let views: Vec<ArgView> = w.ast.types.args(l).iter().map(|a| a.view()).collect();
    for v in views {
        match v {
            ArgView::Type(t) => scan(w, t, out, depth + 1),
            ArgView::Expr { expr, .. } => push(w, Op::Expr(EXPR_ARG, expr), out),
            ArgView::Pack(p) => args(w, p, out, depth + 1),
            _ => {}
        }
    }
}

/// The operands written in nested-name-specifier `n`: the types it names.
fn scan_nns(w: &mut Walk<'_>, n: NnsId, out: &mut Vec<Op>, depth: u32) {
    if depth > MAX_DEPTH {
        return;
    }
    match w.ast.types.nns_view(n) {
        Nns::Type { prefix, ty, .. } => {
            if let Some(p) = prefix {
                scan_nns(w, p, out, depth + 1);
            }
            scan(w, ty, out, depth + 1);
        }
        Nns::Namespace { prefix: Some(p), .. }
        | Nns::NamespaceAlias { prefix: Some(p), .. }
        | Nns::Identifier { prefix: Some(p), .. } => scan_nns(w, p, out, depth + 1),
        _ => {}
    }
}

/// Where an operand met now goes: the entry being walked's nested list,
/// else the stream's own group.
fn target<'w>(w: &'w mut Walk<'_>) -> &'w mut Vec<Op> {
    let st = &mut w.unit.operands;
    match st.nested.as_mut() {
        Some(n) => n,
        None => &mut st.groups[st.own],
    }
}

/// The hook of `expr::write_node`: the holders of expression `e`, met in
/// a unit's own stream.
pub(crate) fn expr_node(w: &mut Walk<'_>, e: ExprId) {
    if !collecting(w) {
        return;
    }
    let ast = w.ast;
    let x = &ast.exprs;
    let n = *x.get(e);
    let mut out = Vec::new();
    match n.kind() {
        K::DeclRefExpr | K::MemberExpr => {
            if let Some(r) = x.decl_ref_extra(e).copied() {
                if let Some(q) = r.qualifier {
                    scan_nns(w, q, &mut out, 0);
                }
                if let Some(l) = r.template_args {
                    args(w, l, &mut out, 0);
                }
            }
            if let Some(m) = x.cold.member_overload.get(e).copied() {
                if let Some(q) = m.qualifier {
                    scan_nns(w, q, &mut out, 0);
                }
                if let Some(l) = m.template_args {
                    args(w, l, &mut out, 0);
                }
            }
        }
        K::ConceptSpecializationExpr => {
            if let Some(l) = ArgListId::try_new(n.b()) {
                args(w, l, &mut out, 0);
            }
        }
        K::CStyleCastExpr
        | K::CXXFunctionalCastExpr
        | K::CXXStaticCastExpr
        | K::CXXConstCastExpr
        | K::CXXReinterpretCastExpr
        | K::CXXDynamicCastExpr
        | K::BuiltinBitCastExpr => {
            // The node's type is the type as written but where Kiln keeps
            // that apart (`cast_written`).
            let q = x.cast_written_type(e).unwrap_or(n.ty());
            scan(w, q, &mut out, 0);
        }
        // `T(...)`, `T{...}` and `T()`: an explicit type conversion in
        // functional notation, whose node type is the type as written.
        K::CXXTemporaryObjectExpr | K::CXXScalarValueInitExpr => scan(w, n.ty(), &mut out, 0),
        K::UnaryExprOrTypeTraitExpr => {
            if n.has(ExprFlags::ARG_IS_TYPE)
                && let Some(t) = x.trait_type_arg(e)
            {
                scan(w, t, &mut out, 0);
            }
        }
        K::CXXTypeidExpr => {
            if let TypeidOperand::Type(t) = x.typeid_operand(e) {
                scan(w, t, &mut out, 0);
            }
        }
        K::TypeTraitExpr => {
            let ts: Vec<QualType> = ast.lists.iter::<QualType>(x.type_trait_args(e)).collect();
            for t in ts {
                scan(w, t, &mut out, 0);
            }
        }
        K::ArrayTypeTraitExpr => scan(w, x.array_trait_type(e), &mut out, 0),
        K::CXXNewExpr => scan(w, x.new_extra(e).allocated_ty, &mut out, 0),
        K::CompoundLiteralExpr => scan(w, x.compound_literal_written_type(e), &mut out, 0),
        _ => {}
    }
    if !out.is_empty() {
        target(w).extend(out);
    }
}

/// The hook of `Walk::decl_stream`: the signature of declaration `d`,
/// written in place in a unit's own stream (a local variable, typedef or
/// alias, a parameter of a function that is not the unit's entity, a
/// member of a local class), unless the groups read it ([`begin`]). A
/// template parameter's default is read by the groups of its template's
/// declarations only (a local template's is not: a limit by name).
pub(crate) fn decl(w: &mut Walk<'_>, d: DeclId) {
    if !collecting(w) {
        return;
    }
    let ast = w.ast;
    let kind = ast.decls.get(d).kind();
    if matches!(
        kind,
        DeclKind::TemplateTypeParm | DeclKind::NonTypeTemplateParm | DeclKind::TemplateTemplateParm
    ) {
        return;
    }
    let covered = &w.unit.operands.covered;
    if covered.contains(&ast.first(d)) {
        return;
    }
    if kind == DeclKind::ParmVar
        && ast.decls.get(d).ctx().is_some_and(|c| covered.contains(&ast.first(c)))
    {
        return;
    }
    let mut out = Vec::new();
    signature(w, d, false, &mut out);
    if !out.is_empty() {
        target(w).extend(out);
    }
}

/// While an owner is numbered (`local::member`, which enters no member
/// of a local type): the operands the writing walk meets at member `e`'s
/// declarations, as [`decl`] meets them in its stream (its own, its
/// pattern's, and their parameters'), so that the local types and objects
/// in them are numbered as the operand unit walks them.
pub(crate) fn member(w: &mut Walk<'_>, e: DeclId) {
    if !collecting(w) {
        return;
    }
    let ast = w.ast;
    let from = crate::units::read_decl(w, e);
    let mut decls = vec![from];
    if let Some(p) = ast.template_info(from).and_then(|i| i.pattern) {
        decls.push(p);
    }
    for x in decls {
        decl(w, x);
        let ps: Vec<DeclId> =
            ast.decls.function(x).map(|f| ast.lists.iter::<DeclId>(f.params).collect()).unwrap_or_default();
        for p in ps {
            decl(w, p);
        }
    }
}

/// Whether parameter `d` is a parameter of a function declaration (else of
/// a function type no function declaration holds, `void (*p)(int a)`,
/// whose context is the scope around it): `Walk::write_ref` writes the
/// latter as `Ref::param`, by depth and index (S6).
pub(crate) fn of_function(w: &Walk<'_>, d: DeclId) -> bool {
    w.ast.decls.get(d).ctx().is_some_and(|c| w.ast.decls.function(c).is_some())
}

/// Ends the operand unit of the unit being written, whose stream is
/// finished and whose state is still live: writes its groups (the module
/// documentation) and returns its hashed stream, or `None` when no
/// operand is written.
pub(crate) fn finish(w: &mut Walk<'_>) -> Option<Hashed> {
    if !w.unit.operands.on {
        return None;
    }
    let groups = std::mem::take(&mut w.unit.operands.groups);
    // The parent's own-spelling list and mark are written: what the
    // entries meet is the operand unit's.
    let facts = std::mem::take(&mut w.unit.facts);
    let facts_due = std::mem::take(&mut w.unit.facts_due);
    let emits = std::mem::take(&mut w.unit.emits);
    let mut s = Stream::new();
    let mut total = 0usize;
    let mut entries = 0usize;
    for g in groups {
        let mut gs = Stream::new();
        let mut count = 0usize;
        let mut queue: VecDeque<Op> = g.into();
        while let Some(op) = queue.pop_front() {
            entries += 1;
            if entries > MAX_ENTRIES {
                s.indeterminate(TOO_MANY);
                break;
            }
            let (es, written, nested) = entry(w, op);
            for n in nested.into_iter().rev() {
                queue.push_front(n);
            }
            if written {
                count += 1;
                gs.raw(es.as_bytes()).absorb(es.cause());
            }
        }
        if count > 0 {
            s.count(count);
            s.raw(gs.as_bytes()).absorb(gs.cause());
            total += count;
        }
    }
    w.unit.operands.on = false;
    let h = if total > 0 {
        crate::own_spelling::write_unit_list(w, &mut s);
        let h = s.finish(UNIT_TAG);
        Some(w.hashed(h))
    } else {
        None
    };
    w.unit.facts = facts;
    w.unit.facts_due = facts_due;
    w.unit.emits = emits;
    h
}

/// Writes one entry: its stream, whether it is written (S5), and the
/// operands its walk met (S7).
fn entry(w: &mut Walk<'_>, op: Op) -> (Stream, bool, Vec<Op>) {
    let saved = w.unit.operands.nested.replace(Vec::new());
    w.unit.operands.writing += 1;
    let refs = w.op_refs;
    // The names digest (digest §9) is fed by a written entry only.
    let outer_names = w.names.take();
    if outer_names.is_some() {
        w.names = Some(slip_model::wire::Encoder::default());
    }
    let mut es = Stream::new();
    let site = match op {
        Op::Expr(site, e) => {
            es.uv(u64::from(site));
            crate::expr::child(w, &mut es, e);
            site
        }
        Op::Alias(l) => {
            es.uv(u64::from(ALIAS_ARGS));
            w.write_args(&mut es, l);
            ALIAS_ARGS
        }
    };
    w.unit.operands.writing -= 1;
    let nested = std::mem::replace(&mut w.unit.operands.nested, saved).unwrap_or_default();
    let written = site == ALIAS_ARGS || w.op_refs != refs || es.cause().is_some();
    let mine = std::mem::replace(&mut w.names, outer_names);
    if written
        && let (Some(outer), Some(mine)) = (w.names.as_mut(), mine)
    {
        for &b in mine.as_bytes() {
            outer.u8(b);
        }
    }
    if counting(w) {
        if written {
            w.op_counts.sites[usize::from(site)] += 1;
        } else {
            w.op_counts.unreferenced += 1;
        }
    }
    (es, written, nested)
}

/// The operand unit of `parent` (a unit of entity `e`, written), whose
/// stream is `h`: key `operands` / `Parent::Unit(parent.key)` /
/// `operands`, no discriminator; derived, in the derived group of the
/// parent's file (none for a namespace's); the parent's cause.
pub(crate) fn unit(w: &mut Walk<'_>, e: DeclId, parent: &Written, h: Hashed) -> Written {
    let key = Key {
        kind: KeyKind::Operands,
        parent: Parent::Unit(parent.key),
        name: Name::Operands,
        disc: None,
    };
    let key = w.rules.map(key);
    w.bytes += key.to_bytes().len() as u64;
    w.op_counts.units += 1;
    let file = match parent.class {
        GroupClass::Tu => Vec::new(),
        GroupClass::Declared | GroupClass::Derived => {
            if parent.file.is_empty() {
                crate::units::origin_file(w, parent.read.unwrap_or(e))
            } else {
                parent.file.clone()
            }
        }
    };
    // The parent's cause first: an operand unit inherits it.
    let cause = match (parent.cause, h.cause) {
        (Some(p), Some(own)) => Some(merge(Some(p), own)),
        (p, own) => p.or(own),
    };
    Written {
        entity: Some(e),
        read: parent.read,
        key: key.id(),
        kind: KeyKind::Operands,
        digest: h.digest,
        cause,
        class: GroupClass::Derived,
        file,
        resolved: parent.resolved,
        default_argument: parent.default_argument,
        owner: Some(parent.key),
        collided: false,
        view: None,
        denotes: None,
        emitted: false,
        operands: Some(parent.kind),
    }
}

/// Merges into each operand unit of `written` the cause of its parent as
/// it stands after `units::apply_taints` and the marks of collisions (an
/// operand unit inherits its parent's cause, `spec/landed/digest/WP-153.md`
/// §13.2's notes).
pub(crate) fn inherit_causes(written: &mut [Written]) {
    // By key: every unit that is no operand unit under the parent's key
    // gives its cause (a key two entities share is a collision of both).
    let mut causes: HashMap<Digest, Option<Cause>> = HashMap::new();
    for p in written.iter().filter(|p| p.operands.is_none()) {
        let c = causes.entry(p.key).or_insert(None);
        if let Some(pc) = p.cause {
            *c = Some(merge(*c, pc));
        }
    }
    for u in written.iter_mut().filter(|u| u.operands.is_some()) {
        if let Some(&Some(c)) = u.owner.and_then(|k| causes.get(&k)) {
            u.cause = Some(match u.cause {
                Some(own) => merge(Some(c), own),
                None => c,
            });
        }
    }
}

/// The counters (`spec/digest.md` §12): `digest.operands.*`.
pub(crate) fn counters(w: &Walk<'_>, out: &mut Vec<slip_model::Counter>) {
    let c = &w.op_counts;
    // Each only when not 0, as `digest.indeterminate.<cause>`: a
    // translation unit with no operand writes none.
    let mut push = |name: &str, value: u64| {
        if value != 0 {
            out.push(slip_model::Counter { name: format!("digest.operands.{name}"), value });
        }
    };
    push("units", c.units);
    for site in [DECLTYPE, TYPEOF, EXPR_ARG, NOEXCEPT, ALIAS_ARGS] {
        push(&format!("site.{site}"), c.sites[usize::from(site)]);
    }
    push("unreferenced", c.unreferenced);
    push("dependent", c.dependent);
    push("deduced", c.deduced);
}
