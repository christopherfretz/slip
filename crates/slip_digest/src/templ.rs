//! Templates and specialisations (`spec/digest-kinds.md` §9, digest §8):
//! template heads, `TemplateInfo` and `TemplateParmInfo` (§9.2), the
//! links and specialisation records (§9.1, §9.4), and the rows of
//! `TemplateTables` keyed by a declaration or an expression (§9.5).
//! Decisions the specs left open: `spec/landed/digest-kinds/WP-029.md`.

use slip_kiln::kiln::ast::decl::{DeclKind, TemplLink, TemplLinkKind};
use slip_kiln::kiln::ast::templates::{
    MemberSpecInfo, PartialSpecInfo, SpecInfo, SpecKind, TemplateParmInfo,
};
use slip_kiln::kiln::ast::types::{ArgView, TemplateArg};
use slip_kiln::kiln::ast::{DeclId, ExprId, TemplateParamListId};

use crate::keys::{Role, info};
use crate::refs::{REF_NONE, REF_PARAM};
use crate::stream::{FlagWord, Stream};
use crate::tables::{arg_view, spec_kind, templ_link_kind, ttp_kind};
use crate::walk::{TOO_DEEP, Walk};
use crate::{Area, Cause};

/// The template head of a discriminator (§9, §3.3): `record
/// TemplateParamList` with each parameter as its `DeclNode` rows (which
/// bring in its `TemplateParmInfo`) and nothing else, so that a key does
/// not change when a default argument is added.
pub(crate) fn template_head(w: &mut Walk<'_>, s: &mut Stream, l: TemplateParamListId) {
    head(w, s, l, Head::Key);
}

/// `record TemplateParamList` (§9.2) as a unit writes it: the parameters
/// as `decl*`, each its whole stream (§5.1), which closes with the rows
/// of `TemplateTables` keyed by the parameter: its default argument
/// (`decl_maps`).
pub(crate) fn template_head_unit(w: &mut Walk<'_>, s: &mut Stream, l: TemplateParamListId) {
    head(w, s, l, Head::Unit(&[]));
}

/// How a head is written.
#[derive(Clone, Copy)]
enum Head<'h> {
    /// A key's: `DeclNode` rows only.
    Key,
    /// A unit's: each parameter's whole stream. The lists are those of the
    /// template's other declarations: a parameter that has no default of
    /// its own nor an inherited one is written from the parameter of the
    /// declaration that gives it one (§2.3: default template arguments
    /// "each from the declaration that gives it").
    Unit(&'h [TemplateParamListId]),
}

fn head(w: &mut Walk<'_>, s: &mut Stream, l: TemplateParamListId, how: Head<'_>) {
    if !w.enter() {
        s.uv(0).indeterminate(TOO_DEEP);
        return;
    }
    let ast = w.ast;
    let list = ast.templates.param_list(l);
    let (depth, params, requires) = (list.depth, list.params, list.requires);
    s.uv(u64::from(depth));
    let params: Vec<DeclId> = ast.lists.iter::<DeclId>(params).collect();
    s.count(params.len());
    for (i, &p) in params.iter().enumerate() {
        match how {
            Head::Key => w.decl_node(s, p, p),
            Head::Unit(others) => {
                let q = default_giver(w, p, i, others);
                w.decl_stream(s, q, q);
            }
        }
    }
    s.opt(requires.is_some());
    if let Some(r) = requires {
        crate::expr::requires_clause(w, s, r);
    }
    w.leave();
}

/// The parameter at position `i` whose stream a unit writes for `p`: `p`
/// itself if it has a default (its own or inherited) or if no other
/// declaration's parameter at `i` has one of its own; else the first such
/// parameter, in the order of `others`.
fn default_giver(w: &Walk<'_>, p: DeclId, i: usize, others: &[TemplateParamListId]) -> DeclId {
    if default_of(w, p).is_some() {
        return p;
    }
    let ast = w.ast;
    for &l in others {
        let list = ast.templates.param_list(l);
        if let Some(q) = ast.lists.iter::<DeclId>(list.params).nth(i)
            && ast.templates.defaults.get(q).is_some()
        {
            return q;
        }
    }
    p
}

/// The default template argument of parameter `p`: its own, or the one
/// `inherited_defaults` names (§9.5).
fn default_of(w: &Walk<'_>, p: DeclId) -> Option<TemplateArg> {
    let templates = &w.ast.templates;
    templates
        .defaults
        .get(p)
        .copied()
        .or_else(|| templates.inherited_defaults.get(p).and_then(|&o| templates.defaults.get(o).copied()))
}

