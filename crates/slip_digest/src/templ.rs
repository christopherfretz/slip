//! Templates (`spec/digest-kinds.md` §9): template heads, `TemplateInfo`
//! and `TemplateParmInfo` here, as keys need them; the rest pending
//! (WP-024).

use slip_kiln::kiln::ast::templates::TemplateParmInfo;
use slip_kiln::kiln::ast::{DeclId, TemplateParamListId};

use crate::stream::{FlagWord, Stream};
use crate::tables::ttp_kind;
use crate::walk::{TOO_DEEP, Walk};
use crate::{Area, Cause};

/// `record TemplateParamList` (§9.2): `depth uv`, the parameters as
/// `decl*` (each its `DeclNode` rows, which bring in its
/// `TemplateParmInfo`), and the requires-clause `child?`.
///
/// A parameter's default argument is a row of `ColdDeclData`, written by
/// the declaration's own stream, not here: a template head in a key does
/// not change when a default is added.
pub(crate) fn template_head(w: &mut Walk<'_>, s: &mut Stream, l: TemplateParamListId) {
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
        w.decl_node(s, p, p);
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
    template_head(w, s, params);
    s.opt(pattern.is_some());
    if let Some(p) = pattern {
        let read = ast.definition(p).unwrap_or(p);
        w.decl_stream(s, read, p);
    }
    crate::expr::child_opt(w, s, constraint);
}

/// The value of a `TemplateParamObject` (`TemplateTables::param_object_value`).
pub(crate) fn param_object(_w: &mut Walk<'_>, s: &mut Stream, _d: DeclId) {
    s.pending(Area::Templ);
}

/// The `in` rows of `TemplateTables` whose map is keyed by `DeclId`
/// (§9.5), for `d`: its `TemplLink` record and the specialisation and
/// partial-specialisation records.
pub(crate) fn decl_maps(_w: &mut Walk<'_>, s: &mut Stream, _d: DeclId) {
    s.pending(Area::Templ);
}
