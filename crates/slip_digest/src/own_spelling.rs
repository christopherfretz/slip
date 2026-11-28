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

use slip_kiln::kiln::ast::decl::{DeclKind, FriendTarget};
use slip_kiln::kiln::ast::expr::{Designator, ExprDeps, ExprFlags, ExprKind as K, TypeidOperand};
use slip_kiln::kiln::ast::fp::FpRound;
use slip_kiln::kiln::ast::templates::{Requirement, SpecKind};
use slip_kiln::kiln::ast::types::{
    ArgView, ExcSpec, ExprOperand, Nns, NoexceptValue, TagKeyword, TemplateArg, TemplateName,
    TypeFlags, TypeOfOperand, TypeView,
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
    /// The profile bytes of an expression the written type holds that is
    /// instantiation-dependent and neither type- nor value-dependent: the
    /// canonical type holds its value, not the expression (an array bound,
    /// a template argument, a resolved `noexcept` operand).
    Profile(Vec<u8>),
    /// The rounding modes where the operations of one expression the unit
    /// reads were written, where one is not the default (`fp_round`): the
    /// expression's position among the expressions the unit reads, then
    /// each node's position in that expression's reading and its mode.
    Round(u64, Vec<(u64, FpRound)>),
}

/// The kind tags of a list entry (§4.4 "Spellings").
pub(crate) const FACT_FOUND: u64 = 1;
pub(crate) const FACT_OPER: u64 = 2;
pub(crate) const FACT_ALIAS: u64 = 3;
pub(crate) const FACT_CHOSEN: u64 = 4;
pub(crate) const FACT_PROFILE: u64 = 5;
pub(crate) const FACT_ROUND: u64 = 6;

impl Fact {
    /// Whether the fact makes a list due: a non-empty set, or an alias
    /// (pushed only when its own spelling makes a list due).
    fn due(&self) -> bool {
        match self {
            Fact::Found(ds) | Fact::Oper(ds) => !ds.is_empty(),
            Fact::Chosen(ds) => ds.iter().any(Option::is_some),
            Fact::Alias(_) | Fact::Profile(_) | Fact::Round(_, _) => true,
        }
    }
}

