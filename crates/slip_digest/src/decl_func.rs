//! The payload records of functions and variables
//! (`spec/digest-kinds.md` §5.4), with the language linkage their
//! streams hold (§5.6 `ScopeData`).
//!
//! Each function writes one `record T` for the declaration `d`, the one
//! the walk reads (§2.3), merging over the declarations of its entity what
//! §2.3 merges. What `spec/landed/digest-kinds/WP-026.md` decides is
//! cited where it is applied.

use slip_kiln::kiln::ast::DeclId;
use slip_kiln::kiln::ast::decl::{
    ConstexprKind, DeclFlags, Defaulted, FnSpec, FnStorage, FunctionData, InitStyle, LetData,
    ParamData, ScopeData, VarData, VarEvalState, VarSpec,
};
use slip_kiln::kiln::ast::templates::SpecKind;

use crate::stream::{FlagWord, Stream};
use crate::tables::{
    constexpr_kind, defaulted, fn_storage, init_style, linkage_lang, tls_kind, var_storage,
};
use crate::walk::Walk;
use crate::{Area, Cause};

/// A bit or a value of `FnSpec` that no row accounts for.
const FN_SPEC_UNACCOUNTED: Cause = Cause::Unaccounted("FnSpec/0");
/// A bit or a value of `VarSpec` that no row accounts for.
const VAR_SPEC_UNACCOUNTED: Cause = Cause::Unaccounted("VarSpec/0");
/// A bit of `ParamData` `flags` that no row accounts for.
const PARAM_FLAGS_UNACCOUNTED: Cause = Cause::Unaccounted("ParamData/flags");
/// Declarations of one entity that disagree on `constexpr`/`consteval`.
const CONSTEXPR_CONFLICT: Cause = Cause::Unaccounted("FnSpec/0-constexpr-kind");
/// A late-parsed default argument that is still missing in a function
/// that is no instantiation (§11.4).
const DEFAULT_ARG_MISSING: Cause = Cause::Unaccounted("ParamData/flags-default-arg-pending");
/// `VarEvalState::Evaluating` after the translation unit (§11.3).
const EVALUATING: Cause = Cause::Unaccounted("VarEvalState/Evaluating");
/// `ScopeData` `aux` bits no row accounts for.
const SCOPE_AUX_UNACCOUNTED: Cause = Cause::Unaccounted("ScopeData/aux");
/// A declaration of a function or variable kind with no payload record.
const NO_PAYLOAD: Cause = Cause::Unaccounted("DeclNode/data");

/// `FunctionData` (§5.4), then the language linkage (§5.6).
pub(crate) fn function_data(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    let ast = w.ast;
    let Some(&FunctionData { redecl: _, params, body, spec, templ }) = ast.decls.function(d) else {
        s.uv(0).indeterminate(NO_PAYLOAD);
        return;
    };
    let e = ast.first(d);
    let definition_class = function_definition_class(w, e);
    // params: decl*, in order.
    let params: Vec<DeclId> = ast.lists.iter::<DeclId>(params).collect();
    s.count(params.len());
    for p in params {
        crate::local::write_decl(w, s, p);
    }
    // body: child?; in `definition` for the definition class (§11.9),
    // which the demand package writes.
    if definition_class {
        s.opt(false).pending(Area::Demand);
    } else {
        s.opt(body.is_some());
        if let Some(b) = body {
            crate::stmt::child(w, s, b);
        }
    }
    // spec: record FnSpec.
    fn_spec(w, s, e, spec, definition_class);
    // templ: record TemplLink.
    crate::templ::templ_link(w, s, templ);
    linkage(w, s, e);
}

