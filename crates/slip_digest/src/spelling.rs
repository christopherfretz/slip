//! What a unit's own spellings hold beyond their profiles
//! (`spec/digest-kinds.md` §4.4 "Spellings", WP-041).
//!
//! A dependent type is canonical by profile (§4.2 rule 1), and Kiln's
//! profile of an expression leaves out three facts its spelling holds:
//! the declarations unqualified lookup found where the template was
//! written, for an unresolved name (`UnresolvedLookupExpr.b`) and for a
//! dependent operator (`TemplateTables` `oper_lookups`), which are what
//! overload resolution starts from at instantiation, and the functions a
//! resolved `new` or `delete` chose there (`NewExtra`, `CXXDeleteExpr.b`). The type's bytes are the
//! profile's, a function of the profile and not of which spelling Kiln
//! met first; the unit writes the left-out facts of its **own** spelling,
//! once, at the end of its stream ([`write_unit_list`]).
//!
//! The facts are read from the spelling the unit holds: the written type
//! (through sugar, [`left_out_of_type`]) or the expression
//! ([`left_out_of_expr`]), in the order of their structure, which is the
//! order of the profile's structure (two spellings with one profile have
//! their unresolved names and operators in the same places, so their
//! lists line up entry for entry). A typedef, an alias or an alias
//! template specialisation names another unit, which writes its own
//! facts: the list holds its `Ref` and the walk does not descend into
//! what it aliases (an alias template's arguments, which the unit wrote,
//! are walked).
//!
//! The list is written when the unit's own spellings hold a non-empty set
//! (or name an alias that does), whatever the mark of the canonical type:
//! the mark is a fact of the first spelling only. Where the unit holds a
//! canonical operand and no spelling of its own (a type Kiln computed, a
//! canonical argument list) and the operand's type is marked, the unit is
//! `Unaccounted("spelling/<site>")`.

use slip_kiln::kiln::ast::decl::DeclKind;
use slip_kiln::kiln::ast::expr::{Designator, ExprFlags, ExprKind as K, TypeidOperand};
use slip_kiln::kiln::ast::templates::Requirement;
use slip_kiln::kiln::ast::types::{
    ArgView, ExcSpec, ExprOperand, Nns, NoexceptValue, TemplateArg, TemplateName, TypeFlags,
    TypeOfOperand, TypeView,
};
use slip_kiln::kiln::ast::{ArgListId, DeclId, ExprId, ListId, NnsId, QualType, TemplateNameId};

use crate::Cause;
use crate::stream::Stream;
use crate::walk::{TOO_DEEP, Walk};

/// One fact a profile leaves out, as the unit's spelling holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Fact {
    /// The declarations an unresolved name found (possibly none).
    Found(Vec<DeclId>),
    /// The non-member operator functions a dependent operator found
    /// (`oper_lookups`, possibly none).
    Oper(Vec<DeclId>),
    /// The allocation and deallocation functions a resolved `new`, or
    /// the deallocation function a resolved `delete`, chose where it was
    /// written (`NewExtra::operator_new`, `operator_delete`,
    /// `CXXDeleteExpr.b`), in that order.
    Chosen(Vec<Option<DeclId>>),
    /// A typedef, alias or alias template whose own spelling holds a
    /// left-out fact: written by its unit.
    Alias(DeclId),
}

/// The kind tags of a list entry (§4.4 "Spellings").
pub(crate) const FACT_FOUND: u64 = 1;
pub(crate) const FACT_OPER: u64 = 2;
pub(crate) const FACT_ALIAS: u64 = 3;
pub(crate) const FACT_CHOSEN: u64 = 4;

impl Fact {
    /// Whether the fact makes a list due: a non-empty set, or an alias
    /// (pushed only when its own spelling makes a list due).
    fn due(&self) -> bool {
        match self {
            Fact::Found(ds) | Fact::Oper(ds) => !ds.is_empty(),
            Fact::Chosen(ds) => ds.iter().any(Option::is_some),
            Fact::Alias(_) => true,
        }
    }
}

