//! Templates (`spec/digest-kinds.md` §9): template heads, `TemplateInfo`
//! and `TemplateParmInfo` here, as keys need them; the rest pending
//! (WP-024).

use slip_kiln::kiln::ast::templates::TemplateParmInfo;
use slip_kiln::kiln::ast::types::ArgView;
use slip_kiln::kiln::ast::{DeclId, TemplateParamListId};

use crate::stream::{FlagWord, Stream};
use crate::tables::{arg_view, ttp_kind};
use crate::walk::{TOO_DEEP, Walk};
use crate::{Area, Cause};

/// The template head of a discriminator (§9, §3.3): `record
/// TemplateParamList` with each parameter as its `DeclNode` rows (which
/// bring in its `TemplateParmInfo`) and nothing else, so that a key does
/// not change when a default argument is added.
pub(crate) fn template_head(w: &mut Walk<'_>, s: &mut Stream, l: TemplateParamListId) {
    head(w, s, l, false);
}

/// `record TemplateParamList` (§9.2) as a unit writes it: the parameters
/// as `decl*`, each its whole stream (§5.1), which closes with the rows
/// of `TemplateTables` keyed by the parameter: its default argument
/// (`decl_maps`).
pub(crate) fn template_head_unit(w: &mut Walk<'_>, s: &mut Stream, l: TemplateParamListId) {
    head(w, s, l, true);
}

fn head(w: &mut Walk<'_>, s: &mut Stream, l: TemplateParamListId, unit: bool) {
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
    for p in params {
        if unit {
            w.decl_stream(s, p, p);
        } else {
            w.decl_node(s, p, p);
        }
    }
    s.opt(requires.is_some());
    if let Some(r) = requires {
        crate::expr::requires_clause(w, s, r);
    }
    w.leave();
}

/// The `data` row of a template parameter: `record TemplateParmInfo`
/// (§9.2).
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
        template_head(w, s, l);
    }
    s.tag(ttp_kind::row(&ttp_kind));
}

/// The `data` row of a template: `record TemplateInfo` (§9.2): the head,
/// the pattern written whole (`decl?`, its definition if it has one), and
/// a concept's constraint.
pub(crate) fn template_info(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    let ast = w.ast;
    let Some(i) = ast.template_info(d) else {
        s.uv(0).indeterminate(Cause::Unaccounted("TemplateInfo/missing"));
        return;
    };
    let (params, pattern, constraint) = (i.params, i.pattern, i.constraint);
    template_head_unit(w, s, params);
    s.opt(pattern.is_some());
    if let Some(p) = pattern {
        let read = ast.definition(p).unwrap_or(p);
        w.decl_stream(s, read, p);
    }
    crate::expr::child_opt(w, s, constraint);
}

/// `record TemplLink` (§9.1): the `templ` row of a payload record.
#[allow(dead_code, reason = "the interface of an area that lands later (lib.rs)")]
pub(crate) fn templ_link(
    _w: &mut Walk<'_>,
    s: &mut Stream,
    _l: slip_kiln::kiln::ast::decl::TemplLink,
) {
    s.pending(Area::Templ);
}

/// The `in` rows of `TemplateTables` whose map is keyed by `ExprId`
/// (§9.5), for `e`: they close every expression's stream (§2.1 step 3).
#[allow(dead_code, reason = "the interface of an area that lands later (lib.rs)")]
pub(crate) fn expr_maps(_w: &mut Walk<'_>, s: &mut Stream, _e: slip_kiln::kiln::ast::ExprId) {
    s.pending(Area::Templ);
}

/// The value of a `TemplateParamObject` (`TemplateTables::param_object_value`).
pub(crate) fn param_object(_w: &mut Walk<'_>, s: &mut Stream, _d: DeclId) {
    s.pending(Area::Templ);
}

/// The `in` rows of `TemplateTables` whose map is keyed by `DeclId`
/// (§9.5), for `d`: `defaults` (written: the default template argument of
/// a parameter, its own or the one `inherited_defaults` names, in
/// canonical form), then the rest pending: `trailing_requires`, the
/// `TemplLink` record and the specialisation and partial-specialisation
/// records.
pub(crate) fn decl_maps(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    let ast = w.ast;
    let templates = &ast.templates;
    let default = templates.defaults.get(d).copied().or_else(|| {
        templates.inherited_defaults.get(d).and_then(|&o| templates.defaults.get(o).copied())
    });
    s.opt(default.is_some());
    if let Some(a) = default {
        match ast.types.canonical_arg(a).view() {
            // A default written as an expression stays one in canonical
            // form: it is the canonical operand of that expression.
            ArgView::Expr { expr, profile: _ } => {
                s.tag(arg_view::PROFILE);
                crate::expr_dep::profile_expr(w, s, expr);
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
    s.pending(Area::Templ);
}