/// `record FnSpec` of the declaration read (`spec`), merged over the
/// declarations of `e` as §2.3 and `spec/landed/digest-kinds/WP-026.md`
/// say.
fn fn_spec(w: &mut Walk<'_>, s: &mut Stream, e: DeclId, spec: FnSpec, definition_class: bool) {
    let ast = w.ast;
    if rebuild_fn_spec(spec).bits() != spec.bits() {
        s.indeterminate(FN_SPEC_UNACCOUNTED);
    }
    let specs: Vec<FnSpec> = w
        .redecls(e)
        .iter()
        .filter_map(|&x| ast.decls.function(x).map(|f| f.spec))
        .collect();
    for &x in &specs {
        if rebuild_fn_spec(x).bits() != x.bits() {
            s.indeterminate(FN_SPEC_UNACCOUNTED);
        }
    }
    // Set if any declaration sets them (§2.3).
    let inline = specs.iter().any(|x| x.is_inline());
    // A declaration after an `inline` one is marked implicitly inline by
    // Kiln: that mark says nothing a written `inline` does not.
    let implicit_inline = !inline && specs.iter().any(|x| x.implicit_inline());
    let (mut any_constexpr, mut any_consteval) = (false, false);
    for x in &specs {
        match x.constexpr_kind() {
            ConstexprKind::None => {}
            ConstexprKind::Constexpr => any_constexpr = true,
            ConstexprKind::Consteval => any_consteval = true,
        }
    }
    if any_constexpr && any_consteval {
        s.indeterminate(CONSTEXPR_CONFLICT);
    }
    let constexpr = if any_consteval {
        ConstexprKind::Consteval
    } else if any_constexpr {
        ConstexprKind::Constexpr
    } else {
        ConstexprKind::None
    };
    // 0-storage: `extern` is written as `None` unless the function is
    // inline; it counts if any declaration says it.
    let storage = match spec.storage() {
        FnStorage::Static => FnStorage::Static,
        FnStorage::None | FnStorage::Extern => {
            let any_extern = specs.iter().any(|x| x.storage() == FnStorage::Extern)
                || spec.storage() == FnStorage::Extern;
            if any_extern && (inline || specs.iter().any(|x| x.implicit_inline())) {
                FnStorage::Extern
            } else {
                FnStorage::None
            }
        }
    };
    s.tag(fn_storage::row(&storage));
    // 0-return-deduced is in `definition` for the definition class
    // (§11.9): its empty form here.
    let return_deduced = spec.return_deduced() && !definition_class;
    s.flags(
        FlagWord::new()
            .bit(inline)
            .bit(implicit_inline)
            .bit(spec.is_virtual())
            .bit(spec.is_pure())
            .bit(spec.is_deleted())
            .bit(spec.is_explicit())
            .bit(spec.is_trivial())
            .bit(spec.immediate_escalating())
            .bit(spec.has_contracts())
            .bit(spec.is_coroutine())
            .bit(spec.escalated())
            .bit(spec.is_variadic())
            .bit(spec.is_final())
            .bit(spec.has_explicit_object())
            .bit(spec.not_selected())
            .bit(return_deduced),
    );
    s.tag(constexpr_kind::row(&constexpr));
    s.tag(defaulted::row(&spec.defaulted()));
}

/// `spec` rebuilt from what its accessors decode: a bit or a value no
/// accessor reads (no row accounts for it) is lost.
fn rebuild_fn_spec(spec: FnSpec) -> FnSpec {
    FnSpec::new()
        .with_storage(spec.storage())
        .with_inline(spec.is_inline())
        .with_implicit_inline(spec.implicit_inline())
        .with_constexpr_kind(spec.constexpr_kind())
        .with_virtual(spec.is_virtual())
        .with_pure(spec.is_pure())
        .with_defaulted(spec.defaulted())
        .with_deleted(spec.is_deleted())
        .with_explicit(spec.is_explicit())
        .with_trivial(spec.is_trivial())
        .with_immediate_escalating(spec.immediate_escalating())
        .with_has_contracts(spec.has_contracts())
        .with_coroutine(spec.is_coroutine())
        .with_escalated(spec.escalated())
        .with_variadic(spec.is_variadic())
        .with_override(spec.is_override())
        .with_final(spec.is_final())
        .with_explicit_object(spec.has_explicit_object())
        .with_virtual_written(spec.virtual_written())
        .with_storage_inherited(spec.storage_inherited())
        .with_not_selected(spec.not_selected())
        .with_return_deduced(spec.return_deduced())
}

/// `VarData` (§5.4), then the language linkage (§5.6).
pub(crate) fn var_data(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    let ast = w.ast;
    let Some(&VarData { redecl: _, init, spec, eval, templ }) = ast.decls.var(d) else {
        s.uv(0).indeterminate(NO_PAYLOAD);
        return;
    };
    let e = ast.first(d);
    let definition_class = var_definition_class(w, e);
    // init: child?; in `definition` for the definition class (§11.9).
    if definition_class {
        s.opt(false).pending(Area::Demand);
    } else {
        crate::expr::child_opt(w, s, init);
    }
    // spec: record VarSpec.
    var_spec(w, s, e, spec, definition_class);
    // eval: demand, the resolved unit `value` (§11.3). Only a state that
    // cannot outlive the translation unit is read here.
    match eval.state() {
        VarEvalState::Evaluating => {
            s.indeterminate(EVALUATING);
        }
        VarEvalState::NotEvaluated | VarEvalState::NotConstant | VarEvalState::Value(_) => {}
    }
    // templ: record TemplLink.
    crate::templ::templ_link(w, s, templ);
    linkage(w, s, e);
}