#[cfg(test)]
thread_local! {
    /// The expressions whose non-empty set a reading yielded directly
    /// (not through a type nested in the expression): the count test of
    /// rule 6 (`spelling_tests`).
    pub(crate) static DIRECT: std::cell::RefCell<Vec<ExprId>> =
        const { std::cell::RefCell::new(Vec::new()) };
    /// How many types the reading is inside.
    static IN_TYPE: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    /// Lists are not written (decision 2's test: what a list changes).
    pub(crate) static NO_LIST: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// How many lists were written.
    pub(crate) static LISTS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Where the mark of a canonical operand is read: the type around it, or
/// a mark already known (a profile written straight into the unit).
#[derive(Clone, Copy, Debug)]
pub(crate) enum Ctx {
    Type(QualType),
    Known(bool),
}

/// Whether the type digest of `q` is marked (its profile's first spelling
/// left a fact out), without touching the walk's counts.
fn marked(w: &mut Walk<'_>, ctx: Ctx) -> bool {
    match ctx {
        Ctx::Known(b) => b,
        Ctx::Type(q) => {
            let left_out = w.left_out;
            let uses = w.unit.local_uses;
            let t = w.type_digest(q, false);
            w.left_out = left_out;
            w.unit.local_uses = uses;
            t.spelling
        }
    }
}

/// A canonical operand where the unit has no spelling of its own: rule 5,
/// `Unaccounted(site)` when the type around it is marked.
fn no_spelling(w: &mut Walk<'_>, s: &mut Stream, ctx: Ctx, site: &'static str) {
    if marked(w, ctx) {
        s.indeterminate(Cause::Unaccounted(site));
    }
}

fn push(w: &mut Walk<'_>, f: Fact) {
    w.unit.facts_due |= f.due();
    w.unit.facts.push(f);
}

/// The left-out facts of expression `e`, the unit's own (a profile written
/// straight into a unit stream: a template argument, a table row), added
/// to the unit's list; `marked` is whether its profile walk left a fact
/// out (read for a kind walked in full).
pub(crate) fn left_out_of_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId, marked: bool) {
    let facts = std::mem::take(&mut w.left_out);
    expr(w, s, e, Ctx::Known(marked));
    w.left_out = facts;
}

/// The left-out facts of written type `written`, added to the unit's list.
/// `own_fn`: `written` is a function declaration's own type, whose
/// exception specification its unit walks in full (`DeclNode`
/// `ty-exception-spec`), so it is not read here.
pub(crate) fn left_out_of_type(w: &mut Walk<'_>, s: &mut Stream, written: QualType, own_fn: bool) {
    ty(w, s, written, own_fn);
}

/// The left-out facts of a written template argument list (a partial
/// specialisation's arguments as written).
pub(crate) fn left_out_of_args(w: &mut Walk<'_>, s: &mut Stream, l: ArgListId, marked: bool) {
    args(w, s, l, Ctx::Known(marked));
}

/// The left-out facts of one template argument as written (a default
/// template argument).
pub(crate) fn left_out_of_arg(w: &mut Walk<'_>, s: &mut Stream, a: TemplateArg, marked: bool) {
    arg(w, s, a, Ctx::Known(marked));
}

/// The end of a unit stream: when the unit's spellings hold a non-empty
/// set, `count` and per fact its kind tag and payload, in walk order; else
/// nothing (digest-kinds §4.4 "Spellings").
pub(crate) fn write_unit_list(w: &mut Walk<'_>, s: &mut Stream) {
    let facts = std::mem::take(&mut w.unit.facts);
    let due = std::mem::take(&mut w.unit.facts_due);
    if !due {
        return;
    }
    #[cfg(test)]
    {
        if NO_LIST.with(std::cell::Cell::get) {
            return;
        }
        LISTS.with(|c| c.set(c.get() + 1));
    }
    s.count(facts.len());
    for f in facts {
        match f {
            Fact::Found(ds) => {
                s.uv(FACT_FOUND);
                w.write_refs_sorted(s, &ds);
            }
            Fact::Oper(ds) => {
                s.uv(FACT_OPER);
                w.write_refs_sorted(s, &ds);
            }
            Fact::Chosen(ds) => {
                s.uv(FACT_CHOSEN).count(ds.len());
                for d in ds {
                    w.write_ref_opt(s, d);
                }
            }
            Fact::Alias(d) => {
                s.uv(FACT_ALIAS);
                w.write_ref(s, d);
            }
        }
    }
}

