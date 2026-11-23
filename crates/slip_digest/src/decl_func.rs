//! The payload records of functions and variables
//! (`spec/digest-kinds.md` §5.4), with the language linkage their
//! streams hold (§5.6 `ScopeData`).
//!
//! Each function writes one `record T` for the declaration `d`, the one
//! the walk reads (§2.3), merging over the declarations of its entity what
//! §2.3 merges. What `spec/landed/digest-kinds/WP-026.md` decides is
//! cited where it is applied.

use slip_kiln::kiln::ast::decl::{
    ConstexprKind, FnSpec, FnStorage, FunctionData, InitStyle, LetData, ParamData, ScopeData,
    VarData, VarEvalState, VarSpec,
};
use slip_kiln::kiln::ast::types::TypeKind;
use slip_kiln::kiln::ast::{DeclId, QualType, StmtId};

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
/// A `LinkageSpec` whose language byte is no `LinkageLang`.
const LANG_UNACCOUNTED: Cause = Cause::Unaccounted("ScopeData/aux-lang");
/// A declaration of a function or variable kind with no payload record.
const NO_PAYLOAD: Cause = Cause::Unaccounted("DeclNode/data");
/// Two declarations of a variable's chain hold an initialiser (ill-formed,
/// [basic.def.odr]); Kiln reads the first.
const TWO_INITS: Cause = Cause::Unaccounted("VarData/init");

/// `FunctionData` (§5.4), then the language linkage (§5.6).
pub(crate) fn function_data(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    let ast = w.ast;
    let Some(&FunctionData { redecl: _, params, body, spec, templ }) = ast.decls.function(d) else {
        s.uv(0).indeterminate(NO_PAYLOAD);
        return;
    };
    let e = w.representative(ast.first(d));
    let definition_class = crate::demand::definition_class(w, e);
    // params: decl*, in order. A default argument taken from another
    // declaration names that declaration's parameters: parameter i of
    // every declaration of the chain is one local entity, with the
    // ordinal of parameter i of the declaration read
    // (`spec/landed/digest-kinds/WP-026.md`).
    let params: Vec<DeclId> = ast.lists.iter::<DeclId>(params).collect();
    alias_params(w, e, &params);
    s.count(params.len());
    for p in params {
        crate::local::write_decl(w, s, p);
    }
    // body: child?; in `definition` for the definition class (§11.9),
    // its empty form here.
    if definition_class {
        s.opt(false);
    } else {
        write_body(w, s, body);
    }
    // spec: record FnSpec.
    fn_spec(w, s, e, spec, definition_class);
    // templ: record TemplLink.
    crate::templ::templ_link(w, s, d, templ);
    linkage(w, s, e);
}

/// Gives the parameters `params` of the declaration read their ordinals,
/// in order, and makes parameter i of every other declaration of `e` the
/// same local entity as `params[i]`.
fn alias_params(w: &mut Walk<'_>, e: DeclId, params: &[DeclId]) {
    let ast = w.ast;
    let ordinals: Vec<u64> = params.iter().map(|&p| crate::local::ordinal(w, p)).collect();
    for x in w.redecls(e) {
        let Some(f) = ast.decls.function(x) else { continue };
        for (i, q) in ast.lists.iter::<DeclId>(f.params).enumerate() {
            if let Some(&n) = ordinals.get(i) {
                w.unit.locals.entry(ast.first(q)).or_insert(n);
            }
        }
    }
}

