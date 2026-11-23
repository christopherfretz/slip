//! Profile mode (`spec/digest-kinds.md` §4.4, WP-036): the walk of an
//! expression whose bytes are those of its canonical profile.
//!
//! Kiln's profiler (`kiln_ast` `types/profile_expr.rs`) decides when two
//! expressions are equivalent ([temp.over.link]/5): a `ProfileId` is the
//! class of all its spellings. While a profile is walked (a `ProfileId`
//! operand of a type, and every expression written into a key), a fact is
//! written iff Kiln's profile distinguishes it, so the bytes are a
//! function of the profile and not of which spelling Kiln met first.
//!
//! The walk follows `Ast::profile_node` kind by kind and writes, for each
//! word of Kiln's, the digest's form of the same fact: a canonical type
//! as its type digest, a declaration as its `Ref` (the first
//! declaration) or by position (a template or function parameter, as
//! Kiln's `decl_words`), a name as its `Name`, a canonical qualifier,
//! argument list or template name as the digest writes it. It uses an
//! explicit stack, as Kiln's does, so an operator chain of any length
//! takes one Rust frame.
//!
//! What Kiln's profile does not hold is not written: the declarations
//! lookup found for an unresolved name (`UnresolvedLookupExpr.b`), the
//! types, value categories and dependence bits of nodes, implicit
//! conversions, temporaries and cleanups (transparent), the parameter a
//! `SubstNonTypeTemplateParmExpr` replaced (it is its replacement), the
//! stored result of a requires-expression or concept-id, a qualifier and
//! the declaration found through it (`DeclRefExtra`), non-dependent
//! explicit template arguments of a reference to a specialisation. A
//! left-out found set that held a declaration is counted in
//! `Walk::left_out`: a type digest whose stream left one out is marked
//! (`TypeRef::spelling`), and a unit stream that writes a marked type is
//! pending (`Area::Spelling`).
//!
//! A kind Kiln profiles as unique (equal only to itself: a lambda, a
//! statement expression, a pack of substituted parameters, ...) has no
//! equivalence to follow; it is walked in full by §7 (`expr::child`), its
//! bytes a function of its own spelling. So are the kinds whose profile
//! needs a fact the digest does not yet write (a reflection, a splice).

use slip_kiln::kiln::ast::decl::{DeclKind, ParamData};
use slip_kiln::kiln::ast::expr::{
    BinaryOp, CastKind, CharKind, Designator, ExprFlags, ExprKind as K, PredefinedKind,
    SourceLocKind, TraitKind, TypeTrait, TypeidOperand, UnaryOp,
};
use slip_kiln::kiln::ast::names::OverloadedOperator;
use slip_kiln::kiln::ast::templates::Requirement;
use slip_kiln::kiln::ast::types::{ArgView, Nns, TagKeyword, TemplateArg, TypeFlags, TypeView};
use slip_kiln::kiln::ast::{ArgListId, DeclId, ExprId, ListId, NnsId, QualType};

use crate::Cause;
use crate::stream::{FlagWord, Stream};
use crate::tables::{
    auto_keyword, binary_op, cast_kind, char_kind, expr_kind, new_init_style, overloaded_operator,
    predefined_kind, source_loc_kind, trait_kind, type_trait, unary_op, value_category,
};
use crate::walk::{TOO_DEEP, Walk};

/// How a declaration is written (Kiln's `decl_words`).
const D_TEMPLATE_PARM: u64 = 1;
const D_NTTP: u64 = 2;
const D_PARM: u64 = 3;
const D_DECL: u64 = 4;
const D_RESULT_NAME: u64 = 5;

/// A requirement of a requires-expression (Kiln's `R_*`).
const R_SIMPLE: u64 = 1;
const R_TYPE: u64 = 2;
const R_COMPOUND: u64 = 3;
const R_NESTED: u64 = 4;

/// Explicit template arguments: absent, the canonical list, or one by one
/// (a list holding a non-dependent expression, Kiln's `ARGS_EXPR`).
const ARGS_NONE: u64 = 0;
const ARGS_LIST: u64 = 1;
const ARGS_EACH: u64 = 2;

/// Whether a function parameter's canonical type is written with its
/// position (Kiln's `D_PARM` holds it): the nets of
/// `expr_profile_tests` decide.
const PARAM_TYPE: bool = true;