/// `record VarSpec` of the declaration read (`spec`), merged over the
/// declarations of `e` as §2.3 and `spec/landed/digest-kinds/WP-026.md`
/// say.
fn var_spec(w: &mut Walk<'_>, s: &mut Stream, e: DeclId, spec: VarSpec, definition_class: bool) {
    let ast = w.ast;
    let specs: Vec<VarSpec> =
        w.redecls(e).iter().filter_map(|&x| ast.decls.var(x).map(|v| v.spec)).collect();
    if rebuild_var_spec(spec).bits() != spec.bits()
        || specs.iter().any(|&x| rebuild_var_spec(x).bits() != x.bits())
    {
        s.indeterminate(VAR_SPEC_UNACCOUNTED);
    }
    // Set if any declaration sets them (§2.3).
    let constexpr = specs.iter().any(|x| x.is_constexpr());
    let constinit = specs.iter().any(|x| x.is_constinit());
    let inline = specs.iter().any(|x| x.is_inline());
    // Implied: inline, and written on no declaration.
    let inline_implied = inline && specs.iter().all(|x| !x.is_inline() || x.is_inline_implied());
    s.tag(var_storage::row(&spec.storage()));
    s.tag(tls_kind::row(&spec.tls()));
    // 0-init-style, 0-inline and 0-inline-implied are in `definition` for
    // the definition class (§11.9): their empty forms here.
    let style = if definition_class { InitStyle::C } else { spec.init_style() };
    s.tag(init_style::row(&style));
    s.flags(
        FlagWord::new()
            .bit(constexpr)
            .bit(constinit)
            .bit(inline && !definition_class)
            .bit(spec.is_nrvo())
            .bit(spec.is_exception_var())
            .bit(spec.is_cxx_for_range())
            .bit(inline_implied && !definition_class),
    );
}

/// `spec` rebuilt from what its accessors decode (`rebuild_fn_spec`).
fn rebuild_var_spec(spec: VarSpec) -> VarSpec {
    VarSpec::new()
        .with_storage(spec.storage())
        .with_tls(spec.tls())
        .with_init_style(spec.init_style())
        .with_constexpr(spec.is_constexpr())
        .with_constinit(spec.is_constinit())
        .with_inline(spec.is_inline())
        .with_nrvo(spec.is_nrvo())
        .with_exception_var(spec.is_exception_var())
        .with_cxx_for_range(spec.is_cxx_for_range())
        .with_inline_implied(spec.is_inline_implied())
}

/// `ParamData` (§5.4) of the parameter `d`.
pub(crate) fn param_data(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    let ast = w.ast;
    let Some(data) = ast.decls.param(d) else {
        s.uv(0).indeterminate(NO_PAYLOAD);
        return;
    };
    let ParamData { default_arg, original_ty, index, flags } = *data;
    let known = ParamData::EXPLICIT_OBJECT
        | ParamData::PACK
        | ParamData::DEFAULT_ARG_PENDING
        | ParamData::STATIC_ARRAY
        | ParamData::SCOPE_DEPTH_MASK;
    if flags & !known != 0 {
        s.indeterminate(PARAM_FLAGS_UNACCOUNTED);
    }
    // The function whose parameter `d` is, if it is one's.
    let function = ast.decls.get(d).ctx().filter(|&f| ast.decls.function(f).is_some());
    let instantiation = function.is_some_and(|f| implicit_instantiation(w, ast.first(f)));
    // default_arg: child?. In an implicit instantiation an absent child
    // whatever Kiln holds (§11.4); else the one a declaration of the
    // function gives (§2.3).
    let default = if instantiation {
        None
    } else {
        default_arg.or_else(|| function.and_then(|f| merged_default(w, f, index)))
    };
    if !instantiation && default.is_none() && flags & ParamData::DEFAULT_ARG_PENDING != 0 {
        s.indeterminate(DEFAULT_ARG_MISSING);
    }
    crate::expr::child_opt(w, s, default);
    // original_ty: type?.
    w.write_type_opt(s, original_ty);
    // index: out. flags-explicit-object, flags-pack, flags-static-array:
    // bit; flags-default-arg-pending: demand (not written).
    s.flags(
        FlagWord::new()
            .bit(flags & ParamData::EXPLICIT_OBJECT != 0)
            .bit(flags & ParamData::PACK != 0)
            .bit(flags & ParamData::STATIC_ARRAY != 0),
    );
    // flags-scope-depth: uv.
    s.uv(u64::from(data.scope_depth()));
}