/// The `data` row of a template parameter: `record TemplateParmInfo`
/// (§9.2). A template template parameter's own head is written in the
/// form of the stream that holds it: the key's inside a key, a unit's
/// (defaults included) otherwise.
pub(crate) fn template_parm_info(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    let Some(&TemplateParmInfo {
        depth,
        index,
        pack,
        typename_kw: _,
        constraint,
        params,
        ttp_kind,
    }) = w.ast.template_parm(d)
    else {
        s.uv(0).indeterminate(Cause::Unaccounted("TemplateParmInfo/missing"));
        return;
    };
    s.uv(u64::from(depth)).uv(u64::from(index)).flags(FlagWord::new().bit(pack));
    crate::expr::child_opt(w, s, constraint);
    s.opt(params.is_some());
    if let Some(l) = params {
        if w.in_key > 0 {
            template_head(w, s, l);
        } else {
            template_head_unit(w, s, l);
        }
    }
    s.tag(ttp_kind::row(&ttp_kind));
}

/// The `data` row of a template: `record TemplateInfo` (§9.2): the head,
/// with each default from the declaration that gives it (§2.3), the
/// pattern written whole (`decl?`, its definition if it has one), and a
/// concept's constraint.
pub(crate) fn template_info(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    let ast = w.ast;
    let Some(i) = ast.template_info(d) else {
        s.uv(0).indeterminate(Cause::Unaccounted("TemplateInfo/missing"));
        return;
    };
    let (params, pattern, constraint) = (i.params, i.pattern, i.constraint);
    // A template's redeclarations are those of its pattern (Kiln's
    // `Ast::first`): their heads, latest first, without `d`'s own.
    let others: Vec<TemplateParamListId> = pattern
        .map(|p| {
            ast.redecls_of(p)
                .filter_map(|r| ast.described_template(r))
                .filter(|&t| t != d)
                .filter_map(|t| ast.template_info(t).map(|x| x.params))
                .collect()
        })
        .unwrap_or_default();
    head(w, s, params, Head::Unit(&others));
    s.opt(pattern.is_some());
    if let Some(p) = pattern {
        let read = ast.definition(p).unwrap_or(p);
        w.decl_stream(s, read, p);
    }
    crate::expr::child_opt(w, s, constraint);
}

/// `record TemplLink` (§9.1): the `templ` row of a payload record.
/// `0-kind` is `enum TemplLinkKind?` (absent for the word 0), then the row
/// of the kind's index: nothing for a pattern (`Pattern.0-index` is
/// `out`), else the record it indexes.
#[allow(dead_code, reason = "called by the payload records (decl_func.rs, decl_class.rs)")]
pub(crate) fn templ_link(w: &mut Walk<'_>, s: &mut Stream, l: TemplLink) {
    let Some((kind, index)) = l.get() else {
        s.opt(false);
        if !l.is_none() {
            // A tag no variant of `TemplLinkKind` has.
            s.indeterminate(Cause::Unaccounted("TemplLink/0-kind"));
        }
        return;
    };
    s.opt(true).tag(templ_link_kind::row(&kind));
    let templates = &w.ast.templates;
    match kind {
        TemplLinkKind::Pattern => {}
        TemplLinkKind::Specialization => match templates.spec(index).copied() {
            Some(info) => spec_info(w, s, &info),
            None => {
                s.uv(0).indeterminate(Cause::Unaccounted("TemplLink/Specialization.0-index"));
            }
        },
        TemplLinkKind::MemberOfSpecialization => match templates.member_spec(index).copied() {
            Some(info) => member_spec_info(w, s, &info),
            None => {
                s.uv(0)
                    .indeterminate(Cause::Unaccounted("TemplLink/MemberOfSpecialization.0-index"));
            }
        },
        TemplLinkKind::DependentFriend => {
            s.indeterminate(Cause::Unsupported("TemplLinkKind/DependentFriend"));
        }
        TemplLinkKind::PartialSpecialization => match templates.partial(index).copied() {
            Some(info) => partial_spec_info(w, s, &info),
            None => {
                s.uv(0)
                    .indeterminate(Cause::Unaccounted("TemplLink/PartialSpecialization.0-index"));
            }
        },
    }
}

/// `enum SpecKind`. `Undeclared` is `out`: such a specialisation has no
/// unit (§9), so no stream should meet it.
fn write_spec_kind(s: &mut Stream, k: SpecKind) {
    s.tag(spec_kind::row(&k));
    match k {
        SpecKind::Undeclared => {
            s.indeterminate(Cause::Unaccounted("SpecKind/Undeclared"));
        }
        SpecKind::ImplicitInstantiation
        | SpecKind::ExplicitSpecialization
        | SpecKind::ExplicitInstantiationDeclaration
        | SpecKind::ExplicitInstantiationDefinition => {}
    }
}