/// Whether the own spelling of typedef-name `d` (its aliased type as
/// written) makes a list due, or cannot be read: then a use of it writes
/// its `Ref` (rule 4). Memoised per walk.
fn alias_due(w: &mut Walk<'_>, d: DeclId) -> bool {
    let d = w.ast.first(d);
    if let Some(&due) = w.alias_due.get(&d) {
        return due;
    }
    let Some(underlying) = w.ast.decls.typedef(d).map(|t| t.underlying) else {
        return false;
    };
    // Guard against a cycle (an alias whose reading reaches itself); read
    // from depth 0, so the answer does not depend on who asks first.
    w.alias_due.insert(d, false);
    let facts = std::mem::take(&mut w.unit.facts);
    let due = std::mem::take(&mut w.unit.facts_due);
    let depth = std::mem::take(&mut w.depth);
    let node_depth = std::mem::take(&mut w.node_depth);
    let mut scratch = Stream::new();
    ty(w, &mut scratch, underlying, false);
    w.depth = depth;
    w.node_depth = node_depth;
    let mine = w.unit.facts_due || scratch.cause().is_some();
    w.unit.facts = facts;
    w.unit.facts_due = due;
    w.alias_due.insert(d, mine);
    mine
}

fn alias(w: &mut Walk<'_>, d: DeclId) {
    if alias_due(w, d) {
        push(w, Fact::Alias(w.ast.first(d)));
    }
}