/// The default argument of parameter `index` given by a declaration of
/// function `f`'s entity, the first declaration first (§2.3).
fn merged_default(
    w: &Walk<'_>,
    f: DeclId,
    index: u16,
) -> Option<slip_kiln::kiln::ast::ExprId> {
    let ast = w.ast;
    let mut chain = w.redecls(ast.first(f));
    chain.reverse();
    chain.iter().find_map(|&x| {
        let data = ast.decls.function(x)?;
        let p = ast.lists.iter::<DeclId>(data.params).nth(usize::from(index))?;
        ast.decls.param(p)?.default_arg
    })
}

/// `LetData` (§5.4).
pub(crate) fn let_data(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    let Some(&LetData { extending, mangling_index, value: _ }) = w.ast.decls.lifetime_extended(d)
    else {
        s.uv(0).indeterminate(NO_PAYLOAD);
        return;
    };
    // extending: ref; mangling_index: uv; value: demand (§11.3).
    w.write_ref(s, extending);
    s.uv(u64::from(mangling_index));
}

/// The language linkage a function's or variable's stream holds
/// (§5.6 `ScopeData`): `enum LinkageLang?` of the innermost
/// `LinkageSpec` around the first declaration `e`, through classes and
/// functions, then, when present, `aux-braceless` as `bool`. The reading
/// is the one the key's parent uses (`Walk::linkage_spec`).
fn linkage(w: &mut Walk<'_>, s: &mut Stream, e: DeclId) {
    match w.linkage_spec(e, true) {
        Some((spec, lang)) => {
            let aux = w.ast.decls.scope(spec).map_or(0, |x| x.aux);
            if aux & !(0xff | ScopeData::LINKAGE_BRACELESS) != 0 {
                s.indeterminate(SCOPE_AUX_UNACCOUNTED);
            }
            s.opt(true).tag(linkage_lang::row(&lang));
            s.bool(aux & ScopeData::LINKAGE_BRACELESS != 0);
        }
        None => {
            s.opt(false);
        }
    }
}

/// Whether the entity `e` (a first declaration) is an implicit
/// instantiation: its own `SpecInfo` or `MemberSpecInfo` kind, else an
/// enclosing implicit instantiation, as `units::unit_class` reads it.
pub(crate) fn implicit_instantiation(w: &Walk<'_>, e: DeclId) -> bool {
    let ast = w.ast;
    let own = ast
        .spec_info(e)
        .map(|(_, i)| i.kind)
        .or_else(|| ast.member_spec_info(e).map(|(_, i)| i.kind));
    match own {
        Some(SpecKind::ImplicitInstantiation) => true,
        Some(
            SpecKind::Undeclared
            | SpecKind::ExplicitSpecialization
            | SpecKind::ExplicitInstantiationDeclaration
            | SpecKind::ExplicitInstantiationDefinition,
        ) => false,
        None => crate::units::in_implicit_instantiation(w, e),
    }
}

/// Every declaration of `e` is implicit.
fn all_implicit(w: &Walk<'_>, e: DeclId) -> bool {
    w.redecls(e).iter().all(|&x| w.ast.decls.get(x).has(DeclFlags::IMPLICIT))
}

/// A function of the definition class (§11.9): an implicit
/// instantiation, an implicitly declared member, comparison or deduction
/// guide (and the conversion functions of a closure type, which Kiln
/// declares implicitly). Also a function explicitly defaulted on its
/// first declaration, which Kiln defines on odr-use as it does an
/// implicit member (`spec/landed/digest-kinds/WP-026.md`).
fn function_definition_class(w: &Walk<'_>, e: DeclId) -> bool {
    let defaulted_first =
        w.ast.decls.function(e).is_some_and(|f| f.spec.defaulted() == Defaulted::Explicit);
    implicit_instantiation(w, e) || all_implicit(w, e) || defaulted_first
}

/// A variable of the definition class (§11.9): an implicit
/// instantiation.
fn var_definition_class(w: &Walk<'_>, e: DeclId) -> bool {
    implicit_instantiation(w, e)
}