/// `record SpecInfo` (§9.4). `template` and `args` are `keyed`. For an
/// implicit instantiation `from` and `from_args` are written in their
/// empty form: they are the resolved unit `definition`'s (§11.9), which
/// the demand area writes.
fn spec_info(w: &mut Walk<'_>, s: &mut Stream, x: &SpecInfo) {
    let SpecInfo {
        template: _,
        args: _,
        kind,
        from,
        from_args,
        point_of_instantiation: _,
        print_args: _,
        defaulted,
    } = *x;
    write_spec_kind(s, kind);
    let moved = match kind {
        SpecKind::ImplicitInstantiation => true,
        SpecKind::Undeclared
        | SpecKind::ExplicitSpecialization
        | SpecKind::ExplicitInstantiationDeclaration
        | SpecKind::ExplicitInstantiationDefinition => false,
    };
    if moved {
        s.uv(REF_NONE).opt(false).pending(Area::Demand);
    } else {
        write_pattern_ref(w, s, from);
        s.opt(from_args.is_some());
        if let Some(l) = from_args {
            w.write_args(s, l);
        }
    }
    s.uv(defaulted);
}

/// `record MemberSpecInfo` (§9.4).
fn member_spec_info(w: &mut Walk<'_>, s: &mut Stream, x: &MemberSpecInfo) {
    let MemberSpecInfo { instantiated_from, kind, point_of_instantiation: _ } = *x;
    write_pattern_ref(w, s, instantiated_from);
    write_spec_kind(s, kind);
}

/// `record PartialSpecInfo` (§9.4): `template` and `canonical` are
/// `keyed`, `params` is the head as a unit writes it. The record has
/// private fields (`shapes`, `shapes_len`), so it is read field by field
/// and not destructured.
fn partial_spec_info(w: &mut Walk<'_>, s: &mut Stream, x: &PartialSpecInfo) {
    template_head_unit(w, s, x.params);
}

/// `ref` to a declaration of a pattern (§9): `Ref::none` for one that has
/// no key, a local declaration of the template's unit (a parameter, a
/// local variable, class or label), without a cause; a template parameter
/// is `Ref::template_param`; any other is its `Ref` as `write_ref` writes
/// it.
fn write_pattern_ref(w: &mut Walk<'_>, s: &mut Stream, p: DeclId) {
    if is_pattern_local(w, p) {
        s.uv(REF_NONE);
    } else {
        w.write_ref(s, p);
    }
}

fn write_pattern_ref_opt(w: &mut Walk<'_>, s: &mut Stream, p: Option<DeclId>) {
    match p {
        Some(p) => write_pattern_ref(w, s, p),
        None => {
            s.uv(REF_NONE);
        }
    }
}

/// A declaration of another unit that has no key: local to its unit
/// (digest §4.3), not a template parameter.
fn is_pattern_local(w: &Walk<'_>, p: DeclId) -> bool {
    let kind = w.ast.decls.get(p).kind();
    if kind == DeclKind::TemplateTypeParm
        || kind == DeclKind::NonTypeTemplateParm
        || kind == DeclKind::TemplateTemplateParm
    {
        return false;
    }
    let local_role = match info(kind).role {
        Role::Local => true,
        Role::Nothing
        | Role::Transparent
        | Role::Root
        | Role::Entity(_)
        | Role::Content(_)
        | Role::InParent
        | Role::Unsupported(_) => false,
    };
    local_role || (w.is_local(p) && !w.is_local_extern(p))
}