/// `record FnSpec` of the declaration read (`spec`), merged over the
/// declarations of `e` as §2.3 and `spec/landed/digest-kinds/WP-026.md`
/// say: every bit is set if any declaration sets it.
fn fn_spec(w: &mut Walk<'_>, s: &mut Stream, e: DeclId, spec: FnSpec, definition_class: bool) {
    let ast = w.ast;
    if rebuild_fn_spec(spec).bits() != spec.bits() {
        s.indeterminate(FN_SPEC_UNACCOUNTED);
    }
    let mut specs: Vec<FnSpec> =
        w.redecls(e).iter().filter_map(|&x| ast.decls.function(x).map(|f| f.spec)).collect();
    specs.push(spec);
    for &x in &specs {
        if rebuild_fn_spec(x).bits() != x.bits() {
            s.indeterminate(FN_SPEC_UNACCOUNTED);
        }
    }
    let any = |f: fn(FnSpec) -> bool| specs.iter().any(|&x| f(x));
    let inline = any(FnSpec::is_inline);
    // A declaration after an `inline` one is marked implicitly inline by
    // Kiln: that mark says nothing a written `inline` does not. For the
    // definition class, Kiln sets it when it instantiates the definition
    // (`enter_definition_scopes`, `kiln_sema/src/class/members.rs:902-907`):
    // `definition`'s, its empty form here (§11.9).
    let implicit_inline = !inline && any(FnSpec::implicit_inline) && !definition_class;
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
            let any_extern = specs.iter().any(|x| x.storage() == FnStorage::Extern);
            if any_extern && (inline || (!definition_class && any(FnSpec::implicit_inline))) {
                FnStorage::Extern
            } else {
                FnStorage::None
            }
        }
    };
    s.tag(fn_storage::row(&storage));
    // In `definition` for the definition class (§11.9), their empty form
    // here: `0-escalated` and `0-is-coroutine`, which the analysis of the
    // body sets; `0-has-contracts`, which Kiln sets when it instantiates
    // the definition's contracts (`install_instantiated_contracts`,
    // `kiln_sema/src/contracts.rs:681-687`); `0-return-deduced` for an instantiation, whose return
    // type is deduced when its body is instantiated (a function defaulted
    // on its first declaration deduces at the class's end).
    let from_body = |v: bool| v && !definition_class;
    let return_deduced = any(FnSpec::return_deduced)
        && !(definition_class && crate::demand::pattern_holds_placeholder(w, e));
    s.flags(
        FlagWord::new()
            .bit(inline)
            .bit(implicit_inline)
            .bit(any(FnSpec::is_virtual))
            .bit(any(FnSpec::is_pure))
            .bit(any(FnSpec::is_deleted))
            .bit(any(FnSpec::is_explicit))
            .bit(any(FnSpec::is_trivial))
            .bit(any(FnSpec::immediate_escalating))
            .bit(from_body(any(FnSpec::has_contracts)))
            .bit(from_body(any(FnSpec::is_coroutine)))
            .bit(from_body(any(FnSpec::escalated)))
            .bit(any(FnSpec::is_variadic))
            .bit(any(FnSpec::is_final))
            .bit(any(FnSpec::has_explicit_object))
            .bit(any(FnSpec::not_selected))
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
    let e = w.representative(ast.first(d));
    let definition_class = crate::demand::definition_class(w, e);
    // `init`, `0-init-style` and `eval` belong to the declaration that
    // holds the initialiser, which need not be the one read (an in-class
    // `static const int x = 1;` defined by `const int S::x;`).
    let (init, style, eval) = match init_holder(w, e) {
        Ok(Some(holder)) => ast
            .decls
            .var(holder)
            .map_or((init, spec.init_style(), eval), |v| (v.init, v.spec.init_style(), v.eval)),
        Ok(None) => (init, spec.init_style(), eval),
        Err(()) => {
            s.indeterminate(TWO_INITS);
            (init, spec.init_style(), eval)
        }
    };
    // init: child?; in `definition` for the definition class (§11.9),
    // its empty form here.
    if definition_class {
        s.opt(false);
    } else {
        crate::expr::child_opt(w, s, init);
    }
    // spec: record VarSpec. A declaration directly in a linkage
    // specification without braces is `extern` ([dcl.link]/8): written so,
    // which is all `ScopeData` `aux-braceless` (out) says of a variable.
    let spec = if spec.storage() == slip_kiln::kiln::ast::decl::VarStorage::None
        && ast.decls.is_single_line_linkage(d)
    {
        spec.with_storage(slip_kiln::kiln::ast::decl::VarStorage::Extern)
    } else {
        spec
    };
    var_spec(w, s, e, spec, style, definition_class);
    // eval: demand, the resolved unit `value` (§11.3). Only a state that
    // cannot outlive the translation unit is read here. For the
    // definition class nothing of it is read, not even whether Kiln
    // evaluated: Kiln evaluates with the instantiated initialiser, on
    // use (§11.9).
    if !definition_class {
        match eval.state() {
            VarEvalState::Evaluating => {
                s.indeterminate(EVALUATING);
            }
            // What Kiln found is the resolved unit's, not yet written.
            VarEvalState::NotConstant | VarEvalState::Value(_) => {
                s.pending(Area::Demand);
            }
            // Not evaluated yet: with an initialiser, the value is still the
            // resolved unit's, whenever Kiln evaluates it (it evaluates an
            // out-of-line `const int S::x = 1;` at once, the in-class
            // `static const int x = 1;` later).
            VarEvalState::NotEvaluated => {
                if init.is_some() {
                    s.pending(Area::Demand);
                }
            }
        }
    }
    // templ: record TemplLink.
    crate::templ::templ_link(w, s, d, templ);
    linkage(w, s, e);
}

/// The declaration of variable `e`'s chain that holds its initialiser,
/// block-scope `extern`s left out (Kiln reads the first that has one,
/// `kiln_oven/src/value.rs:89-98`); `Err` when two hold one.
fn init_holder(w: &Walk<'_>, e: DeclId) -> Result<Option<DeclId>, ()> {
    let ast = w.ast;
    let mut chain = w.redecls(e);
    chain.reverse();
    let mut holders = chain
        .into_iter()
        .filter(|&x| !w.is_local_extern(x))
        .filter(|&x| ast.decls.var(x).is_some_and(|v| v.init.is_some()));
    let first = holders.next();
    if holders.next().is_some() { Err(()) } else { Ok(first) }
}