#[cfg(test)]
thread_local! {
    /// How many expressions were walked in full (`P::Full`): the nets
    /// leave those walks out.
    static FULL: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    /// The kinds profile mode met (`expr_profile_tests::the_bytes_are_pinned`).
    static VISITED: std::cell::RefCell<Vec<K>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The kinds met since the last call (tests only).
#[cfg(test)]
pub(crate) fn visited_take() -> Vec<K> {
    VISITED.with(std::cell::RefCell::take)
}

/// One step of the walk.
#[derive(Clone, Copy, Debug)]
enum P {
    /// Profile this expression.
    Expr(ExprId),
    /// `opt`, then the expression when present.
    Opt(Option<ExprId>),
    Uv(u64),
    /// A symbol (a field designator's name).
    Sym(slip_kiln::kiln::source::symbol::Symbol),
    Type(QualType),
    /// One template argument of a list written one by one.
    Arg(TemplateArg),
    /// Walk this expression in full (§7): a kind with no equivalence rule.
    Full(ExprId),
}

/// The bytes of profile mode for `root` (the frame and the expression
/// state are the caller's: `expr_dep::profile_expr`).
pub(crate) fn walk(w: &mut Walk<'_>, s: &mut Stream, root: ExprId) {
    if !w.enter_node() {
        s.uv(0).indeterminate(TOO_DEEP);
        return;
    }
    let mut stack = vec![P::Expr(root)];
    while let Some(p) = stack.pop() {
        match p {
            P::Expr(e) => node(w, s, e, &mut stack),
            P::Opt(None) => {
                s.opt(false);
            }
            P::Opt(Some(e)) => {
                s.opt(true);
                node(w, s, e, &mut stack);
            }
            P::Uv(v) => {
                s.uv(v);
            }
            P::Sym(name) => w.write_sym(s, name),
            P::Type(q) => w.write_type(s, q),
            P::Arg(a) => w.write_arg(s, a.view()),
            P::Full(e) => {
                #[cfg(test)]
                FULL.with(|c| c.set(c.get() + 1));
                crate::expr::child(w, s, e);
            }
        }
    }
    w.leave_node();
}

/// The kind's tag.
fn tag(s: &mut Stream, k: K) {
    s.tag(expr_kind::row(&k));
}

/// `opt` and the type.
fn type_opt(w: &mut Walk<'_>, s: &mut Stream, q: Option<QualType>) {
    w.write_type_opt(s, q);
}

/// A length-prefixed expression list (Kiln's `push_list`).
fn push_list(w: &Walk<'_>, l: ListId, stack: &mut Vec<P>) {
    let es: Vec<ExprId> = w.ast.lists.iter::<ExprId>(l).collect();
    stack.push(P::Uv(es.len() as u64));
    stack.extend(es.into_iter().map(P::Expr));
}

/// A list word that may be 0 (an empty list).
fn push_list_raw(w: &Walk<'_>, raw: u32, stack: &mut Vec<P>) {
    match ListId::try_new(raw) {
        Some(l) => push_list(w, l, stack),
        None => stack.push(P::Uv(0)),
    }
}

/// The canonical qualifier, or absent (Kiln's `nns_word`).
fn nns_opt(w: &mut Walk<'_>, s: &mut Stream, n: Option<NnsId>) {
    s.opt(n.is_some());
    if let Some(n) = n {
        w.write_nns(s, n);
    }
}

/// The words identifying declaration `d` (Kiln's `decl_words`): a
/// template parameter by depth, position and pack-ness (and a non-type
/// one's type), a function parameter by position, pack-ness, whether it
/// is a requires-expression's, its function-scope depth and its type,
/// anything else as the `Ref` of its first declaration.
fn decl_words(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    let ast = w.ast;
    let node = ast.decls.get(d);
    match node.kind() {
        DeclKind::TemplateTypeParm
        | DeclKind::NonTypeTemplateParm
        | DeclKind::TemplateTemplateParm => {
            let Some(p) = ast.template_parm(d) else {
                s.uv(D_DECL);
                w.write_ref(s, ast.first(d));
                return;
            };
            let (depth, index, pack) = (p.depth, p.index, p.pack);
            if node.kind() == DeclKind::NonTypeTemplateParm {
                s.uv(D_NTTP).uv(u64::from(depth)).uv(u64::from(index)).bool(pack);
                nttp_type(w, s, node.ty());
            } else {
                s.uv(D_TEMPLATE_PARM).uv(u64::from(depth)).uv(u64::from(index)).bool(pack);
            }
        }
        DeclKind::ParmVar => {
            let (index, pack, depth) = ast.decls.param(d).map_or((0, false, 0), |p| {
                (u64::from(p.index), p.flags & ParamData::PACK != 0, p.scope_depth())
            });
            let in_requires =
                node.ctx().is_some_and(|c| ast.decls.get(c).kind() == DeclKind::RequiresExprBody);
            s.uv(D_PARM)
                .uv(index)
                .flags(FlagWord::new().bit(pack).bit(in_requires))
                .uv(u64::from(depth));
            if PARAM_TYPE {
                type_opt(w, s, node.ty());
            }
        }
        DeclKind::ContractResultName => {
            s.uv(D_RESULT_NAME);
        }
        _ => {
            s.uv(D_DECL);
            w.write_ref(s, ast.first(d));
        }
    }
}

/// The type of a non-type template parameter, a top-level placeholder's
/// constraint ignored (Kiln's `nttp_type_words`).
fn nttp_type(w: &mut Walk<'_>, s: &mut Stream, q: Option<QualType>) {
    let ast = w.ast;
    if let Some(q) = q {
        let c = ast.types.canonical(q);
        if let TypeView::Auto { keyword, deduced: None, concept: _, args: _, pack } =
            ast.types.view(c.ty())
        {
            s.uv(1).tag(auto_keyword::row(&keyword)).bool(pack).uv(u64::from(c.quals_bits()));
            return;
        }
    }
    s.uv(0);
    type_opt(w, s, q);
}

/// The written template arguments of a reference to `d` (Kiln's
/// `ref_args`): a non-dependent list naming a specialisation is implied
/// by it and left out.
fn ref_args(w: &mut Walk<'_>, s: &mut Stream, d: DeclId, l: Option<ArgListId>, stack: &mut Vec<P>) {
    let ast = w.ast;
    match l {
        Some(l)
            if !ast.types.arg_list_flags(l).intersects(TypeFlags::DEPENDENT_INSTANTIATION)
                && !matches!(
                    ast.decls.get(d).kind(),
                    DeclKind::FunctionTemplate | DeclKind::VarTemplate
                ) =>
        {
            s.uv(ARGS_NONE);
        }
        _ => push_args(w, s, l, stack),
    }
}

/// Explicit template arguments (Kiln's `push_args`): the canonical list,
/// or, when it holds a non-dependent expression, each argument, an
/// expression by its profile.
fn push_args(w: &mut Walk<'_>, s: &mut Stream, l: Option<ArgListId>, stack: &mut Vec<P>) {
    let Some(l) = l else {
        s.uv(ARGS_NONE);
        return;
    };
    let types = &w.ast.types;
    let canon = types.canonical_args(l);
    let args: Vec<TemplateArg> = types.args(canon).to_vec();
    if !args.iter().any(|a| matches!(a.view(), ArgView::Expr { profile: None, .. })) {
        s.uv(ARGS_LIST);
        w.write_args(s, canon);
        return;
    }
    s.uv(ARGS_EACH).count(args.len());
    for a in args {
        match a.view() {
            ArgView::Expr { expr, profile: None } => {
                stack.extend([P::Uv(0), P::Expr(expr)]);
            }
            _ => stack.extend([P::Uv(1), P::Arg(a)]),
        }
    }
}

/// The qualifier of a member access on an object of dependent type, each
/// level by name (Kiln's `spelled_nns_words`).
fn spelled_nns(w: &mut Walk<'_>, s: &mut Stream, nns: NnsId) {
    let ast = w.ast;
    let types = &ast.types;
    let mut chain = Vec::new();
    let mut cur = Some(nns);
    while let Some(n) = cur {
        chain.push(n);
        cur = match types.nns_view(n) {
            Nns::Namespace { prefix, ns: _ }
            | Nns::NamespaceAlias { prefix, alias: _, ns: _ }
            | Nns::Type { prefix, ty: _, template_kw: _ }
            | Nns::Identifier { prefix, name: _ } => prefix,
            Nns::Global | Nns::Splice(_) => None,
        };
    }
    s.count(chain.len());
    for &n in chain.iter().rev() {
        match types.nns_view(n) {
            Nns::Global => {
                s.uv(1);
            }
            Nns::Namespace { prefix: _, ns } => {
                s.uv(2);
                decl_name_or_ref(w, s, ns);
            }
            Nns::NamespaceAlias { prefix: _, alias, ns: _ } => {
                s.uv(3);
                decl_name_or_ref(w, s, alias);
            }
            Nns::Type { prefix: _, ty, template_kw: false }
                if let Some(sym) = spelled_type_name(w, ty) =>
            {
                s.uv(4);
                w.write_sym(s, sym);
            }
            Nns::Identifier { prefix: _, name } => {
                s.uv(5);
                w.write_sym(s, name);
            }
            Nns::Type { prefix: _, ty: _, template_kw: _ } | Nns::Splice(_) => {
                s.uv(6);
                w.write_nns(s, n);
            }
        }
    }
}

/// A declaration's identifier, or its `Ref` when its name is not one
/// (Kiln writes the identifier's symbol, else the declaration).
fn decl_name_or_ref(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    let ast = w.ast;
    if let slip_kiln::kiln::ast::names::DeclNameView::Identifier(sym) =
        ast.names.view(ast.decls.get(d).name())
    {
        s.uv(0);
        w.write_sym(s, sym);
    } else {
        s.uv(1);
        w.write_ref(s, d);
    }
}

/// The name `ty` is written with, when it is one identifier (Kiln's
/// private `spelled_type_name`, read through public views).
fn spelled_type_name(
    w: &Walk<'_>,
    mut ty: QualType,
) -> Option<slip_kiln::kiln::source::symbol::Symbol> {
    let ast = w.ast;
    let d = loop {
        match ast.types.view(ty.ty()) {
            TypeView::TemplateTypeParm(d) | TypeView::Record(d) | TypeView::Enum(d) => break d,
            TypeView::Typedef { decl, .. } => break decl,
            TypeView::Using { shadow, .. } => break shadow,
            TypeView::Elaborated { keyword: TagKeyword::None, nns: None, named } => ty = named,
            _ => return None,
        }
    };
    let node = ast.decls.get(d);
    if matches!(
        node.kind(),
        DeclKind::ClassTemplateSpecialization | DeclKind::ClassTemplatePartialSpecialization
    ) {
        return None;
    }
    match ast.names.view(node.name()) {
        slip_kiln::kiln::ast::names::DeclNameView::Identifier(sym) => Some(sym),
        _ => None,
    }
}

/// The template of a class template name to deduce (Kiln's `ctad_word`).
fn ctad(w: &mut Walk<'_>, s: &mut Stream, q: QualType) {
    let ast = w.ast;
    match ast.types.view(q.ty()) {
        TypeView::DeducedTemplateSpecialization { name, deduced }
            if deduced.is_none_or(|d| d == QualType::DEPENDENT) =>
        {
            s.opt(true);
            w.write_template_name(s, name);
        }
        _ => {
            s.opt(false);
        }
    }
}

/// A tag of a decoded `aux`, or `uv 0` and `cause`.
fn aux_tag(s: &mut Stream, row: Option<crate::tables::Row>, cause: &'static str) {
    match row {
        Some(r) => {
            s.tag(r);
        }
        None => {
            s.uv(0).indeterminate(Cause::Unaccounted(cause));
        }
    }
}

/// Writes the header of `e` and pushes the rest (Kiln's `profile_node`).
#[allow(clippy::too_many_lines, reason = "one arm per expression kind, as Kiln's profiler")]
#[allow(
    clippy::many_single_char_names,
    reason = "the names of digest-kinds: s stream, e expression, n node, k kind, x arena"
)]
fn node(w: &mut Walk<'_>, s: &mut Stream, e: ExprId, stack: &mut Vec<P>) {
    crate::walk::stack_probe();
    let ast = w.ast;
    let x = &ast.exprs;
    let n = *x.get(e);
    let k = n.kind();
    #[cfg(test)]
    VISITED.with(|v| v.borrow_mut().push(k));
    let aux = n.aux();
    let id = ExprId::try_new;
    let mark = stack.len();
    match k {
        // ----- transparent
        K::ImplicitCastExpr
        | K::ConstantExpr
        | K::MaterializeTemporaryExpr
        | K::CXXBindTemporaryExpr
        | K::ExprWithCleanups => transparent(s, id(n.a()), stack),
        K::SubstNonTypeTemplateParmExpr => transparent(s, id(n.b()), stack),
        K::OpaqueValueExpr => match x.opaque_source(e) {
            Some(src) => stack.push(P::Expr(src)),
            None => tag(s, k),
        },

        // ----- literals: value and type
        K::IntegerLiteral => {
            tag(s, k);
            w.write_type(s, n.ty());
            if let Some(v) = x.int_value(e) {
                s.uv(0).uv(v);
            } else {
                s.uv(1);
                literal_value(w, s, x.literal_const(e));
            }
        }
        K::FloatingLiteral => {
            tag(s, k);
            w.write_type(s, n.ty());
            literal_value(w, s, x.literal_const(e));
        }
        K::CharacterLiteral => {
            tag(s, k);
            aux_tag(
                s,
                CharKind::from_u8(aux).map(|c| char_kind::row(&c)),
                "profile/CharacterLiteral.aux",
            );
            w.write_type(s, n.ty());
            s.uv(u64::from(n.a()));
        }
        K::StringLiteral => {
            tag(s, k);
            w.write_type(s, n.ty());
            match slip_kiln::kiln::ast::StringLitId::try_new(n.a()) {
                Some(l) => crate::value::string(w, s, l),
                None => {
                    s.uv(0).indeterminate(Cause::Unaccounted("profile/StringLiteral.a"));
                }
            }
        }
        K::CXXBoolLiteralExpr => {
            tag(s, k);
            s.bool(x.bool_value(e));
        }
        K::AddrLabelExpr => {
            tag(s, k);
            match DeclId::try_new(n.a()) {
                Some(d) => w.write_ref(s, d),
                None => {
                    s.uv(0).indeterminate(Cause::Unaccounted("profile/AddrLabelExpr.a"));
                }
            }
        }
        K::CXXNullPtrLiteralExpr
        | K::GNUNullExpr
        | K::ImplicitValueInitExpr
        | K::CXXScalarValueInitExpr => {
            tag(s, k);
            w.write_type(s, n.ty());
        }

        // ----- names
        K::DeclRefExpr => {
            tag(s, k);
            let d = x.decl(e);
            decl_words(w, s, d);
            let l = x.decl_ref_extra(e).and_then(|r| r.template_args);
            ref_args(w, s, d, l, stack);
        }
        K::MemberExpr => {
            tag(s, k);
            s.bool(n.has(ExprFlags::ARROW));
            let d = x.decl(e);
            decl_words(w, s, d);
            let l = x.decl_ref_extra(e).and_then(|r| r.template_args);
            ref_args(w, s, d, l, stack);
            stack.push(P::Opt(id(n.a())));
        }
        K::CXXThisExpr => {
            tag(s, k);
            s.bool(x.this_is_implicit(e));
        }
        K::PredefinedExpr => {
            tag(s, k);
            let kind = PredefinedKind::ALL.get(usize::from(aux)).copied();
            aux_tag(s, kind.map(|p| predefined_kind::row(&p)), "profile/PredefinedExpr.aux");
        }
        K::SourceLocExpr => {
            tag(s, k);
            let kind = SourceLocKind::ALL.get(usize::from(aux)).copied();
            aux_tag(s, kind.map(|p| source_loc_kind::row(&p)), "profile/SourceLocExpr.aux");
        }
        K::UnresolvedLookupExpr => {
            tag(s, k);
            s.bool(aux & 1 != 0);
            let found: Vec<DeclId> = ListId::try_new(n.b())
                .map(|l| ast.lists.iter::<DeclId>(l).collect())
                .unwrap_or_default();
            match found.first() {
                // `V<args>` for a variable template template parameter:
                // the parameter by position.
                Some(&d) if ast.decls.get(d).kind() == DeclKind::TemplateTemplateParm => {
                    s.uv(1);
                    decl_words(w, s, d);
                }
                _ => {
                    s.uv(0);
                    crate::expr_dep::name_of(w, s, n.a());
                    if !found.is_empty() {
                        // The set found at the definition: not in the
                        // profile, held by the spelling (WP-041).
                        w.left_out += 1;
                    }
                }
            }
            nns_opt(w, s, ast.templates.expr_nns.get(e).copied());
            push_args(w, s, ast.templates.expr_targs.get(e).copied(), stack);
        }
        K::DependentScopeDeclRefExpr => {
            tag(s, k);
            nns_opt(w, s, NnsId::try_new(n.a()));
            crate::expr_dep::name_of(w, s, n.b());
            push_args(w, s, ast.templates.expr_targs.get(e).copied(), stack);
        }
        K::CXXDependentScopeMemberExpr => {
            let base = id(n.a());
            let nns = ast.templates.expr_nns.get(e).copied();
            let spelled = base
                .filter(|&b| {
                    let bn = x.get(b);
                    bn.is_type_dependent()
                        && !(bn.kind() == K::CXXThisExpr && x.this_is_implicit(b))
                })
                .and(nns);
            tag(s, k);
            s.flags(FlagWord::new().bit(base.is_some() && aux & 1 != 0).bit(spelled.is_some()));
            crate::expr_dep::name_of(w, s, n.b());
            nns_opt(w, s, if spelled.is_some() { None } else { nns });
            push_args(w, s, ast.templates.expr_targs.get(e).copied(), stack);
            if let Some(q) = spelled {
                spelled_nns(w, s, q);
            }
            stack.push(P::Opt(base));
        }
        K::SizeOfPackExpr => {
            tag(s, k);
            match DeclId::try_new(n.a()) {
                Some(d) => {
                    s.opt(true);
                    decl_words(w, s, d);
                }
                None => {
                    s.opt(false);
                }
            }
        }

        // ----- operators
        K::UnaryOperator => {
            tag(s, k);
            aux_tag(
                s,
                UnaryOp::from_u8(aux).map(|o| unary_op::row(&o)),
                "profile/UnaryOperator.aux",
            );
            stack.push(P::Opt(id(n.a())));
        }
        K::ParenExpr | K::PackExpansionExpr | K::CXXNoexceptExpr | K::IntegerPackExpr => {
            tag(s, k);
            stack.push(P::Opt(id(n.a())));
        }
        K::BinaryOperator => {
            tag(s, k);
            aux_tag(
                s,
                BinaryOp::from_u8(aux).map(|o| binary_op::row(&o)),
                "profile/BinaryOperator.aux",
            );
            stack.extend([P::Opt(id(n.a())), P::Opt(id(n.b()))]);
        }
        K::ArraySubscriptExpr
        | K::PackIndexingExpr
        | K::ArrayInitLoopExpr
        | K::DesignatedInitUpdateExpr => {
            tag(s, k);
            stack.extend([P::Opt(id(n.a())), P::Opt(id(n.b()))]);
        }
        K::CompoundAssignOperator => {
            tag(s, k);
            aux_tag(
                s,
                BinaryOp::from_u8(aux).map(|o| binary_op::row(&o)),
                "profile/CompoundAssignOperator.aux",
            );
            stack.extend([P::Opt(id(n.a())), P::Expr(x.compound_assign(e).rhs)]);
        }
        K::ConditionalOperator => {
            let [t, f] = x.cond_arms(e);
            tag(s, k);
            stack.extend([P::Opt(id(n.a())), P::Expr(t), P::Expr(f)]);
        }
        K::CXXFoldExpr => {
            tag(s, k);
            aux_tag(
                s,
                BinaryOp::from_u8(aux).map(|o| binary_op::row(&o)),
                "profile/CXXFoldExpr.aux",
            );
            stack.extend([P::Opt(id(n.a())), P::Opt(id(n.b()))]);
        }
        K::CallExpr | K::CXXMemberCallExpr => {
            tag(s, k);
            stack.push(P::Opt(id(n.a())));
            push_list_raw(w, n.b(), stack);
        }
        K::CXXOperatorCallExpr => {
            tag(s, k);
            aux_tag(
                s,
                OverloadedOperator::from_u8(aux).map(|o| overloaded_operator::row(&o)),
                "profile/CXXOperatorCallExpr.aux",
            );
            stack.push(P::Opt(id(n.a())));
            push_list_raw(w, n.b(), stack);
        }
        K::PseudoObjectExpr => stack.push(P::Expr(x.pseudo_syntactic(e, &ast.lists))),
        K::CXXRewrittenBinaryOperator => {
            let (op, written) = x.rewritten_written(e);
            tag(s, k);
            s.tag(binary_op::row(&op));
            push_list(w, written, stack);
        }
        K::BinaryConditionalOperator | K::ParenListExpr | K::InitListExpr => {
            tag(s, k);
            push_list_raw(w, n.a(), stack);
        }
        K::DesignatedInitExpr => {
            let ds = x.designators(e).to_vec();
            tag(s, k);
            s.count(ds.len());
            for d in ds {
                match d {
                    // On the stack, as Kiln's `Work::Word`s: written at
                    // once, a field would come before an earlier index's
                    // bounds, so `[i].f` and `.f[i]` would be one string.
                    Designator::Field { name, loc: _ } => stack.extend([P::Uv(1), P::Sym(name)]),
                    Designator::Index { index, .. } => stack.extend([P::Uv(2), P::Expr(index)]),
                    Designator::Range { first, last, .. } => {
                        stack.extend([P::Uv(3), P::Expr(first), P::Expr(last)]);
                    }
                }
            }
            stack.push(P::Opt(id(n.a())));
        }

        // ----- casts and construction: the written type
        K::CStyleCastExpr
        | K::CXXFunctionalCastExpr
        | K::CXXStaticCastExpr
        | K::CXXConstCastExpr
        | K::CXXReinterpretCastExpr
        | K::CXXDynamicCastExpr
        | K::BuiltinBitCastExpr => {
            tag(s, k);
            aux_tag(s, CastKind::from_u8(aux).map(|c| cast_kind::row(&c)), "profile/cast.aux");
            s.tag(value_category::row(&n.value_category()));
            w.write_type(s, n.ty());
            stack.push(P::Opt(id(n.a())));
        }
        K::CXXUnresolvedConstructExpr => {
            let t = x.unresolved_construct_type(e);
            tag(s, k);
            s.bool(aux & 1 != 0);
            w.write_type(s, t);
            ctad(w, s, t);
            push_list_raw(w, n.a(), stack);
        }
        K::CXXParenListInitExpr => {
            tag(s, k);
            w.write_type(s, n.ty());
            push_list_raw(w, n.a(), stack);
        }
        K::CXXConstructExpr | K::CXXTemporaryObjectExpr => {
            tag(s, k);
            w.write_type(s, n.ty());
            decl_words(w, s, x.constructor(e));
            push_list_raw(w, n.b(), stack);
        }
        K::CompoundLiteralExpr => {
            tag(s, k);
            w.write_type(s, n.ty());
            stack.push(P::Opt(id(n.a())));
        }
        K::CXXDefaultArgExpr => {
            tag(s, k);
            decl_words(w, s, x.default_arg_param(e));
        }
        K::CXXDefaultInitExpr => {
            tag(s, k);
            decl_words(w, s, x.default_init_field(e));
        }
        K::ArrayInitIndexExpr | K::NoInitExpr => tag(s, k),

        // ----- memory, exceptions, RTTI
        K::CXXNewExpr => {
            let ne = *x.new_extra(e);
            let style = x.new_init_style(e);
            tag(s, k);
            s.flags(
                FlagWord::new().bit(n.has(ExprFlags::GLOBAL)).bit(n.has(ExprFlags::ARRAY_FORM)),
            );
            s.tag(new_init_style::row(&style));
            w.write_type(s, ne.allocated_ty);
            stack.extend([P::Opt(ne.array_size), P::Opt(ne.init)]);
            match ne.placement {
                Some(p) => push_list(w, p, stack),
                None => stack.push(P::Uv(0)),
            }
        }
        K::CXXDeleteExpr => {
            tag(s, k);
            s.flags(
                FlagWord::new().bit(n.has(ExprFlags::GLOBAL)).bit(n.has(ExprFlags::ARRAY_FORM)),
            );
            stack.push(P::Opt(id(n.a())));
        }
        K::CXXThrowExpr => {
            tag(s, k);
            stack.push(P::Opt(x.throw_operand(e)));
        }
        K::CXXPseudoDestructorExpr => {
            let (base, arrow, destroyed) = x.pseudo_destructor(e);
            tag(s, k);
            s.bool(arrow);
            w.write_type(s, destroyed);
            stack.push(P::Expr(base));
        }
        K::CXXTypeidExpr => {
            tag(s, k);
            match x.typeid_operand(e) {
                TypeidOperand::Type(t) => {
                    s.uv(1);
                    w.write_type(s, t);
                }
                TypeidOperand::Expr(o) => {
                    s.uv(0);
                    stack.push(P::Expr(o));
                }
            }
        }

        // ----- traits
        K::UnaryExprOrTypeTraitExpr => {
            tag(s, k);
            aux_tag(
                s,
                TraitKind::from_u8(aux).map(|t| trait_kind::row(&t)),
                "profile/UnaryExprOrTypeTraitExpr.aux",
            );
            if n.has(ExprFlags::ARG_IS_TYPE) {
                s.uv(1);
                type_opt(w, s, x.trait_type_arg(e));
            } else {
                s.uv(0);
                stack.push(P::Opt(id(n.a())));
            }
        }
        K::TypeTraitExpr => {
            tag(s, k);
            aux_tag(
                s,
                TypeTrait::from_u8(aux).map(|t| type_trait::row(&t)),
                "profile/TypeTraitExpr.aux",
            );
            let ts: Vec<QualType> = ast.lists.iter::<QualType>(x.type_trait_args(e)).collect();
            w.write_types(s, &ts);
        }
        K::ArrayTypeTraitExpr => {
            tag(s, k);
            aux_tag(
                s,
                TypeTrait::from_u8(aux).map(|t| type_trait::row(&t)),
                "profile/ArrayTypeTraitExpr.aux",
            );
            w.write_type(s, x.array_trait_type(e));
            stack.push(P::Opt(x.array_trait(e).dim));
        }
        K::ExpressionTraitExpr => {
            tag(s, k);
            aux_tag(
                s,
                TypeTrait::from_u8(aux).map(|t| type_trait::row(&t)),
                "profile/ExpressionTraitExpr.aux",
            );
            stack.push(P::Expr(x.expression_trait_operand(e)));
        }
        K::RequiresExpr => match ast.requires_of(e).copied() {
            Some(r) => {
                tag(s, k);
                // The parameters by type only; a reference to one is its
                // position (`decl_words`).
                let params: Vec<DeclId> = ast.lists.iter::<DeclId>(r.params).collect();
                s.count(params.len());
                for p in params {
                    type_opt(w, s, ast.decls.get(p).ty());
                }
                let reqs = ast.templates.requirements(&r).to_vec();
                s.count(reqs.len());
                for req in reqs {
                    match req {
                        Requirement::Simple(x) => stack.extend([P::Uv(R_SIMPLE), P::Expr(x)]),
                        Requirement::Type(t, _position) => {
                            stack.extend([P::Uv(R_TYPE), P::Type(t)]);
                        }
                        Requirement::Compound { expr, noexcept, constraint } => stack.extend([
                            P::Uv(R_COMPOUND),
                            P::Uv(u64::from(noexcept.is_some())),
                            P::Expr(expr),
                            P::Opt(constraint),
                        ]),
                        Requirement::Nested(c) => stack.extend([P::Uv(R_NESTED), P::Expr(c)]),
                    }
                }
            }
            None => stack.push(P::Full(e)),
        },
        K::ConceptSpecializationExpr => {
            tag(s, k);
            match DeclId::try_new(n.a()) {
                Some(c) if ast.decls.get(c).kind() == DeclKind::TemplateTemplateParm => {
                    s.uv(1);
                    decl_words(w, s, c);
                }
                Some(c) => {
                    s.uv(2);
                    w.write_ref(s, ast.first(c));
                }
                None => {
                    s.uv(0);
                }
            }
            match ArgListId::try_new(n.b()) {
                Some(l) => {
                    s.opt(true);
                    w.write_args(s, l);
                }
                None => {
                    s.opt(false);
                }
            }
        }

        // ----- no equivalence rule: in full, a function of the spelling;
        // and a reflection or a splice, whose profile needs facts the
        // digest does not yet write in profile form (their package's)
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
        | K::CXXExpansionSelectExpr => stack.push(P::Full(e)),
    }
    stack[mark..].reverse();
}

/// A transparent node: its operand, or the placeholder of an absent one.
fn transparent(s: &mut Stream, a: Option<ExprId>, stack: &mut Vec<P>) {
    match a {
        Some(a) => stack.push(P::Expr(a)),
        None => {
            s.uv(0).indeterminate(Cause::Unaccounted("profile/transparent.a"));
        }
    }
}

/// A literal's constant value.
fn literal_value(w: &mut Walk<'_>, s: &mut Stream, v: Option<slip_kiln::kiln::ast::ConstValueId>) {
    match v {
        Some(v) => crate::value::value(w, s, v),
        None => {
            s.uv(0).indeterminate(Cause::Unaccounted("profile/literal"));
        }
    }
}

#[cfg(test)]
#[path = "expr_profile_tests.rs"]
mod tests;