/// The written type `q`, through sugar, in the order of its structure.
#[allow(clippy::too_many_lines, reason = "one arm per TypeView variant")]
#[allow(
    clippy::many_single_char_names,
    reason = "s stream, q type, n qualifier, l list, p prototype"
)]
fn ty(w: &mut Walk<'_>, s: &mut Stream, q: QualType, own_fn: bool) {
    let types = &w.ast.types;
    if !types.flags(q.ty()).intersects(TypeFlags::DEPENDENT_INSTANTIATION) {
        // No profile in it.
        return;
    }
    if !w.enter() {
        s.indeterminate(TOO_DEEP);
        return;
    }
    #[cfg(test)]
    IN_TYPE.with(|c| c.set(c.get() + 1));
    let here = Ctx::Type(q);
    match types.view(q.ty()) {
        TypeView::Invalid
        | TypeView::Builtin(_)
        | TypeView::BitInt { signed: _, width: _ }
        | TypeView::Record(_)
        | TypeView::Enum(_)
        | TypeView::UnresolvedUsing(_)
        | TypeView::TemplateTypeParm(_)
        | TypeView::CanonTemplateTypeParm { depth: _, index: _, pack: _ }
        | TypeView::VariableArray { elem: _, size: _ }
        | TypeView::InjectedClassName { pattern: _, injected: _ }
        | TypeView::Splice { operand: _, designated: _ } => {}
        TypeView::DependentBitInt { signed: _, width: _ } => {
            no_spelling(w, s, here, "spelling/bit-int");
        }
        TypeView::Pointer(x)
        | TypeView::LValueRef(x)
        | TypeView::RValueRef(x)
        | TypeView::IncompleteArray(x)
        | TypeView::Complex(x)
        | TypeView::Atomic(x)
        | TypeView::ExtQual { base: x, word: _ }
        | TypeView::PackExpansion { pattern: x, num_expansions: _ }
        | TypeView::Vector { elem: x, count: _, kind: _ }
        | TypeView::SubstTemplateTypeParm {
            replacement: x,
            associated: _,
            index: _,
            pack_index: _,
        } => ty(w, s, x, false),
        TypeView::QualifiedArray { quals: _, base } => ty(w, s, QualType::unqual(base), false),
        TypeView::Paren(x) => ty(w, s, x, own_fn),
        TypeView::Attributed { kind: _, modified, equivalent: _ } => ty(w, s, modified, own_fn),
        TypeView::ConstantArray(a) => {
            let slip_kiln::kiln::ast::types::ConstArray { elem, size: _, size_expr: _ } = a;
            ty(w, s, elem, false);
        }
        TypeView::MemberPointer { pointee, class } => {
            ty(w, s, pointee, false);
            ty(w, s, QualType::unqual(class), false);
        }
        TypeView::DependentSizedArray { elem, size } => {
            ty(w, s, elem, false);
            operand(w, s, size, here, "spelling/array-bound");
        }
        TypeView::DependentVector { elem, size, kind: _ } => {
            ty(w, s, elem, false);
            operand(w, s, size, here, "spelling/vector-size");
        }
        TypeView::DependentAddrSpace { pointee, space } => {
            ty(w, s, pointee, false);
            operand(w, s, space, here, "spelling/addr-space");
        }
        TypeView::Function(p) => {
            ty(w, s, p.ret(), false);
            let params = w.ast.types.params(&p).to_vec();
            for x in params {
                ty(w, s, x, false);
            }
            if !own_fn {
                let e = w.ast.types.exc_spec(&p);
                exc(w, s, e, here);
            }
        }
        TypeView::Typedef { decl, underlying: _ } => alias(w, decl),
        TypeView::Using { shadow, underlying } => {
            let target = w.ast.strip_shadow(shadow);
            if w.ast.decls.typedef(target).is_some() {
                alias(w, target);
            } else {
                ty(w, s, underlying, false);
            }
        }
        TypeView::Elaborated { keyword: _, nns: n, named } => {
            if let Some(n) = n {
                nns(w, s, n);
            }
            ty(w, s, named, false);
        }
        TypeView::SubstTemplateTypeParmPack { associated: _, index: _, pack: _ } => {
            no_spelling(w, s, here, "spelling/subst-pack");
        }
        TypeView::TemplateSpecialization { name, args: l, underlying, alias: is_alias } => {
            if is_alias {
                // Rule 4: the alias template's own spelling is its unit's;
                // the arguments are this unit's.
                if let Some(t) = alias_template(w, name)
                    && let Some(pattern) = w.ast.template_info(t).and_then(|i| i.pattern)
                {
                    alias(w, pattern);
                }
                args(w, s, l, here);
            } else if underlying.is_none() {
                template_name(w, s, name);
                args(w, s, l, here);
            }
        }
        TypeView::DependentName { keyword: _, nns: n, name: _ } => nns(w, s, n),
        TypeView::DependentTemplateSpecialization { keyword: _, nns: n, name: _, args: l } => {
            nns(w, s, n);
            args(w, s, l, here);
        }
        TypeView::PackIndexing { pattern, index, selected } => {
            if selected.is_some() {
                no_spelling(w, s, here, "spelling/deduced");
            } else {
                ty(w, s, pattern, false);
                operand(w, s, index, here, "spelling/pack-index");
            }
        }
        TypeView::Decltype { expr: e, underlying: _ } => expr(w, s, e, here),
        TypeView::DependentDecltype(_) => no_spelling(w, s, here, "spelling/decltype"),
        TypeView::TypeOf { operand: op, underlying: _, unqual: _ } => match op {
            TypeOfOperand::Expr(e) => expr(w, s, e, here),
            TypeOfOperand::Type(t) => ty(w, s, t, false),
            TypeOfOperand::Profile(_) => no_spelling(w, s, here, "spelling/typeof"),
        },
        TypeView::UnaryTransform { kind: _, base, result } => match result {
            None => ty(w, s, base, false),
            Some(_) => no_spelling(w, s, here, "spelling/deduced"),
        },
        TypeView::Auto { keyword: _, deduced, concept: _, args: l, pack: _ } => {
            if let Some(l) = l {
                args(w, s, l, here);
            }
            if deduced.is_some_and(|d| d != QualType::DEPENDENT) {
                no_spelling(w, s, here, "spelling/deduced");
            }
        }
        TypeView::DeducedTemplateSpecialization { name: _, deduced } => {
            if deduced.is_some_and(|d| d != QualType::DEPENDENT) {
                no_spelling(w, s, here, "spelling/deduced");
            }
        }
    }
    #[cfg(test)]
    IN_TYPE.with(|c| c.set(c.get() - 1));
    w.leave();
}

/// The alias template a written alias template specialisation names.
fn alias_template(w: &Walk<'_>, mut n: TemplateNameId) -> Option<DeclId> {
    let types = &w.ast.types;
    for _ in 0..64 {
        match types.template_name_view(n) {
            TemplateName::Template(d) => return Some(d),
            TemplateName::Qualified { nns: _, template_kw: _, underlying }
            | TemplateName::Using { shadow: _, underlying } => n = underlying,
            TemplateName::SubstTemplateTemplateParm { replacement, associated: _, index: _ } => {
                n = replacement;
            }
            TemplateName::DependentIdentifier { .. }
            | TemplateName::DependentOperator { .. }
            | TemplateName::TemplateTemplateParm { .. }
            | TemplateName::CanonTemplateTemplateParm { .. }
            | TemplateName::SubstTemplateTemplateParmPack { .. }
            | TemplateName::Overloaded(_)
            | TemplateName::Splice { .. }
            | TemplateName::SpliceProfile(_) => return None,
        }
    }
    None
}