#[cfg(test)]
thread_local! {
    /// The expressions whose non-empty set a reading yielded directly
    /// (not through a type nested in the expression): the count test of
    /// rule 6 (`own_spelling_tests`).
    pub(crate) static DIRECT: std::cell::RefCell<Vec<ExprId>> =
        const { std::cell::RefCell::new(Vec::new()) };
    /// How many types the reading is inside.
    static IN_TYPE: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    /// Lists are not written (decision 2's test: what a list changes).
    pub(crate) static NO_LIST: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// How many lists were written.
    pub(crate) static LISTS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Display names of content-keyed units that wrote a list (decision 2's test).
    pub(crate) static CONTENT_LIST_NAMES: std::cell::RefCell<std::collections::BTreeSet<String>> =
        const { std::cell::RefCell::new(std::collections::BTreeSet::new()) };
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

/// Rule 10 in the full walk (§7 `DeclRefExtra`): what the qualifier of
/// resolved name `e` and its own template arguments hold beyond the
/// referent, read as written into the unit's list (rule 7 for an
/// instantiation-dependent operand, rule 1 for the rest), whatever Kiln's
/// flags say. Arguments of a name of a template are the bytes' (written
/// through `Walk::write_args`, a site already). Outside a unit stream
/// (in a type digest, a key, a profile) the facts are left out: the mark
/// is raised as profile mode raises it (`name_parts_left_out`).
pub(crate) fn name_parts(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let Some(&slip_kiln::kiln::ast::expr::DeclRefExtra { qualifier, template_args, found_decl: _ }) =
        w.ast.exprs.decl_ref_extra(e)
    else {
        return;
    };
    let node = *w.ast.exprs.get(e);
    // `DeclRefExpr.a` the entity, `MemberExpr.b` the member.
    let d = DeclId::try_new(if node.kind() == K::DeclRefExpr { node.a() } else { node.b() });
    let template_args =
        template_args.filter(|_| !d.is_some_and(|d| crate::expr_core::names_template(w, d)));
    if qualifier.is_none() && template_args.is_none() {
        return;
    }
    if w.unit_level() {
        if let Some(q) = qualifier {
            nns(w, s, q);
        }
        if let Some(l) = template_args {
            args(w, s, l, Ctx::Known(true));
        }
    } else if d.is_some_and(|d| name_parts_left_out(w, d)) {
        w.left_out += 1;
    }
}

/// Whether a reference to `d` in a profile may have had a qualifier or
/// template arguments that hold more than `d` (rule 10): a member of a
/// class or an enumeration (named through a type, which can be an
/// alias's or a specialisation's with any operand), or a specialisation.
/// A function of the referent, so of the profile, not of the spelling Kiln
/// met first.
pub(crate) fn name_parts_left_out(w: &Walk<'_>, d: DeclId) -> bool {
    let ast = w.ast;
    ast.spec_info(d).is_some()
        || ast.decls.get(d).ctx().is_some_and(|c| {
            matches!(
                ast.decls.get(c).kind(),
                DeclKind::CXXRecord
                    | DeclKind::ClassTemplateSpecialization
                    | DeclKind::ClassTemplatePartialSpecialization
                    | DeclKind::Enum
            )
        })
}

/// The left-out facts of written type `written`, added to the unit's list.
/// `own_fn`: `written` is the own type of function declaration `d`, whose
/// unit walks a dependent exception specification in full (`DeclNode`
/// `ty-exception-spec`) and writes each parameter's declaration, with
/// the parameter's type as written (`ParamData` `original_ty` for an
/// adjusted one): neither is read here.
pub(crate) fn left_out_of_type(
    w: &mut Walk<'_>,
    s: &mut Stream,
    written: QualType,
    own_fn: Option<DeclId>,
) {
    ty(w, s, written, own_fn);
}

/// The left-out facts of a template argument list `l`: a partial
/// specialisation's arguments (converted, sugared), with `written`, the
/// list as written, and `canonical`, the canonical list, where Kiln keeps
/// them (`args_as_written`): a written expression whose canonical
/// argument is a converted value is read as rule 12 reads one
/// (`converted_args`); otherwise the list `l` the unit holds.
pub(crate) fn left_out_of_args(
    w: &mut Walk<'_>,
    s: &mut Stream,
    l: ArgListId,
    written: Option<(ArgListId, ArgListId)>,
    marked: bool,
) {
    match written {
        Some((written, canonical)) => {
            converted_args(w, s, l, written, Some(canonical), Ctx::Known(marked));
        }
        None => args(w, s, l, Ctx::Known(marked)),
    }
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
        if let Some(e) = w.unit.local.content
            && let crate::keys::Role::Content(kind) =
                crate::keys::info(w.ast.decls.get(e).kind()).role
        {
            let name = crate::names::unit_name(w, e, kind);
            CONTENT_LIST_NAMES.with(|names| {
                names.borrow_mut().insert(name);
            });
        }
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
            Fact::Profile(bytes) => {
                s.uv(FACT_PROFILE).bytes(&bytes);
            }
            Fact::Round(root, modes) => {
                s.uv(FACT_ROUND).uv(root).count(modes.len());
                for (at, mode) in modes {
                    s.uv(at).tag(crate::tables::fp_round::row(&mode));
                }
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
    let roots = std::mem::take(&mut w.unit.roots);
    let depth = std::mem::take(&mut w.depth);
    let node_depth = std::mem::take(&mut w.node_depth);
    let mut scratch = Stream::new();
    ty(w, &mut scratch, underlying, None);
    w.depth = depth;
    w.node_depth = node_depth;
    let mine = w.unit.facts_due || scratch.cause().is_some();
    w.unit.facts = facts;
    w.unit.facts_due = due;
    w.unit.roots = roots;
    w.alias_due.insert(d, mine);
    mine
}

fn alias(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    // A block-scope typedef or alias has no unit of its own: its
    // declaration writes its aliased type into the enclosing unit
    // (`local::write_decl`), and a use reads that type again here, so
    // which of two such aliases a use names is in the list.
    if w.is_local(d) {
        if let Some(underlying) = w.ast.decls.typedef(d).map(|t| t.underlying) {
            ty(w, s, underlying, None);
        }
        return;
    }
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
fn ty(w: &mut Walk<'_>, s: &mut Stream, q: QualType, own_fn: Option<DeclId>) {
    let types = &w.ast.types;
    if !types.flags(q.ty()).intersects(TypeFlags::DEPENDENT_INSTANTIATION)
        && types.canonical(q) == q
    {
        // No profile in it, and no sugar: Kiln's flags do not hold the
        // instantiation dependence of a constant array's bound as written
        // or of a resolved `noexcept` operand (`ConstArray::size_expr`,
        // `TypeContext::constant_array`: the element's flags), so a type
        // with sugar is walked whatever its flags (rule 7).
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
        | TypeView::Record(_)
        | TypeView::Enum(_)
        | TypeView::UnresolvedUsing(_)
        | TypeView::TemplateTypeParm(_)
        | TypeView::CanonTemplateTypeParm { depth: _, index: _, pack: _ }
        | TypeView::VariableArray { elem: _, size: _ }
        | TypeView::InjectedClassName { pattern: _, injected: _ }
        | TypeView::Splice { operand: _, designated: _ } => {}
        TypeView::DependentBitInt { signed: _, width: _ } => {
            // Kiln interns the node with the first spelling's operand.
            no_spelling(w, s, here, "spelling/bit-int");
        }
        TypeView::BitInt { signed: _, width: _ } => {
            // An instantiation-dependent width Kiln computed: the written
            // expression is not held.
            unreachable_operand(w, s, q, "spelling/bit-int-width");
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
        } => ty(w, s, x, None),
        TypeView::QualifiedArray { quals: _, base } => ty(w, s, QualType::unqual(base), None),
        TypeView::Paren(x) => ty(w, s, x, own_fn),
        TypeView::Attributed { kind: _, modified, equivalent: _ } => ty(w, s, modified, own_fn),
        // A parameter type adjusted from an array whose bound is an
        // instantiation-dependent expression (Kiln WP-2720): the declared
        // array type holds the bound as written (rule 7); the adjusted
        // pointer has lost it.
        TypeView::Decayed { original, adjusted: _ } => ty(w, s, original, None),
        TypeView::ConstantArray(a) => {
            let slip_kiln::kiln::ast::types::ConstArray { elem, size: _, size_expr } = a;
            ty(w, s, elem, None);
            match size_expr {
                Some(e) => written_expr(w, s, e, here),
                None => {
                    if !types.flags(elem.ty()).intersects(TypeFlags::DEPENDENT_INSTANTIATION) {
                        unreachable_operand(w, s, q, "spelling/array-size");
                    }
                }
            }
        }
        TypeView::MemberPointer { pointee, class } => {
            ty(w, s, pointee, None);
            ty(w, s, QualType::unqual(class), None);
        }
        TypeView::DependentSizedArray { elem, size } => {
            ty(w, s, elem, None);
            operand(w, s, size, here, "spelling/array-bound");
        }
        TypeView::DependentVector { elem, size, kind: _ } => {
            ty(w, s, elem, None);
            operand(w, s, size, here, "spelling/vector-size");
        }
        TypeView::DependentAddrSpace { pointee, space } => {
            ty(w, s, pointee, None);
            operand(w, s, space, here, "spelling/addr-space");
        }
        TypeView::Function(p) => {
            ty(w, s, p.ret(), None);
            let params = w.ast.types.params(&p).to_vec();
            // The parameters an own type's declaration writes, each with
            // its type as written (`decl_func::function_data`): an
            // adjusted parameter type holds the canonical element type.
            let declared = own_fn
                .and_then(|d| w.ast.decls.function(d))
                .map_or(0, |f| w.ast.lists.iter::<DeclId>(f.params).count());
            for x in params.into_iter().skip(declared) {
                ty(w, s, x, None);
            }
            let e = w.ast.types.exc_spec(&p);
            exc(w, s, e, here, own_fn.is_some());
        }
        TypeView::Typedef { decl, underlying: _ } => alias(w, s, decl),
        TypeView::Using { shadow, underlying } => {
            let target = w.ast.strip_shadow(shadow);
            if w.ast.decls.typedef(target).is_some() {
                alias(w, s, target);
            } else {
                ty(w, s, underlying, None);
            }
        }
        TypeView::Elaborated { keyword: _, nns: n, named } => {
            if let Some(n) = n {
                nns(w, s, n);
            }
            ty(w, s, named, None);
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
                    alias(w, s, pattern);
                }
                args(w, s, l, here);
            } else if let Some(u) = underlying {
                // With `underlying` (a specialisation that is not
                // dependent, or a dependent one whose canonical form holds
                // converted values, Kiln WP-3321), the canonical type holds
                // the arguments' values: an instantiation-dependent one is
                // read as written, its profile with it (`written_expr`),
                // and so is a converted one (`converted_args`, rule 12).
                let canonical = dependent_canonical_args(w, u);
                template_name(w, s, name);
                converted_args(w, s, l, l, canonical, here);
            } else {
                // A canonical dependent specialisation with no spelling of
                // its own: a converted value argument hides which entity
                // was written (WP-163; `CONVERTED_ARGUMENT`), inside a pack
                // too (Kiln converts no pack's arguments today).
                if types.flags(q.ty()).intersects(TypeFlags::DEPENDENT) && holds_value(w, l) {
                    s.indeterminate(CONVERTED_ARGUMENT);
                }
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
                ty(w, s, pattern, None);
                operand(w, s, index, here, "spelling/pack-index");
            }
        }
        TypeView::Decltype { expr: e, underlying: _ } => expr(w, s, e, here),
        TypeView::DependentDecltype(_) => no_spelling(w, s, here, "spelling/decltype"),
        TypeView::TypeOf { operand: op, underlying: _, unqual: _ } => match op {
            TypeOfOperand::Expr(e) => expr(w, s, e, here),
            TypeOfOperand::Type(t) => ty(w, s, t, None),
            TypeOfOperand::Profile(_) => no_spelling(w, s, here, "spelling/typeof"),
        },
        TypeView::UnaryTransform { kind: _, base, result } => match result {
            None => ty(w, s, base, None),
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

/// Whether expression `e` is instantiation-dependent and neither type- nor
/// value-dependent: a type holding it is canonical by its value, and not
/// by a profile (digest-kinds §4.4 "Spellings", rule 7).
fn instantiation_only(w: &Walk<'_>, e: ExprId) -> bool {
    let d = w.ast.exprs.get(e).deps();
    d.contains(ExprDeps::INSTANTIATION)
        && !d.intersects(ExprDeps::TYPE)
        && !d.intersects(ExprDeps::VALUE)
}

/// A dependent class template-id's canonical type whose arguments hold a
/// converted value where the template-id was written with an expression
/// (Kiln WP-3321, `ea9a8e141`, in the pin since WP-163: `It<T, n::k>` is
/// `It<T, 0>`), met where the unit holds no written form of it: which
/// entity the argument named is not known, so the unit is indeterminate
/// (`spec/digest-kinds.md` §4.4 "Spellings", rule 12).
pub(crate) const CONVERTED_ARGUMENT: Cause = Cause::Unaccounted("spelling/converted-argument");

/// The canonical argument list of `underlying`, the canonical form of a
/// written template-id, where rule 12 compares the written list with it:
/// a dependent specialisation's (Kiln WP-3321: there the canonical type
/// no longer says which entity a written expression names, which the
/// profile of the written expression did at the pin before). None for a
/// specialisation that is not dependent (its canonical type is its
/// specialisation's record at both pins, digest-kinds §4.6) and for a
/// canonical form that is not a specialisation (a partial
/// specialisation's current instantiation, whose canonical arguments are
/// the partial specialisation's own: they are compared where the list is
/// written, `left_out_of_args`, and an out-of-line member's qualifier,
/// which Kiln does not keep, is `out_of_line_member`'s).
fn dependent_canonical_args(w: &Walk<'_>, underlying: QualType) -> Option<ArgListId> {
    let types = &w.ast.types;
    if !types.flags(underlying.ty()).intersects(TypeFlags::DEPENDENT) {
        return None;
    }
    match types.view(types.canonical(underlying).ty()) {
        TypeView::TemplateSpecialization { args: cl, .. } => Some(cl),
        _ => None,
    }
}

/// Rule 12 (digest-kinds §4.4 "Spellings"), the one reader of a list
/// whose canonical form holds converted values: list `read` position by
/// position (`read` is `written` itself for a written type, and a
/// partial specialisation's converted list, which has the canonical
/// list's shape, for its own list), where `written` (the list as
/// written) holds an expression and `canonical` a converted value, the
/// written expression with its profile (`profiled_expr`); elsewhere
/// `read`'s argument, as `arg` reads it.
///
/// Before a pack the three lists hold one argument per parameter, and
/// the written list is shorter only where defaults fill the canonical
/// one, which the template's own unit reads (its default template
/// argument): the zip stops at the shorter. A canonical argument that
/// is a pack holding a converted value (a partial specialisation of a
/// variadic template, `P<T, n::k>` with `template<class, int...>`:
/// the canonical list is `T, {0}`) covers the rest of the written list
/// (a class or variable template's pack is its last parameter): when
/// the written arguments from that position are exactly as many as the
/// pack's elements, each is read against its element as above, a
/// written pack expansion over a converted value excepted; otherwise
/// which written argument names which element is not known and the
/// form is lost (`lost_form`, `CONVERTED_ARGUMENT`), never equal. A
/// written dependent type's canonical list holds no such pack (Kiln
/// converts no dependent variadic template-id's arguments).
fn converted_args(
    w: &mut Walk<'_>,
    s: &mut Stream,
    read: ArgListId,
    written: ArgListId,
    canonical: Option<ArgListId>,
    here: Ctx,
) {
    let types = &w.ast.types;
    let read = types.args(read).to_vec();
    let written = types.args(written).to_vec();
    let canonical = canonical.map_or_else(Vec::new, |c| types.args(c).to_vec());
    for (i, a) in read.into_iter().enumerate() {
        let c = canonical.get(i).map(TemplateArg::view);
        if let Some(ArgView::Pack(p)) = c
            && holds_value(w, p)
        {
            let elements = w.ast.types.args(p).to_vec();
            let rest = written.get(i..).unwrap_or(&[]);
            let aligned = rest.len() == elements.len()
                && rest
                    .iter()
                    .zip(&elements)
                    .all(|(x, c)| !(expansion(w, *x) && matches!(c.view(), ArgView::Value { .. })));
            if !aligned {
                lost_form(w, s, CONVERTED_ARGUMENT);
                arg(w, s, a, here);
                return;
            }
            for (x, c) in rest.iter().zip(&elements) {
                one_converted(w, s, *x, *x, c.view(), here);
            }
            return;
        }
        let x = written.get(i).copied().unwrap_or(a);
        match c {
            Some(c) => one_converted(w, s, a, x, c, here),
            None => arg(w, s, a, here),
        }
    }
}

/// One position of `converted_args`: argument `written` over canonical
/// argument `canonical`, read as rule 12 reads it where `written` is an
/// expression and `canonical` a converted value; otherwise `held`, the
/// argument the unit holds.
fn one_converted(
    w: &mut Walk<'_>,
    s: &mut Stream,
    held: TemplateArg,
    written: TemplateArg,
    canonical: ArgView,
    here: Ctx,
) {
    match (written.view(), canonical) {
        (ArgView::Expr { expr, profile: _ }, ArgView::Value { .. }) => profiled_expr(w, s, expr),
        _ => arg(w, s, held, here),
    }
}

/// Whether written argument `a` is a pack expansion (`Ns...`,
/// `Ts...`, `TT...`).
fn expansion(w: &Walk<'_>, a: TemplateArg) -> bool {
    match a.view() {
        ArgView::Expr { expr: e, profile: _ } => w.ast.exprs.get(e).kind() == K::PackExpansionExpr,
        ArgView::Type(q) => {
            matches!(w.ast.types.view(q.ty()), TypeView::PackExpansion { .. })
        }
        ArgView::TemplateExpansion { .. } => true,
        _ => false,
    }
}

/// A written argument expression `e` whose canonical form is a converted
/// value (`converted_args`): its profile bytes added to the list
/// first, then read as written, as an instantiation-dependent one is
/// (rule 7, `written_expr`).
fn profiled_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let mut p = Stream::new();
    let left_out = w.left_out;
    w.spelling_off += 1;
    crate::expr_dep::profile_expr(w, &mut p, e);
    w.spelling_off -= 1;
    let marked = w.left_out != left_out;
    w.left_out = left_out;
    s.absorb(p.cause());
    push(w, Fact::Profile(p.as_bytes().to_vec()));
    expr(w, s, e, Ctx::Known(marked));
}

/// An expression a written type holds where the canonical type holds a
/// value (an array bound, a non-dependent specialisation's argument, a
/// resolved `noexcept` operand): read as written, and, when it is
/// instantiation-dependent, its profile bytes added to the list first
/// (rule 7). A dependent one is read as any written operand.
fn written_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId, ctx: Ctx) {
    if !instantiation_only(w, e) {
        expr(w, s, e, ctx);
        return;
    }
    let mut p = Stream::new();
    let left_out = w.left_out;
    // Not the unit's own reading of `e`: that follows, below.
    w.spelling_off += 1;
    crate::expr_dep::profile_expr(w, &mut p, e);
    w.spelling_off -= 1;
    let marked = w.left_out != left_out;
    w.left_out = left_out;
    s.absorb(p.cause());
    push(w, Fact::Profile(p.as_bytes().to_vec()));
    expr(w, s, e, Ctx::Known(marked));
}

/// A type that is instantiation-dependent through an operand Kiln does
/// not keep as written (a value it computed): `Unaccounted(site)` when the
/// type is instantiation-dependent and not dependent, the operand then
/// being the only source of that.
fn unreachable_operand(w: &Walk<'_>, s: &mut Stream, q: QualType, site: &'static str) {
    let f = w.ast.types.flags(q.ty());
    if f.intersects(TypeFlags::INSTANTIATION_DEPENDENT) && !f.intersects(TypeFlags::DEPENDENT) {
        s.indeterminate(Cause::Unaccounted(site));
    }
}

/// An expression operand of a type: its expression as written, or rule 5
/// for a canonical one.
fn operand(w: &mut Walk<'_>, s: &mut Stream, op: ExprOperand, ctx: Ctx, site: &'static str) {
    match op {
        ExprOperand::Expr(e) => expr(w, s, e, ctx),
        ExprOperand::Profile(_) => no_spelling(w, s, ctx, site),
    }
}

/// A function type's exception specification as written. `own`: the
/// declaration's own, whose dependent operand its unit walks in full.
fn exc(w: &mut Walk<'_>, s: &mut Stream, e: ExcSpec, ctx: Ctx, own: bool) {
    match e {
        ExcSpec::NoexceptExpr { expr: x, value: NoexceptValue::Dependent(_) } => {
            if !own {
                expr(w, s, x, ctx);
            }
        }
        ExcSpec::DependentNoexcept(_) => no_spelling(w, s, ctx, "spelling/noexcept"),
        ExcSpec::Dynamic(list) => {
            let ts = w.ast.types.type_list_types(list).to_vec();
            for t in ts {
                ty(w, s, t, None);
            }
        }
        // A resolved operand: the type holds its value only.
        ExcSpec::NoexceptExpr { expr: x, value: NoexceptValue::False | NoexceptValue::True } => {
            written_expr(w, s, x, ctx);
        }
        ExcSpec::None
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
        match w.ast.types.nns_view(n) {
            // A namespace (through an alias or not) and `::` hold nothing
            // beyond the entity, which the canonical qualifier writes; an
            // identifier's name is the canonical qualifier's too (its
            // prefix is a level of its own).
            Nns::Global
            | Nns::Namespace { prefix: _, ns: _ }
            | Nns::NamespaceAlias { prefix: _, alias: _, ns: _ }
            | Nns::Identifier { prefix: _, name: _ } => {}
            // A type as written (rule 2: a class, an enumeration, an
            // alias, a specialisation, `T`, `T::X`, `T::template X<..>`,
            // `decltype(e)`, `Ts...[i]`).
            Nns::Type { prefix: _, ty: t, template_kw: _ } => ty(w, s, t, None),
            // A level the reading does not know: never skipped.
            Nns::Splice(_) => {
                s.indeterminate(Cause::Unaccounted("spelling/nns-splice"));
            }
        }
    }
}

/// The left-out facts of a qualifier written in an expression the full
/// walk writes (`Walk::write_written_nns`: an unresolved name's,
/// `DependentScopeDeclRefExpr`'s, a dependent member access's), read as
/// written into the unit's list (rule 11).
pub(crate) fn left_out_of_nns(w: &mut Walk<'_>, s: &mut Stream, n: NnsId) {
    nns(w, s, n);
}

/// A qualified pseudo-destructor name (`p->X::I::~I()`): Kiln keeps that
/// it was qualified and not the qualifier (rule 11), which can hold an
/// instantiation-dependent operand.
pub(crate) const PSEUDO_DESTRUCTOR_QUALIFIER: Cause =
    Cause::Unaccounted("spelling/pseudo-destructor-qualifier");

/// A qualified pseudo-destructor in the full walk (rule 11), in a
/// templated stream ([`templated_at`]): in a unit stream
/// `spelling/pseudo-destructor-qualifier`; elsewhere the mark.
pub(crate) fn pseudo_destructor(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    if !qualifier_lost(w, e) {
        return;
    }
    if w.unit_level() {
        s.indeterminate(PSEUDO_DESTRUCTOR_QUALIFIER);
    } else {
        w.left_out += 1;
    }
}

/// A qualified pseudo-destructor whose qualifier Kiln did not keep: it
/// keeps it for a dependent qualifier only, as `TemplateTables` `expr_nns`
/// or as the first type-name `pseudo_dtor_first` with the rest in the
/// destroyed type (`kiln_sema` `class/member_expr.rs`), which
/// `templ::expr_maps` and rule 1 read. Otherwise it keeps the bit, and
/// computes the node's dependence without the qualifier, so in a
/// templated stream the qualifier can hold an instantiation-dependent
/// operand no field has.
fn qualifier_lost(w: &Walk<'_>, e: ExprId) -> bool {
    let t = &w.ast.templates;
    w.ast.exprs.pseudo_destructor_qualified(e)
        && t.expr_nns.get(e).is_none()
        && t.pseudo_dtor_first.get(e).is_none()
        && templated_at(w, None)
}

/// Rule 11's one question, asked by every check that decides between
/// reading a form as written and making it indeterminate: can the form,
/// written in the stream being written (and, when it is a declaration's,
/// as the declaration `d`), hold an instantiation-dependent operand? It
/// can when the stream is templated or `d` is ([`templated`]); neither
/// half alone is asked anywhere. A lambda's or a local class's
/// declaration chain ends at the namespace its closure lives in while
/// its stream's holder is the variable template, the default argument's
/// function template or the concept it is written in; an out-of-line
/// member's stream holder is itself, reached through its class.
pub(crate) fn templated_at(w: &Walk<'_>, d: Option<DeclId>) -> bool {
    templated_stream(w) || d.is_some_and(|d| templated(w, d))
}

/// The stream being written can hold an instantiation-dependent operand:
/// its holder is templated ([`templated`]), or the walk is inside a
/// generic lambda's body, whose call operator is a template
/// (`local::lambda_expr`). A content digest's stream asks it of the
/// content entity; a stream with neither counts as one: nothing says
/// otherwise. Asked only through [`templated_at`].
fn templated_stream(w: &Walk<'_>) -> bool {
    let Some(h) = w.unit.local.holder.or(w.unit.local.content) else { return true };
    w.unit.local.generic > 0 || templated(w, h)
}

/// A destructor named by member access as written (`q->~X<..>()`): Kiln
/// resolves it to the destructor and keeps neither the type-name nor its
/// arguments, which can hold an instantiation-dependent operand.
pub(crate) const DESTRUCTOR_NAME: Cause = Cause::Unaccounted("spelling/destructor-name");

/// `d` is a destructor.
pub(crate) fn is_destructor(w: &Walk<'_>, d: DeclId) -> bool {
    w.ast.decls.get(d).kind() == DeclKind::CXXDestructor
}

/// A destructor named by member access in the full walk (rule 11), in a
/// templated stream: in a unit stream `spelling/destructor-name`;
/// elsewhere the mark.
pub(crate) fn destructor_name(w: &mut Walk<'_>, s: &mut Stream, _: ExprId) {
    if !templated_at(w, None) {
        return;
    }
    if w.unit_level() {
        s.indeterminate(DESTRUCTOR_NAME);
    } else {
        w.left_out += 1;
    }
}

/// A using-declaration's qualifier as written (rule 11): its key writes
/// the canonical qualifier (`KeyKind::UsingDeclaration`), its stream
/// nothing more, so what the written one holds is read into the list.
pub(crate) fn using_qualifier(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    if !w.unit_level() {
        return;
    }
    if let Some(q) = w.ast.using_qualifier(d) {
        nns(w, s, q);
    }
}

/// A `using enum` in a templated stream or declaration (rule 11,
/// [`templated_at`]):
/// Kiln keeps the enumeration and not the qualifier or alias it was named
/// through (`Sema::act_on_using_enum`, `kiln_sema/src/using.rs`), which
/// can hold an instantiation-dependent operand only there, so the unit is
/// indeterminate rather than equal.
pub(crate) fn using_enum(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    if templated_at(w, Some(d)) {
        lost_form(w, s, USING_ENUM);
    }
}

/// `spelling/using-enum` (rule 11).
pub(crate) const USING_ENUM: Cause = Cause::Unaccounted("spelling/using-enum");

/// A friend declaration naming a function or class through a qualifier
/// or with template arguments (`friend void Bq<..>::g();`, `friend
/// struct Bq<..>::Y;`, `friend void h<..>();`): Kiln resolves a qualified
/// one to the befriended declaration or its unsugared type and keeps no
/// qualifier, and keeps only the function type of a specialisation's.
pub(crate) const FRIEND_NAME: Cause = Cause::Unaccounted("spelling/friend-name");

/// A friend whose written name Kiln did not keep (rule 11, [`FRIEND_NAME`]),
/// in a templated stream or declaration ([`templated_at`]):
/// `spelling/friend-name` in a unit stream; elsewhere the mark. Lost:
/// a befriended declaration that is a class member (named through a
/// qualifier, which can be a specialisation, an alias or a `decltype`)
/// or a template specialisation (named with arguments); a befriended
/// class or enumeration type with no sugar that is either; a befriended
/// function type (Kiln's `Type` target for `friend void h<..>();`). A
/// friend found by an unqualified name, or a namespace member named
/// through namespaces only, holds no operand; a type with sugar is read
/// as written (rule 2).
pub(crate) fn friend_name(w: &mut Walk<'_>, s: &mut Stream, d: DeclId, target: FriendTarget) {
    let ast = w.ast;
    let named = |x: DeclId| {
        ast.spec_info(x).is_some()
            || ast.decls.get(x).ctx().is_some_and(|c| {
                !matches!(
                    ast.decls.get(c).kind(),
                    DeclKind::TranslationUnit
                        | DeclKind::Namespace
                        | DeclKind::LinkageSpec
                        | DeclKind::Export
                )
            })
    };
    let lost = match target {
        FriendTarget::Decl(x) => named(x),
        FriendTarget::Type { ty, owned_tag: _ } => {
            // An elaborated-type-specifier with a class-key (`friend
            // struct Bq<..>::Y;`) is the class itself. At `4a83f6e3c` Kiln
            // built it without its qualifier; since WP-3164 (`2af5deebd`)
            // the type keeps the written qualifier, which this rule does
            // not read (parity, WP-163: reading a friend's written form is
            // WP-126's). A `typename` one (`friend typename Bq<..>::Y;`)
            // kept its qualifier at both pins and is sugar, read as written.
            let mut view = ast.types.view(ty.ty());
            if let TypeView::Elaborated { keyword, nns, named } = view
                && (nns.is_none()
                    || matches!(
                        keyword,
                        TagKeyword::Struct
                            | TagKeyword::Class
                            | TagKeyword::Union
                            | TagKeyword::Enum
                    ))
            {
                view = ast.types.view(named.ty());
            }
            match view {
                TypeView::Record(x) | TypeView::Enum(x) => named(x),
                TypeView::Function(_) => true,
                _ => false,
            }
        }
    };
    if lost && templated_at(w, Some(d)) {
        lost_form(w, s, FRIEND_NAME);
    }
}

/// An explicit specialisation declared in a template (`template<> void
/// h<..>()`, `template<> struct Z<..> {}` inside a class template): Kiln
/// keeps its converted arguments, with an argument's value in place of
/// the expression written, which can hold an instantiation-dependent
/// operand.
pub(crate) const SPECIALISATION_ARGUMENTS: Cause =
    Cause::Unaccounted("spelling/specialisation-arguments");

/// An explicit specialisation (rule 11) whose declaration is templated
/// ([`templated_at`], so a member of a template's pattern): its written
/// arguments are lost, `spelling/specialisation-arguments` in a unit
/// stream; elsewhere the mark. One at namespace scope of a specialised
/// class is no template's: Kiln resolves its arguments where written.
pub(crate) fn explicit_specialisation(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    let ast = w.ast;
    let explicit =
        ast.spec_info(d).is_some_and(|(_, i)| i.kind == SpecKind::ExplicitSpecialization);
    if explicit && templated_at(w, Some(d)) {
        lost_form(w, s, SPECIALISATION_ARGUMENTS);
    }
}

/// Rule 12 at a qualifier Kiln does not keep: declaration `d` written
/// outside a partial specialisation it is a member of (an out-of-line
/// member function, static data member or nested class, or a member of
/// one, `template<class T> void P<T, n::k>::f() {}`), whose canonical
/// arguments hold a converted value. The qualifier's template-id named
/// the partial specialisation by its written arguments, which Kiln keeps
/// nowhere on `d` (only a using-declaration's qualifier is kept), so
/// which entity a written argument named is not known: the unit is
/// indeterminate (`CONVERTED_ARGUMENT`), never equal. The semantic chain
/// is walked up to the first class `d` is written in: a qualifier above
/// it was written by that class's declaration, and is its unit's.
pub(crate) fn out_of_line_member(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    let ast = w.ast;
    let decls = &ast.decls;
    let mut lexical: Vec<DeclId> = Vec::new();
    let mut x = decls.lexical_ctx(d);
    while let Some(c) = x {
        if lexical.len() >= 4096 {
            break;
        }
        lexical.push(ast.first(c));
        x = decls.lexical_ctx(c);
    }
    let mut x = decls.get(d).ctx();
    let mut steps = 0;
    while let Some(c) = x {
        steps += 1;
        if steps > 4096
            || matches!(decls.get(c).kind(), DeclKind::TranslationUnit | DeclKind::Namespace)
            || lexical.contains(&ast.first(c))
        {
            return;
        }
        let info = ast.partial_spec_info(c).or_else(|| ast.partial_spec_info(ast.first(c)));
        if let Some((_, info)) = info
            && holds_value(w, info.canonical)
        {
            lost_form(w, s, CONVERTED_ARGUMENT);
            return;
        }
        x = decls.get(c).ctx();
    }
}

/// Whether list `l` holds a converted value, inside a pack too.
fn holds_value(w: &Walk<'_>, l: ArgListId) -> bool {
    w.ast.types.args(l).iter().any(|a| match a.view() {
        ArgView::Value { .. } => true,
        ArgView::Pack(p) => holds_value(w, p),
        _ => false,
    })
}

/// A form whose written spelling Kiln did not keep, in a templated
/// stream: `cause` in a unit stream; elsewhere the mark.
fn lost_form(w: &mut Walk<'_>, s: &mut Stream, cause: Cause) {
    if w.unit_level() {
        s.indeterminate(cause);
    } else {
        w.left_out += 1;
    }
}

/// Rule 11's one notion of code that can hold an instantiation-dependent
/// operand: `d` is templated when it, or a declaration on its semantic
/// or its lexical context chain (each step taking either context, so an
/// out-of-line definition is reached through its class and a friend
/// defined in a class through the class it is written in), is a
/// template, a template's pattern or a partial specialisation. Kiln
/// records no template parameter lists on an out-of-line member
/// (`template<class T> void S<T>::h()` is a `CXXMethod` whose semantic
/// context is the pattern of `S`), so the semantic chain is what tells
/// it. A chain that does not end within the bound is templated: what
/// cannot be decided is never taken as not templated.
fn templated(w: &Walk<'_>, d: DeclId) -> bool {
    let decls = &w.ast.decls;
    let mut todo = vec![d];
    let mut seen: Vec<DeclId> = Vec::new();
    while let Some(x) = todo.pop() {
        if seen.contains(&x) {
            continue;
        }
        if seen.len() >= 4096 {
            return true;
        }
        seen.push(x);
        if templated_decl(w, x) {
            return true;
        }
        if matches!(decls.get(x).kind(), DeclKind::TranslationUnit | DeclKind::Namespace) {
            continue;
        }
        todo.extend(decls.get(x).ctx());
        todo.extend(decls.lexical_ctx(x));
    }
    false
}

/// `d` is a template (the declaration a unit of one is held by), a
/// template's pattern (`described_template`) or a partial
/// specialisation (`partial_spec_info`).
fn templated_decl(w: &Walk<'_>, d: DeclId) -> bool {
    matches!(
        w.ast.decls.get(d).kind(),
        DeclKind::ClassTemplate
            | DeclKind::FunctionTemplate
            | DeclKind::VarTemplate
            | DeclKind::TypeAliasTemplate
            | DeclKind::Concept
            | DeclKind::ClassTemplatePartialSpecialization
            | DeclKind::VarTemplatePartialSpecialization
    ) || w.ast.described_template(d).is_some()
        // Kiln gives a partial specialisation the kind `CXXRecord` or
        // `Var`; its template link says what it is.
        || w.ast.partial_spec_info(d).is_some()
}

/// Whether a qualifier in a profile may hold more than its canonical form
/// (rule 11): a level that names a type, whose written form can be an
/// alias, a specialisation or a `decltype` with any operand, or a level
/// the reading does not know. A function of the canonical qualifier, so
/// of the profile: every written level has a canonical level of the same
/// kind (a type's is a type).
pub(crate) fn nns_left_out(w: &Walk<'_>, n: NnsId) -> bool {
    let types = &w.ast.types;
    let mut cur = Some(types.canonical_nns(n));
    let mut depth = 0usize;
    while let Some(n) = cur {
        depth += 1;
        if depth > 4096 {
            return true;
        }
        cur = match types.nns_view(n) {
            Nns::Type { prefix: _, ty: _, template_kw: _ } | Nns::Splice(_) => return true,
            Nns::Namespace { prefix, ns: _ }
            | Nns::NamespaceAlias { prefix, alias: _, ns: _ }
            | Nns::Identifier { prefix, name: _ } => prefix,
            Nns::Global => None,
        };
    }
    false
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
        ArgView::Type(q) => ty(w, s, q, None),
        ArgView::Template(n) | ArgView::TemplateExpansion { name: n, num_expansions: _ } => {
            template_name(w, s, n);
        }
        ArgView::Expr { expr: e, profile: _ } => written_expr(w, s, e, ctx),
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
    // Rule 8: the rounding modes of this reading, by node position.
    let at = w.unit.roots;
    w.unit.roots += 1;
    let mut nodes = 0u64;
    let mut modes = Vec::new();
    let mut stack = vec![I::E(root)];
    while let Some(i) = stack.pop() {
        match i {
            I::E(e) => {
                nodes += 1;
                if let Some(r) = w.ast.exprs.fp_round(e).filter(|&r| r != FpRound::Dynamic) {
                    modes.push((nodes, r));
                }
                let mut kids = Vec::new();
                node(w, s, e, ctx, &mut kids);
                stack.extend(kids.into_iter().rev());
            }
            I::T(q) => ty(w, s, q, None),
            I::A(l) => args(w, s, l, ctx),
            I::N(n) => nns(w, s, n),
        }
    }
    if !modes.is_empty() {
        push(w, Fact::Round(at, modes));
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
    // The set a dependent operator or fold expression's lookup found at
    // the definition (`oper_lookups`, digest-kinds §9.5): left out of
    // every profile.
    if let Some(cands) = ast.templates.oper_lookups.of(e) {
        let ds: Vec<DeclId> = cands.iter().map(|c| c.decl).collect();
        record(e);
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
            // The qualifier and the arguments as written: what they hold
            // beyond the referent (an instantiation-dependent operand) is
            // the unit's (rule 10); the prefix itself is not a fact.
            if let Some(q) = x.decl_ref_extra(e).and_then(|r| r.qualifier) {
                kids.push(I::N(q));
            }
            if let Some(l) = x.decl_ref_extra(e).and_then(|r| r.template_args) {
                kids.push(I::A(l));
            }
            // An overloaded member's qualifier and arguments (rule 11).
            if let Some(m) = x.cold.member_overload.get(e) {
                kids.extend(m.qualifier.map(I::N));
                kids.extend(m.template_args.map(I::A));
            }
            if k == K::MemberExpr
                && DeclId::try_new(n.b()).is_some_and(|d| is_destructor(w, d))
                && templated_at(w, None)
            {
                s.indeterminate(DESTRUCTOR_NAME);
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
                record(e);
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
                record(e);
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
                record(e);
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
            // Rule 11: Kiln keeps that the name was qualified, not the
            // qualifier (`pseudo_destructor_qualified`).
            if qualifier_lost(w, e) {
                s.indeterminate(PSEUDO_DESTRUCTOR_QUALIFIER);
            }
            kids.extend(ast.templates.expr_nns.get(e).copied().map(I::N));
            kids.extend(ast.templates.pseudo_dtor_first.get(e).map(|&(q, _)| I::T(q)));
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
        | K::CXXExpansionSelectExpr => full(w, s, e),
    }
}

/// A kind profile mode walks in full: its bytes hold its own facts, and
/// what a type nested in it left out is not read here, so it is
/// `spelling/full` when its own walk left something out (the mark of the
/// node, not of the type around it).
fn full(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let left_out = w.left_out;
    let saved = w.take_unit();
    w.in_key += 1;
    crate::expr_dep::profile_expr(w, &mut Stream::new(), e);
    w.in_key -= 1;
    w.unit = saved;
    let marked = w.left_out != left_out;
    w.left_out = left_out;
    if marked {
        s.indeterminate(Cause::Unaccounted("spelling/full"));
    }
}

/// Tests only: an expression whose fact was read directly.
#[allow(unused_variables, reason = "tests only")]
fn record(e: ExprId) {
    #[cfg(test)]
    if IN_TYPE.with(std::cell::Cell::get) == 0 {
        DIRECT.with(|d| d.borrow_mut().push(e));
    }
}

#[cfg(test)]
#[path = "own_spelling_tests.rs"]
mod tests;
