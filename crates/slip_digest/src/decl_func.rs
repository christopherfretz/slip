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
    let specs: Vec<FnSpec> =
        w.redecls(e).iter().filter_map(|&x| ast.decls.function(x).map(|f| f.spec)).collect();
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
fn merged_default(w: &Walk<'_>, f: DeclId, index: u16) -> Option<slip_kiln::kiln::ast::ExprId> {
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

    // digest-kinds §5.6 ScopeData rule: `enum LinkageLang?`, then
    // `aux-braceless` as `bool`; absent with no `LinkageSpec`.
    #[test]
    fn linkage_is_an_optional_language_then_braceless() {
        let (none, c0) = linkage_bytes("void f();", "f", DeclKind::Function);
        assert_eq!(c0, None);
        let mut want = Stream::new();
        want.opt(false);
        assert_eq!(none, want.as_bytes());
        let (braceless, c1) = linkage_bytes("extern \"C\" void f();", "f", DeclKind::Function);
        assert_eq!(c1, None);
        let mut want = Stream::new();
        want.opt(true)
            .tag(crate::tables::linkage_lang::row(&slip_kiln::kiln::ast::decl::LinkageLang::C))
            .bool(true);
        assert_eq!(braceless, want.as_bytes());
        let (braced, _) = linkage_bytes("extern \"C\" { void f(); }", "f", DeclKind::Function);
        assert_ne!(braced, braceless);
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
            assert!(super::function_definition_class(w, s));
            assert!(!super::function_definition_class(w, t));
            // An implicitly declared member.
            let implicit = ctors
                .iter()
                .copied()
                .find(|&c| w.ast.decls.get(c).has(slip_kiln::kiln::ast::decl::DeclFlags::IMPLICIT))
                .expect("an implicit constructor");
            assert!(super::function_definition_class(w, w.ast.first(implicit)));
            // Q<int>::g is an instantiation; Q::g (the pattern), Q<char>::g
            // (a member of an explicit specialisation) and h are not.
            let gs = named(w, "g", DeclKind::CXXMethod);
            let classes: Vec<bool> =
                gs.iter().map(|&g| super::implicit_instantiation(w, w.ast.first(g))).collect();
            assert_eq!(classes.iter().filter(|&&b| b).count(), 1, "{classes:?}");
            let h = named(w, "h", DeclKind::Function)[0];
            assert!(!super::function_definition_class(w, h));
            let ss = named(w, "s", DeclKind::Var);
            let vs: Vec<bool> = ss.iter().map(|&v| super::var_definition_class(w, v)).collect();
            assert_eq!(vs.iter().filter(|&&b| b).count(), 1, "{vs:?}");
        });
    }

    // digest-kinds §11.9: for the definition class the body is absent and
    // the unit `Pending(Demand)` until resolved units exist.
    #[test]
    fn definition_class_body_is_pending_demand() {
        with_walk(
            "template <class U> struct Q { void g() {} };\nQ<int> q;\nvoid h() { q.g(); }\n",
            |w| {
                let gs = named(w, "g", DeclKind::CXXMethod);
                let inst = gs
                    .iter()
                    .copied()
                    .find(|&g| super::implicit_instantiation(w, w.ast.first(g)))
                    .expect("Q<int>::g");
                assert!(w.ast.decls.function(inst).is_some_and(|f| f.body.is_some()));
                let (_, cause) = written(w, |w, s| super::function_data(w, s, inst));
                assert_eq!(cause, Some(Cause::Pending(crate::Area::Demand)));
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
}