/// An expression operand of a type: its expression as written, or rule 5
/// for a canonical one.
fn operand(w: &mut Walk<'_>, s: &mut Stream, op: ExprOperand, ctx: Ctx, site: &'static str) {
    match op {
        ExprOperand::Expr(e) => expr(w, s, e, ctx),
        ExprOperand::Profile(_) => no_spelling(w, s, ctx, site),
    }
}

/// A function type's exception specification as written.
fn exc(w: &mut Walk<'_>, s: &mut Stream, e: ExcSpec, ctx: Ctx) {
    match e {
        ExcSpec::NoexceptExpr { expr: x, value: NoexceptValue::Dependent(_) } => {
            expr(w, s, x, ctx);
        }
        ExcSpec::DependentNoexcept(_) => no_spelling(w, s, ctx, "spelling/noexcept"),
        ExcSpec::Dynamic(list) => {
            let ts = w.ast.types.type_list_types(list).to_vec();
            for t in ts {
                ty(w, s, t, false);
            }
        }
        ExcSpec::NoexceptExpr { expr: _, value: NoexceptValue::False | NoexceptValue::True }
        | ExcSpec::None
        | ExcSpec::Nothrow
        | ExcSpec::DynamicNone
        | ExcSpec::NothrowAttr
        | ExcSpec::BasicNoexcept
        | ExcSpec::Deferred(_) => {}
    }
}

/// A qualifier as written, outermost level last (each level's type).
fn nns(w: &mut Walk<'_>, s: &mut Stream, n: NnsId) {
    let mut cur = Some(n);
    let mut levels = Vec::new();
    while let Some(n) = cur {
        if levels.len() > 4096 {
            s.indeterminate(TOO_DEEP);
            return;
        }
        levels.push(n);
        cur = match w.ast.types.nns_view(n) {
            Nns::Namespace { prefix, ns: _ }
            | Nns::NamespaceAlias { prefix, alias: _, ns: _ }
            | Nns::Type { prefix, ty: _, template_kw: _ }
            | Nns::Identifier { prefix, name: _ } => prefix,
            Nns::Global | Nns::Splice(_) => None,
        };
    }
    for &n in levels.iter().rev() {
        if let Nns::Type { prefix: _, ty: t, template_kw: _ } = w.ast.types.nns_view(n) {
            ty(w, s, t, false);
        }
    }
}

/// A template name as written: its qualifier.
fn template_name(w: &mut Walk<'_>, s: &mut Stream, n: TemplateNameId) {
    let types = &w.ast.types;
    match types.template_name_view(n) {
        TemplateName::Qualified { nns: q, template_kw: _, underlying } => {
            nns(w, s, q);
            template_name(w, s, underlying);
        }
        TemplateName::DependentIdentifier { nns: q, name: _ }
        | TemplateName::DependentOperator { nns: q, op: _ } => nns(w, s, q),
        TemplateName::Using { shadow: _, underlying }
        | TemplateName::SubstTemplateTemplateParm { replacement: underlying, .. } => {
            template_name(w, s, underlying);
        }
        TemplateName::Template(_)
        | TemplateName::TemplateTemplateParm { .. }
        | TemplateName::CanonTemplateTemplateParm { .. }
        | TemplateName::SubstTemplateTemplateParmPack { .. }
        | TemplateName::Overloaded(_)
        | TemplateName::Splice { .. }
        | TemplateName::SpliceProfile(_) => {}
    }
}

/// A template argument list as written.
fn args(w: &mut Walk<'_>, s: &mut Stream, l: ArgListId, ctx: Ctx) {
    let list = w.ast.types.args(l).to_vec();
    for a in list {
        arg(w, s, a, ctx);
    }
}