/// The `in` rows of `TemplateTables` whose map is keyed by `ExprId`
/// (§9.5), for `e`, in table order and each in its empty form when the
/// map has no entry: they close every expression's stream (§2.1 step 3).
#[allow(dead_code, reason = "called by the expression area (expr.rs)")]
pub(crate) fn expr_maps(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let ast = w.ast;
    let tables = &ast.templates;
    // expr_targs: enum ArgView*?
    let targs = tables.expr_targs.get(e).copied();
    s.opt(targs.is_some());
    if let Some(l) = targs {
        w.write_args(s, l);
    }
    // expr_nns: enum Nns?
    let nns = tables.expr_nns.get(e).copied();
    s.opt(nns.is_some());
    if let Some(n) = nns {
        w.write_nns(s, n);
    }
    // size_of_pack_origin: ref.
    match tables.size_of_pack_origin.get(e).copied() {
        Some(d) => pack_ref(w, s, d),
        None => {
            s.uv(REF_NONE);
        }
    }
    // size_of_pack_partial: enum ArgView*?
    let partial = tables.size_of_pack_partial.get(e).copied();
    s.opt(partial.is_some());
    if let Some(l) = partial {
        w.write_args(s, l);
    }
    // pack_index_origin: bool.
    s.bool(tables.pack_index_origin.contains(e));
    // pseudo_dtor_name: sym?
    let name = tables.pseudo_dtor_name.get(e).copied();
    s.opt(name.is_some());
    if let Some(n) = name {
        w.write_sym(s, n);
    }
    // pseudo_dtor_first-type: type? (`-position` is `out`).
    let first = tables.pseudo_dtor_first.get(e).map(|&(q, _)| q);
    w.write_type_opt(s, first);
}

/// The `ref` of `size_of_pack_origin`: the pack a substituted `sizeof...`
/// named, which the literal still mangles by. A template parameter pack is
/// `Ref::template_param`; a function parameter pack of the pattern has no
/// key and no ordinal of this unit, and is `Ref::param` with the
/// function-scope depth and the position Kiln stores (what the mangling
/// `sZ fL<depth>p<index>_` reads).
fn pack_ref(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    let ast = w.ast;
    if ast.decls.get(d).kind() == DeclKind::ParmVar {
        match ast.decls.param(d) {
            Some(p) => {
                s.uv(REF_PARAM).uv(u64::from(p.scope_depth())).uv(u64::from(p.index));
            }
            None => {
                s.uv(REF_NONE)
                    .indeterminate(Cause::Unaccounted("TemplateTables/size_of_pack_origin"));
            }
        }
        return;
    }
    if is_pattern_local(w, d) {
        // A pack that is neither a parameter nor a template parameter (a
        // structured binding pack of the pattern): no rule names it.
        s.uv(REF_NONE).indeterminate(Cause::Unaccounted("TemplateTables/size_of_pack_origin"));
        return;
    }
    w.write_ref(s, d);
}

/// The value of a `TemplateParamObject` (`TemplateTables`
/// `param_object_values`: `value?`), which is its unit's whole stream
/// after its `DeclNode` rows.
pub(crate) fn param_object(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    let v = w.ast.templates.param_object_value(d);
    s.opt(v.is_some());
    if let Some(v) = v {
        crate::value::value(w, s, v);
    }
}

/// The `in` rows of `TemplateTables` whose map is keyed by `DeclId`
/// (§9.5), for `d`, in table order and each in its empty form when the
/// map has no entry: `defaults` (the default template argument of a
/// parameter, its own or the one `inherited_defaults` names, in canonical
/// form), `trailing_requires`, `ttp_owner`, `member_like_friend`,
/// `instantiated_from`.
pub(crate) fn decl_maps(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    let ast = w.ast;
    let templates = &ast.templates;
    // defaults: enum ArgView?
    let default = default_of(w, d);
    s.opt(default.is_some());
    if let Some(a) = default {
        match ast.types.canonical_arg(a).view() {
            // A default written as an expression stays one in canonical
            // form: it is the canonical operand of that expression.
            ArgView::Expr { expr, profile: _ } => {
                s.tag(arg_view::PROFILE);
                crate::expr::profile_expr(w, s, expr);
            }
            v @ (ArgView::Null
            | ArgView::Type(_)
            | ArgView::Value { .. }
            | ArgView::Template(_)
            | ArgView::TemplateExpansion { .. }
            | ArgView::Profile(_)
            | ArgView::Pack(_)) => w.write_arg(s, v),
        }
    }
    // trailing_requires: child?
    let requires = templates.trailing_requires.get(d).copied();
    s.opt(requires.is_some());
    if let Some(r) = requires {
        crate::expr::requires_clause(w, s, r);
    }
    // ttp_owner: ref (a pattern: its template's `Ref`).
    let owner = templates.ttp_owner.get(d).copied();
    write_pattern_ref_opt(w, s, owner);
    // member_like_friend: ref. Kiln's mangler looks the declaration up,
    // then its first declaration (`member_like_friend_class`).
    let friend_of = templates
        .member_like_friend
        .get(d)
        .or_else(|| templates.member_like_friend.get(ast.first(d)))
        .copied();
    w.write_ref_opt(s, friend_of);
    // instantiated_from: ref (a pattern).
    let from = templates.instantiated_from_map(d);
    write_pattern_ref_opt(w, s, from);
}