/// The type a variable's unit holds: the type of the declaration read,
/// unless it is an array of unknown bound that another declaration of
/// the chain (not a block-scope `extern`) completes ([basic.types.general]/6);
/// then the first such declaration's, first declaration first.
pub(crate) fn var_type(w: &Walk<'_>, d: DeclId, e: DeclId) -> Option<QualType> {
    let ast = w.ast;
    let own = ast.decls.get(d).ty();
    let incomplete = |q: QualType| ast.types.canonical_kind(q) == TypeKind::IncompleteArray;
    if !own.is_some_and(incomplete) {
        return own;
    }
    let mut chain = w.redecls(e);
    chain.reverse();
    chain
        .into_iter()
        .filter(|&x| !w.is_local_extern(x))
        .filter_map(|x| ast.decls.get(x).ty())
        .find(|&q| !incomplete(q))
        .or(own)
}

/// `record VarSpec` of the declaration read (`spec`), merged over the
/// declarations of `e` as §2.3 and `spec/landed/digest-kinds/WP-026.md`
/// say.
fn var_spec(
    w: &mut Walk<'_>,
    s: &mut Stream,
    e: DeclId,
    spec: VarSpec,
    style: InitStyle,
    definition_class: bool,
) {
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
    let style = if definition_class { InitStyle::C } else { style };
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
    let instantiated = function.is_some_and(|f| instantiated_decl(w, f));
    // default_arg: child?. A parameter of an instantiated declaration
    // writes an absent child whatever Kiln holds: its default is the
    // `default_argument` derived unit's (§11.4). Else the default a
    // declaration of the function that is not instantiated and not at
    // block scope gives (§2.3).
    let default = if instantiated {
        None
    } else {
        let own = default_arg.filter(|_| function.is_none_or(|f| !w.is_local_extern(f)));
        own.or_else(|| function.and_then(|f| merged_default(w, f, index)))
    };
    if !instantiated && default.is_none() && flags & ParamData::DEFAULT_ARG_PENDING != 0 {
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
/// function `f`'s entity, the first declaration first (§2.3), leaving out
/// block-scope declarations ([dcl.fct.default]/4: theirs are their own,
/// `local::write_decl`) and instantiated ones.
fn merged_default(w: &Walk<'_>, f: DeclId, index: u16) -> Option<slip_kiln::kiln::ast::ExprId> {
    let ast = w.ast;
    let mut chain = w.redecls(ast.first(f));
    chain.reverse();
    chain.iter().filter(|&&x| !w.is_local_extern(x) && !instantiated_decl(w, x)).find_map(|&x| {
        let data = ast.decls.function(x)?;
        let p = ast.lists.iter::<DeclId>(data.params).nth(usize::from(index))?;
        ast.decls.param(p)?.default_arg
    })
}

/// `LetData` (§5.4).
pub(crate) fn let_data(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    let Some(&LetData { extending, mangling_index, value }) = w.ast.decls.lifetime_extended(d)
    else {
        s.uv(0).indeterminate(NO_PAYLOAD);
        return;
    };
    // extending: ref; mangling_index: uv; value: demand (§11.3).
    w.write_ref(s, extending);
    s.uv(u64::from(mangling_index));
    if value.is_some() {
        s.pending(Area::Demand);
    }
}

/// The language linkage a function's or variable's stream holds
/// (§5.6 `ScopeData`): `enum LinkageLang?` of the innermost
/// `LinkageSpec` around the first declaration `e`, through classes and
/// functions. `aux-braceless` is `out`: what it changes, whether a
/// variable's declaration is a definition, the unit holds already. The
/// reading is the one the key's parent uses (`Walk::linkage_spec`).
fn linkage(w: &mut Walk<'_>, s: &mut Stream, e: DeclId) {
    match w.linkage_spec(e, true) {
        Some((_, None)) => {
            s.uv(0).indeterminate(LANG_UNACCOUNTED);
        }
        Some((spec, Some(lang))) => {
            let aux = w.ast.decls.scope(spec).map_or(0, |x| x.aux);
            if aux & !(0xff | ScopeData::LINKAGE_BRACELESS) != 0 {
                s.indeterminate(SCOPE_AUX_UNACCOUNTED);
            }
            s.opt(true).tag(linkage_lang::row(&lang));
        }
        None => {
            s.opt(false);
        }
    }
}

/// The rows of a function's resolved unit `definition` (§11.9), read
/// from its defining declaration `def`, in this order: the parameters of
/// `def` as the own unit writes them (so that the body's local entities
/// are numbered in this stream), `FunctionData` `body`, `ColdDeclData`
/// `ctor_inits`, the flag word `FnSpec` `0-is-coroutine`, `0-escalated`
/// (set if any declaration sets them), and, when the pattern's declared
/// return type holds a placeholder (`demand::pattern_holds_placeholder`),
/// `DeclNode` `ty-type` and the flag word `FnSpec` `0-return-deduced`.
pub(crate) fn function_definition(w: &mut Walk<'_>, s: &mut Stream, e: DeclId, def: DeclId) {
    let ast = w.ast;
    let Some(&FunctionData { params, body, .. }) = ast.decls.function(def) else {
        s.uv(0).indeterminate(NO_PAYLOAD);
        return;
    };
    let params: Vec<DeclId> = ast.lists.iter::<DeclId>(params).collect();
    alias_params(w, e, &params);
    s.count(params.len());
    for p in params {
        crate::local::write_decl(w, s, p);
    }
    write_body(w, s, body);
    let inits: Vec<_> = ast.ctor_inits(def).copied().collect();
    s.count(inits.len());
    for init in &inits {
        crate::decl_class::ctor_init(w, s, init);
    }
    let mut specs: Vec<FnSpec> =
        w.redecls(e).iter().filter_map(|&x| ast.decls.function(x).map(|f| f.spec)).collect();
    if let Some(f) = ast.decls.function(def) {
        specs.push(f.spec);
    }
    let any = |f: fn(FnSpec) -> bool| specs.iter().any(|&x| f(x));
    let inline = any(FnSpec::is_inline);
    s.flags(
        FlagWord::new()
            .bit(!inline && any(FnSpec::implicit_inline))
            .bit(any(FnSpec::has_contracts))
            .bit(any(FnSpec::is_coroutine))
            .bit(any(FnSpec::escalated)),
    );
    crate::decl_scope::contracts(w, s, def);
    crate::attrs::definition_attrs(w, s, e);
    if crate::demand::pattern_holds_placeholder(w, e) {
        match ast.decls.get(def).ty() {
            Some(ty) => {
                s.opt(true);
                let td = w.type_digest(ty, true);
                s.digest(&td.digest).absorb(td.cause);
            }
            None => {
                s.opt(false);
            }
        }
        s.flags(FlagWord::new().bit(any(FnSpec::return_deduced)));
    }
}

/// The rows of a variable's resolved unit `definition` (§11.9), for the
/// defining declaration `def`, in this order: `VarData` `init` (of the
/// declaration that holds it, as the own unit reads it otherwise),
/// `VarSpec` `0-init-style`, the flag word `0-inline`, `0-inline-implied`
/// (merged over the declarations as the own unit merges them), and,
/// when the pattern's declared type holds a placeholder, `DeclNode`
/// `ty-type`.
pub(crate) fn var_definition(w: &mut Walk<'_>, s: &mut Stream, e: DeclId, def: DeclId) {
    let ast = w.ast;
    let Some(var) = ast.decls.var(def) else {
        s.uv(0).indeterminate(NO_PAYLOAD);
        return;
    };
    let (init, style) = match init_holder(w, e) {
        Ok(Some(holder)) => ast
            .decls
            .var(holder)
            .map_or((var.init, var.spec.init_style()), |h| (h.init, h.spec.init_style())),
        Ok(None) => (var.init, var.spec.init_style()),
        Err(()) => {
            s.indeterminate(TWO_INITS);
            (var.init, var.spec.init_style())
        }
    };
    // Without an initialiser in the pattern, what Kiln adds on use is
    // default-initialisation (`kiln_sema/src/templ/var.rs:212-221`): not
    // written (§11.9).
    let (init, style) =
        if crate::demand::pattern_has_init(w, e) { (init, style) } else { (None, InitStyle::C) };
    crate::expr::child_opt(w, s, init);
    s.tag(init_style::row(&style));
    let mut specs: Vec<VarSpec> =
        w.redecls(e).iter().filter_map(|&x| ast.decls.var(x).map(|xv| xv.spec)).collect();
    specs.push(var.spec);
    let inline = specs.iter().any(|x| x.is_inline());
    let inline_implied = inline && specs.iter().all(|x| !x.is_inline() || x.is_inline_implied());
    s.flags(FlagWord::new().bit(inline).bit(inline_implied));
    // The type: when the pattern's declared type holds a placeholder, or
    // when the own unit's is an array of unknown bound (the in-class
    // declaration of a static data member, completed by the instantiated
    // out-of-line definition, which the own unit does not read).
    let rep_ty = ast.decls.get(w.representative(e)).ty();
    let incomplete =
        rep_ty.is_some_and(|q| ast.types.canonical_kind(q) == TypeKind::IncompleteArray);
    if crate::demand::pattern_holds_placeholder(w, e) || incomplete {
        match var_type(w, def, e) {
            Some(ty) => {
                s.opt(true);
                let td = w.type_digest(ty, false);
                s.digest(&td.digest).absorb(td.cause);
            }
            None => {
                s.opt(false);
            }
        }
    }
}

/// `FunctionData` `body`: `child?`.
fn write_body(w: &mut Walk<'_>, s: &mut Stream, body: Option<StmtId>) {
    s.opt(body.is_some());
    if let Some(b) = body {
        crate::stmt::child(w, s, b);
    }
}

/// Whether the declaration `x` was made by instantiation
/// (`templ::instantiated_declaration`). A declaration written in source (an explicit
/// specialisation, or the out-of-line definition of one) is not.
fn instantiated_decl(w: &Walk<'_>, x: DeclId) -> bool {
    crate::templ::instantiated_declaration(w, x)
}

#[cfg(test)]
pub(crate) mod tests {
    use slip_kiln::kiln::ast::DeclId;
    use slip_kiln::kiln::ast::decl::{
        ConstexprKind, DeclKind, Defaulted, FnSpec, FnStorage, InitStyle, TlsKind, VarSpec,
        VarStorage,
    };
    use slip_kiln::kiln::ast::names::DeclNameView;
    use slip_model::keys::RuleTable;

    use crate::stream::Stream;
    use crate::walk::Walk;
    use crate::{Cause, Input};

    /// Runs `f` on a walk over `src` (`main.cpp`, C++26).
    pub(crate) fn with_walk<R>(src: &str, f: impl FnOnce(&mut Walk<'_>) -> R) -> R {
        let files = [("main.cpp", src)];
        let snippet = slip_kiln::snippet::Snippet {
            files: &files,
            main: "main.cpp",
            include: &[""],
            std: "c++26",
        };
        let rules = RuleTable::empty();
        slip_kiln::snippet::parse(&snippet, |p| {
            assert_eq!(p.errors, 0, "{}", p.diagnostics);
            let path = |f| p.path(f).map(|s: &str| s.as_bytes().to_vec()).unwrap_or_default();
            let input = Input { ast: p.ast, sm: p.sm, subject_path: &path };
            let mut w = Walk::new(&input, &rules);
            f(&mut w)
        })
        .expect("the snippet parses")
    }

    /// Every declaration of `kind` named `name` (an identifier), in arena
    /// order.
    pub(crate) fn named(w: &Walk<'_>, name: &str, kind: DeclKind) -> Vec<DeclId> {
        let ast = w.ast;
        ast.decls
            .ids()
            .filter(|&d| {
                let n = ast.decls.get(d);
                let ident = match ast.names.view(n.name()) {
                    DeclNameView::Identifier(s) => ast.symbols().as_str(s) == name,
                    DeclNameView::Empty
                    | DeclNameView::Operator(_)
                    | DeclNameView::Constructor(_)
                    | DeclNameView::Destructor(_)
                    | DeclNameView::Conversion(_)
                    | DeclNameView::LiteralOperator(_)
                    | DeclNameView::DeductionGuide(_) => false,
                };
                n.kind() == kind && ident
            })
            .collect()
    }

    /// Every declaration of `kind`, in arena order.
    pub(crate) fn of_kind(w: &Walk<'_>, kind: DeclKind) -> Vec<DeclId> {
        w.ast.decls.ids().filter(|&d| w.ast.decls.get(d).kind() == kind).collect()
    }

    fn written(
        w: &mut Walk<'_>,
        f: impl FnOnce(&mut Walk<'_>, &mut Stream),
    ) -> (Vec<u8>, Option<Cause>) {
        let mut s = Stream::new();
        f(w, &mut s);
        (s.as_bytes().to_vec(), s.cause())
    }

    /// The bytes `linkage` writes for the first declaration named `name`.
    fn linkage_bytes(src: &str, name: &str, kind: DeclKind) -> (Vec<u8>, Option<Cause>) {
        with_walk(src, |w| {
            let d = named(w, name, kind)[0];
            written(w, |w, s| super::linkage(w, s, d))
        })
    }

    // digest-kinds §5.4 FnSpec: every accessor's bit survives the rebuild,
    // so the unaccounted check fails only for a bit no row reads.
    #[test]
    fn rebuild_keeps_every_accounted_bit() {
        let f = FnSpec::new()
            .with_storage(FnStorage::Extern)
            .with_inline(true)
            .with_implicit_inline(true)
            .with_constexpr_kind(ConstexprKind::Consteval)
            .with_virtual(true)
            .with_pure(true)
            .with_defaulted(Defaulted::Implicit)
            .with_deleted(true)
            .with_explicit(true)
            .with_trivial(true)
            .with_immediate_escalating(true)
            .with_has_contracts(true)
            .with_coroutine(true)
            .with_escalated(true)
            .with_variadic(true)
            .with_override(true)
            .with_final(true)
            .with_explicit_object(true)
            .with_virtual_written(true)
            .with_storage_inherited(true)
            .with_not_selected(true)
            .with_return_deduced(true);
        assert_eq!(super::rebuild_fn_spec(f).bits(), f.bits());
        let v = VarSpec::new()
            .with_storage(VarStorage::Register)
            .with_tls(TlsKind::Dynamic)
            .with_init_style(InitStyle::ParenList)
            .with_constexpr(true)
            .with_constinit(true)
            .with_inline(true)
            .with_nrvo(true)
            .with_exception_var(true)
            .with_cxx_for_range(true)
            .with_inline_implied(true);
        assert_eq!(super::rebuild_var_spec(v).bits(), v.bits());
    }

    // digest-kinds §5.4 FnSpec: each bit row read from the declaration
    // has its place in the flag word, in table order (rule 6 for the rows
    // whose pair test also changes another unit or the key).
    #[test]
    fn fn_spec_bits_in_table_order() {
        type Set = fn(FnSpec) -> FnSpec;
        // Places 0 and 1 (`0-inline`, `0-implicit-inline`) are merged
        // from the declarations and not taken from `spec`.
        let rows: [(usize, Set); 14] = [
            (2, |x| x.with_virtual(true)),
            (3, |x| x.with_pure(true)),
            (4, |x| x.with_deleted(true)),
            (5, |x| x.with_explicit(true)),
            (6, |x| x.with_trivial(true)),
            (7, |x| x.with_immediate_escalating(true)),
            (8, |x| x.with_has_contracts(true)),
            (9, |x| x.with_coroutine(true)),
            (10, |x| x.with_escalated(true)),
            (11, |x| x.with_variadic(true)),
            (12, |x| x.with_final(true)),
            (13, |x| x.with_explicit_object(true)),
            (14, |x| x.with_not_selected(true)),
            (15, |x| x.with_return_deduced(true)),
        ];
        with_walk("void f();", |w| {
            let f = named(w, "f", DeclKind::Function)[0];
            for (place, set) in rows {
                let (got, cause) =
                    written(w, |w, s| super::fn_spec(w, s, f, set(FnSpec::new()), false));
                assert_eq!(cause, None, "place {place}");
                let mut word = crate::stream::FlagWord::new();
                for i in 0..16 {
                    word = word.bit(i == place);
                }
                let mut want = Stream::new();
                want.tag(crate::tables::fn_storage::row(&FnStorage::None))
                    .flags(word)
                    .tag(crate::tables::constexpr_kind::row(&ConstexprKind::None))
                    .tag(crate::tables::defaulted::row(&Defaulted::No));
                assert_eq!(got, want.as_bytes(), "place {place}");
            }
        });
    }

    // digest-kinds §5.4 FnSpec `0-defaulted`, Defaulted: the tag read from
    // the declaration, last in the record; its pair tests also change
    // other rows (a defaulted function is of the definition class).
    #[test]
    fn fn_spec_defaulted_tag() {
        with_walk("void f();", |w| {
            let f = named(w, "f", DeclKind::Function)[0];
            for d in [Defaulted::No, Defaulted::Explicit, Defaulted::Implicit] {
                let (got, _) = written(w, |w, s| {
                    super::fn_spec(w, s, f, FnSpec::new().with_defaulted(d), false);
                });
                let mut word = crate::stream::FlagWord::new();
                for _ in 0..16 {
                    word = word.bit(false);
                }
                let mut want = Stream::new();
                want.tag(crate::tables::fn_storage::row(&FnStorage::None))
                    .flags(word)
                    .tag(crate::tables::constexpr_kind::row(&ConstexprKind::None))
                    .tag(crate::tables::defaulted::row(&d));
                assert_eq!(got, want.as_bytes(), "{d:?}");
            }
        });
    }

    // digest §4.3, WP-026: parameter i of every declaration of the chain
    // is the local entity of parameter i of the declaration read, so a
    // default argument taken from another declaration names it.
    #[test]
    fn parameters_of_the_chain_share_ordinals() {
        with_walk("void f(int a, int b = sizeof(a));\nvoid f(int c, int d) {}", |w| {
            let def = named(w, "f", DeclKind::Function)[1];
            let (_, _) = written(w, |w, s| super::function_data(w, s, def));
            let ordinal = |w: &Walk<'_>, n: &str| {
                let p = named(w, n, DeclKind::ParmVar)[0];
                w.unit.locals.get(&w.ast.first(p)).copied()
            };
            assert_eq!(ordinal(w, "c"), Some(0));
            assert_eq!(ordinal(w, "d"), Some(1));
            assert_eq!(ordinal(w, "a"), Some(0));
            assert_eq!(ordinal(w, "b"), Some(1));
            assert_eq!(w.unit.next_local, 2);
        });
    }

    // digest-kinds §5.4 ParamData `flags-pack`, `flags-explicit-object`:
    // the bits are in the parameter's own record (their pair tests also
    // change the key).
    #[test]
    fn param_flag_bits_are_written() {
        let src = "template <class... T> void f(T... a);\nstruct C { void m(this C& b); };\nvoid g(int c);";
        with_walk(src, |w| {
            let mut bytes = Vec::new();
            for n in ["a", "b", "c"] {
                let p = named(w, n, DeclKind::ParmVar)[0];
                let (b, _) = written(w, |w, s| super::param_data(w, s, p));
                bytes.push(b);
            }
            let flag = |explicit_object: bool, pack: bool| {
                let mut s = Stream::new();
                s.opt(false)
                    .opt(false)
                    .flags(crate::stream::FlagWord::new().bit(explicit_object).bit(pack).bit(false))
                    .uv(0);
                s.as_bytes().to_vec()
            };
            assert_eq!(bytes[0], flag(false, true));
            assert_eq!(bytes[1], flag(true, false));
            assert_eq!(bytes[2], flag(false, false));
        });
    }

    // digest-kinds §5.6 ScopeData rule: `enum LinkageLang?`, absent with
    // no `LinkageSpec`; `aux-braceless` is `out` (WP-033 review).
    #[test]
    fn linkage_is_an_optional_language() {
        let (none, c0) = linkage_bytes("void f();", "f", DeclKind::Function);
        assert_eq!(c0, None);
        let mut want = Stream::new();
        want.opt(false);
        assert_eq!(none, want.as_bytes());
        let (braceless, c1) = linkage_bytes("extern \"C\" void f();", "f", DeclKind::Function);
        assert_eq!(c1, None);
        let mut want = Stream::new();
        want.opt(true)
            .tag(crate::tables::linkage_lang::row(&slip_kiln::kiln::ast::decl::LinkageLang::C));
        assert_eq!(braceless, want.as_bytes());
        let (braced, _) = linkage_bytes("extern \"C\" { void f(); }", "f", DeclKind::Function);
        assert_eq!(braced, braceless);
        // Through an enclosing class and an enclosing function.
        let (member, _) =
            linkage_bytes("extern \"C\" { struct C { void m(); }; }", "m", DeclKind::CXXMethod);
        assert_eq!(member, braced);
        let (local, _) = linkage_bytes("extern \"C\" { void g() { int x; } }", "x", DeclKind::Var);
        let (top, _) = linkage_bytes("extern \"C\" { int x; }", "x", DeclKind::Var);
        assert_eq!(local, top);
    }

    // digest-kinds §11.9: the definition class, and WP-026's addition of
    // a function defaulted on its first declaration.
    #[test]
    fn definition_class() {
        let src = "struct S { S() = default; };\n\
                   struct T { T(); };\nT::T() = default;\n\
                   template <class U> struct Q { void g() {} static inline int s = 1; };\n\
                   Q<int> q;\n\
                   template <> struct Q<char> { void g() {} };\n\
                   void h() {}\n";
        with_walk(src, |w| {
            let ctors = of_kind(w, DeclKind::CXXConstructor);
            let user: Vec<DeclId> = ctors
                .iter()
                .copied()
                .filter(|&c| {
                    !w.ast.decls.get(c).has(slip_kiln::kiln::ast::decl::DeclFlags::IMPLICIT)
                })
                .collect();
            // S::S() defaulted on its first declaration; T::T() not.
            let s = w.ast.first(user[0]);
            let t = w.ast.first(user[1]);
            assert!(crate::demand::definition_class(w, s));
            assert!(!crate::demand::definition_class(w, t));
            // An implicitly declared member.
            let implicit = ctors
                .iter()
                .copied()
                .find(|&c| w.ast.decls.get(c).has(slip_kiln::kiln::ast::decl::DeclFlags::IMPLICIT))
                .expect("an implicit constructor");
            assert!(crate::demand::definition_class(w, w.ast.first(implicit)));
            // Q<int>::g is an instantiation; Q::g (the pattern), Q<char>::g
            // (a member of an explicit specialisation) and h are not.
            let gs = named(w, "g", DeclKind::CXXMethod);
            let classes: Vec<bool> =
                gs.iter().map(|&g| crate::demand::definition_class(w, w.ast.first(g))).collect();
            assert_eq!(classes.iter().filter(|&&b| b).count(), 1, "{classes:?}");
            let h = named(w, "h", DeclKind::Function)[0];
            assert!(!crate::demand::definition_class(w, h));
            let ss = named(w, "s", DeclKind::Var);
            let vs: Vec<bool> = ss.iter().map(|&v| crate::demand::definition_class(w, v)).collect();
            assert_eq!(vs.iter().filter(|&&b| b).count(), 1, "{vs:?}");
        });
    }

    // WP-032 rule 1: a comparison an instantiation declares first, as a
    // friend, and that is then defaulted at namespace scope is defaulted on
    // its first written declaration, the representative, not on Kiln's
    // first declaration: of the definition class.
    #[test]
    fn defaulted_after_friend_instantiation() {
        let src = "template <class T> struct E { friend bool operator==(const E&, const E&); };\n\
                   E<int> e;\n\
                   bool operator==(const E<int>&, const E<int>&) = default;\n";
        with_walk(src, |w| {
            let written = *of_kind(w, DeclKind::Function).last().expect("the written ==");
            let e = w.entity(written);
            assert_ne!(e, written, "Kiln's first declaration is the instantiation's");
            assert_eq!(w.representative(e), written);
            assert!(crate::demand::definition_class(w, e));
        });
    }

    // digest-kinds §11.9: for the definition class the body is absent from
    // the own unit, without a cause, whether or not Kiln made it; the
    // resolved unit `definition` holds it.
    #[test]
    fn definition_class_body_is_in_definition() {
        with_walk(
            "template <class U> struct Q { void g() {} };\nQ<int> q;\nvoid h() { q.g(); }\n",
            |w| {
                let gs = named(w, "g", DeclKind::CXXMethod);
                let inst = gs
                    .iter()
                    .copied()
                    .find(|&g| crate::demand::definition_class(w, w.ast.first(g)))
                    .expect("Q<int>::g");
                assert!(w.ast.decls.function(inst).is_some_and(|f| f.body.is_some()));
                let (bytes, cause) = written(w, |w, s| super::function_data(w, s, inst));
                assert_eq!(cause, None);
                let e = w.entity(inst);
                assert_eq!(crate::demand::defining_declaration(w, e), Some(inst));
                let (def, cause) = written(w, |w, s| super::function_definition(w, s, e, inst));
                assert_ne!(cause, Some(Cause::Pending(crate::Area::Demand)));
                assert_ne!(bytes, def);
            },
        );
    }

    // A declaration of another kind than the record read has no payload:
    // an unaccounted unit, not a panic.
    #[test]
    fn a_missing_payload_is_unaccounted() {
        with_walk("int x; void f(int p);", |w| {
            let x = named(w, "x", DeclKind::Var)[0];
            let f = named(w, "f", DeclKind::Function)[0];
            for (_, c) in [
                written(w, |w, s| super::function_data(w, s, x)),
                written(w, |w, s| super::param_data(w, s, x)),
                written(w, |w, s| super::let_data(w, s, x)),
                written(w, |w, s| super::var_data(w, s, f)),
            ] {
                assert_eq!(c, Some(super::NO_PAYLOAD));
            }
        });
    }

    // digest-kinds §2.3, §5.4 ParamData `default_arg`: a default from the
    // declaration that gives it; none in an implicit instantiation (§11.4).
    #[test]
    fn param_default_merge() {
        with_walk("void f(int a); void f(int a = 1); void f(int a);", |w| {
            let ps = named(w, "a", DeclKind::ParmVar);
            assert_eq!(ps.len(), 3);
            let fs = named(w, "f", DeclKind::Function);
            assert!(super::merged_default(w, fs[0], 0).is_some());
            assert!(super::merged_default(w, fs[0], 1).is_none());
        });
    }
    /// The `var_data` bytes and cause of the declaration the walk reads for
    /// the variable whose first declaration is named `name`.
    fn var_bytes(src: &str, name: &str) -> (Vec<u8>, Option<Cause>) {
        with_walk(src, |w| {
            let e = w.ast.first(named(w, name, DeclKind::Var)[0]);
            let d = crate::units::read_decl(w, e);
            written(w, |w, s| super::var_data(w, s, d))
        })
    }

    // §2.3, landed WP-026 "The declaration read": an in-class initialiser
    // of a member defined out of line is the unit's `init` child (the
    // first byte, `opt`), and its value is pending the resolved unit.
    #[test]
    fn the_initialiser_comes_from_its_holder() {
        let (held, cause) =
            var_bytes("struct S { static const int x = 1; };\nconst int S::x;", "x");
        assert_eq!(held[0], 1);
        assert_eq!(cause, Some(Cause::Pending(crate::Area::Demand)));
        let (none, _) = var_bytes("struct S { static int x; };\nint S::x;", "x");
        assert_eq!(none[0], 0);
        let (out_of_line, _) =
            var_bytes("struct S { static const int x; };\nconst int S::x = 1;", "x");
        assert_eq!(held, out_of_line);
    }

    // §2.3, landed WP-026: a variable's type is the completed one,
    // whichever declaration is read.
    #[test]
    fn a_variable_type_is_the_completed_one() {
        use slip_kiln::kiln::ast::types::TypeKind;
        for src in ["extern int a[];\nextern int a[3];", "extern int a[3];\nextern int a[];"] {
            with_walk(src, |w| {
                let ds = named(w, "a", DeclKind::Var);
                let e = w.ast.first(ds[0]);
                for &d in &ds {
                    let q = super::var_type(w, d, e).expect("a type");
                    assert_eq!(w.ast.types.canonical_kind(q), TypeKind::ConstantArray, "{src}");
                }
            });
        }
    }
}