/// A template argument as written.
fn arg(w: &mut Walk<'_>, s: &mut Stream, a: TemplateArg, ctx: Ctx) {
    match a.view() {
        ArgView::Null | ArgView::Value { ty: _, value: _ } => {}
        ArgView::Type(q) => ty(w, s, q, false),
        ArgView::Template(n) | ArgView::TemplateExpansion { name: n, num_expansions: _ } => {
            template_name(w, s, n);
        }
        ArgView::Expr { expr: e, profile: _ } => expr(w, s, e, ctx),
        ArgView::Profile(_) => no_spelling(w, s, ctx, "spelling/arg"),
        ArgView::Pack(p) => args(w, s, p, ctx),
    }
}

/// One step of the reading of an expression.
#[derive(Clone, Copy, Debug)]
enum I {
    E(ExprId),
    T(QualType),
    A(ArgListId),
    N(NnsId),
}

fn list_raw(w: &Walk<'_>, raw: u32, out: &mut Vec<I>) {
    if let Some(l) = ListId::try_new(raw) {
        out.extend(w.ast.lists.iter::<ExprId>(l).map(I::E));
    }
}

/// Expression `e` as written, in the order profile mode walks it
/// (`expr_profile::node`): the same children and the same types, an
/// argument list and a qualifier as written. A declaration profile mode
/// writes by position (a parameter, a template parameter) is a reference
/// whose type its own declaration's unit writes.
fn expr(w: &mut Walk<'_>, s: &mut Stream, root: ExprId, ctx: Ctx) {
    if !w.enter_node() {
        s.indeterminate(TOO_DEEP);
        return;
    }
    let mut stack = vec![I::E(root)];
    while let Some(i) = stack.pop() {
        match i {
            I::E(e) => {
                let mut kids = Vec::new();
                node(w, s, e, ctx, &mut kids);
                stack.extend(kids.into_iter().rev());
            }
            I::T(q) => ty(w, s, q, false),
            I::A(l) => args(w, s, l, ctx),
            I::N(n) => nns(w, s, n),
        }
    }
    w.leave_node();
}

/// The facts of node `e` and its children, in order, into `kids`.
#[allow(clippy::too_many_lines, reason = "one arm per expression kind, as profile mode")]
#[allow(clippy::many_single_char_names, reason = "s stream, e expression, n node, k kind, x arena")]
fn node(w: &mut Walk<'_>, s: &mut Stream, e: ExprId, ctx: Ctx, kids: &mut Vec<I>) {
    crate::walk::stack_probe();
    let ast = w.ast;
    let x = &ast.exprs;
    let n = *x.get(e);
    let k = n.kind();
    let id = ExprId::try_new;
    let opt = |kids: &mut Vec<I>, e: Option<ExprId>| kids.extend(e.map(I::E));
    // The set a dependent operator's lookup found at the definition
    // (`oper_lookups`, digest-kinds §9.5): left out of every profile.
    if let Some(cands) = ast.templates.oper_lookups.of(e) {
        let ds: Vec<DeclId> = cands.iter().map(|c| c.decl).collect();
        record(e, !ds.is_empty());
        push(w, Fact::Oper(ds));
    }
    let cast_written = || x.cast_written_type(e);
    match k {
        K::SubstNonTypeTemplateParmExpr => opt(kids, id(n.b())),
        K::OpaqueValueExpr => opt(kids, x.opaque_source(e)),
        K::IntegerLiteral
        | K::FloatingLiteral
        | K::CharacterLiteral
        | K::StringLiteral
        | K::CXXNullPtrLiteralExpr
        | K::GNUNullExpr
        | K::ImplicitValueInitExpr
        | K::CXXScalarValueInitExpr => kids.push(I::T(n.ty())),
        K::CXXBoolLiteralExpr
        | K::AddrLabelExpr
        | K::CXXThisExpr
        | K::PredefinedExpr
        | K::SourceLocExpr
        | K::SizeOfPackExpr
        | K::CXXDefaultArgExpr
        | K::CXXDefaultInitExpr
        | K::ArrayInitIndexExpr
        | K::NoInitExpr => {}
        K::DeclRefExpr | K::MemberExpr => {
            if let Some(l) = x.decl_ref_extra(e).and_then(|r| r.template_args) {
                kids.push(I::A(l));
            }
            if k == K::MemberExpr {
                opt(kids, id(n.a()));
            }
        }
        K::UnresolvedLookupExpr => {
            let found: Vec<DeclId> = ListId::try_new(n.b())
                .map(|l| ast.lists.iter::<DeclId>(l).collect())
                .unwrap_or_default();
            // `V<args>` of a variable template template parameter: the
            // parameter by position, as profile mode writes it.
            let ttp = found
                .first()
                .is_some_and(|&d| ast.decls.get(d).kind() == DeclKind::TemplateTemplateParm);
            if !ttp {
                record(e, !found.is_empty());
                push(w, Fact::Found(found));
            }
            kids.extend(ast.templates.expr_nns.get(e).copied().map(I::N));
            kids.extend(ast.templates.expr_targs.get(e).copied().map(I::A));
        }
        K::DependentScopeDeclRefExpr => {
            kids.extend(NnsId::try_new(n.a()).map(I::N));
            kids.extend(ast.templates.expr_targs.get(e).copied().map(I::A));
        }
        K::CXXDependentScopeMemberExpr => {
            kids.extend(ast.templates.expr_nns.get(e).copied().map(I::N));
            kids.extend(ast.templates.expr_targs.get(e).copied().map(I::A));
            opt(kids, id(n.a()));
        }
        K::ImplicitCastExpr
        | K::ConstantExpr
        | K::MaterializeTemporaryExpr
        | K::CXXBindTemporaryExpr
        | K::ExprWithCleanups
        | K::UnaryOperator
        | K::ParenExpr
        | K::PackExpansionExpr
        | K::CXXNoexceptExpr
        | K::IntegerPackExpr => opt(kids, id(n.a())),
        K::CXXDeleteExpr => {
            if let Some(d) = DeclId::try_new(n.b()) {
                record(e, true);
                push(w, Fact::Chosen(vec![Some(d)]));
            }
            opt(kids, id(n.a()));
        }
        K::BinaryOperator
        | K::ArraySubscriptExpr
        | K::PackIndexingExpr
        | K::ArrayInitLoopExpr
        | K::DesignatedInitUpdateExpr
        | K::CXXFoldExpr => {
            opt(kids, id(n.a()));
            opt(kids, id(n.b()));
        }
        K::CompoundAssignOperator => {
            opt(kids, id(n.a()));
            kids.push(I::E(x.compound_assign(e).rhs));
        }
        K::ConditionalOperator => {
            let [t, f] = x.cond_arms(e);
            opt(kids, id(n.a()));
            kids.extend([I::E(t), I::E(f)]);
        }
        K::CallExpr | K::CXXMemberCallExpr | K::CXXOperatorCallExpr => {
            opt(kids, id(n.a()));
            list_raw(w, n.b(), kids);
        }
        K::PseudoObjectExpr => kids.push(I::E(x.pseudo_syntactic(e, &ast.lists))),
        K::CXXRewrittenBinaryOperator => {
            let (_, written) = x.rewritten_written(e);
            kids.extend(ast.lists.iter::<ExprId>(written).map(I::E));
        }
        K::BinaryConditionalOperator | K::ParenListExpr | K::InitListExpr => {
            list_raw(w, n.a(), kids);
        }
        K::DesignatedInitExpr => {
            for d in x.designators(e).to_vec() {
                match d {
                    Designator::Field { name: _, loc: _ } => {}
                    Designator::Index { index, .. } => kids.push(I::E(index)),
                    Designator::Range { first, last, .. } => {
                        kids.extend([I::E(first), I::E(last)]);
                    }
                }
            }
            opt(kids, id(n.a()));
        }
        K::CStyleCastExpr
        | K::CXXFunctionalCastExpr
        | K::CXXStaticCastExpr
        | K::CXXConstCastExpr
        | K::CXXReinterpretCastExpr
        | K::CXXDynamicCastExpr
        | K::BuiltinBitCastExpr => {
            kids.push(I::T(cast_written().unwrap_or(n.ty())));
            opt(kids, id(n.a()));
        }
        K::CXXUnresolvedConstructExpr => {
            kids.push(I::T(x.unresolved_construct_type(e)));
            list_raw(w, n.a(), kids);
        }
        K::CXXParenListInitExpr => {
            kids.push(I::T(n.ty()));
            list_raw(w, n.a(), kids);
        }
        K::CXXConstructExpr | K::CXXTemporaryObjectExpr => {
            kids.push(I::T(n.ty()));
            list_raw(w, n.b(), kids);
        }
        K::CompoundLiteralExpr => {
            kids.push(I::T(n.ty()));
            opt(kids, id(n.a()));
        }
        K::CXXNewExpr => {
            let ne = *x.new_extra(e);
            let chosen = vec![ne.operator_new, ne.operator_delete];
            if chosen.iter().any(Option::is_some) {
                record(e, true);
                push(w, Fact::Chosen(chosen));
            }
            kids.push(I::T(ne.allocated_ty));
            opt(kids, ne.array_size);
            opt(kids, ne.init);
            if let Some(p) = ne.placement {
                kids.extend(ast.lists.iter::<ExprId>(p).map(I::E));
            }
        }
        K::CXXThrowExpr => opt(kids, x.throw_operand(e)),
        K::CXXPseudoDestructorExpr => {
            let (base, _, destroyed) = x.pseudo_destructor(e);
            kids.extend([I::T(destroyed), I::E(base)]);
        }
        K::CXXTypeidExpr => match x.typeid_operand(e) {
            TypeidOperand::Type(t) => kids.push(I::T(t)),
            TypeidOperand::Expr(o) => kids.push(I::E(o)),
        },
        K::UnaryExprOrTypeTraitExpr => {
            if n.has(ExprFlags::ARG_IS_TYPE) {
                kids.extend(x.trait_type_arg(e).map(I::T));
            } else {
                opt(kids, id(n.a()));
            }
        }
        K::TypeTraitExpr => {
            kids.extend(ast.lists.iter::<QualType>(x.type_trait_args(e)).map(I::T));
        }
        K::ArrayTypeTraitExpr => {
            kids.push(I::T(x.array_trait_type(e)));
            opt(kids, x.array_trait(e).dim);
        }
        K::ExpressionTraitExpr => kids.push(I::E(x.expression_trait_operand(e))),
        K::RequiresExpr => match ast.requires_of(e).copied() {
            Some(r) => {
                for p in ast.lists.iter::<DeclId>(r.params) {
                    kids.extend(ast.decls.get(p).ty().map(I::T));
                }
                for req in ast.templates.requirements(&r).to_vec() {
                    match req {
                        Requirement::Simple(c) | Requirement::Nested(c) => kids.push(I::E(c)),
                        Requirement::Type(t, _position) => kids.push(I::T(t)),
                        Requirement::Compound { expr: c, noexcept: _, constraint } => {
                            kids.push(I::E(c));
                            opt(kids, constraint);
                        }
                    }
                }
            }
            None => no_spelling(w, s, ctx, "spelling/full"),
        },
        K::ConceptSpecializationExpr => {
            kids.extend(ArgListId::try_new(n.b()).map(I::A));
        }
        // Walked in full by profile mode (`expr_profile.rs`): their
        // found sets are in the bytes; a set left out of a type nested in
        // one is not read here (rule 5 when it is marked).
        K::CXXReflectExpr
        | K::CXXSpliceExpr
        | K::UserDefinedLiteral
        | K::ImaginaryLiteral
        | K::EmbedExpr
        | K::UnresolvedMemberExpr
        | K::SubstNonTypeTemplateParmPackExpr
        | K::FunctionParmPackExpr
        | K::CXXInheritedCtorInitExpr
        | K::CXXStdInitializerListExpr
        | K::OffsetOfExpr
        | K::LambdaExpr
        | K::CoawaitExpr
        | K::DependentCoawaitExpr
        | K::CoyieldExpr
        | K::StmtExpr
        | K::ChooseExpr
        | K::GenericSelectionExpr
        | K::VAArgExpr
        | K::AtomicExpr
        | K::ShuffleVectorExpr
        | K::ConvertVectorExpr
        | K::ExtVectorElementExpr
        | K::RecoveryExpr
        | K::CXXExpansionSelectExpr => no_spelling(w, s, ctx, "spelling/full"),
    }
}

/// Tests only: an expression whose non-empty set was read directly.
#[allow(unused_variables, reason = "tests only")]
fn record(e: ExprId, non_empty: bool) {
    #[cfg(test)]
    if non_empty && IN_TYPE.with(std::cell::Cell::get) == 0 {
        DIRECT.with(|d| d.borrow_mut().push(e));
    }
}

#[cfg(test)]
#[path = "spelling_tests.rs"]
mod tests;
