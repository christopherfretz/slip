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
use slip_kiln::kiln::ast::expr::{
    ExprDeps, ExprFlags, ExprKind as K, GenericControl, TypeidOperand,
};
use slip_kiln::kiln::ast::templates::{Requirement, SpecKind};
use slip_kiln::kiln::ast::types::{
    ArgView, ExcSpec, Nns, TemplateArg, TemplateName, TypeOfOperand, TypeView,
};
use slip_kiln::kiln::ast::{
    ArgListId, Ast, DeclId, ExprId, NnsId, QualType, TemplateParamListId, TypeId,
};
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
/// The entry of a holder Kiln does not keep ([`Op::Unkept`]): no site,
/// indeterminate.
pub(crate) const UNKEPT: u8 = 0;

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

/// A declaration whose type a constrained placeholder deduced
/// (`Same<decltype(make(x))> auto v = 1;`, `C<…> auto f() {…}`): Kiln at
/// the pin replaces the declared type by the deduced one and keeps the
/// type-constraint nowhere in the AST (`kiln_sema` `declared_returns`
/// holds a function's, for itself), so its operands cannot be read
/// (`docs/requests/KT1-written-placeholder-and-qualifier.md`).
pub(crate) const PLACEHOLDER: Cause = Cause::Unsupported("kiln/placeholder-constraint");

/// An explicit instantiation of a function, a variable or a member
/// (`template int f<sizeof(make(x))>();`): Kiln keeps neither its
/// template arguments nor its declarator as written.
pub(crate) const EXPLICIT_INSTANTIATION: Cause =
    Cause::Unsupported("kiln/explicit-instantiation-written");

/// A declaration whose declarator's qualifier (or a `using enum`'s) may
/// name a class through a template-id with an operand
/// (`template <> int Q<sizeof(make(x))>::s = 5;`): Kiln keeps no
/// qualifier as written for a declarator.
pub(crate) const QUALIFIER: Cause = Cause::Unsupported("kiln/declarator-qualifier");

/// A splice (`[: e :]`, P2996) in a written type or nested-name-specifier:
/// its operand is not read (the expression walk makes a splice
/// unsupported too).
pub(crate) const SPLICE: Cause = Cause::Unsupported("operands/splice");

/// One operand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Op {
    /// Sites 1, 2, 4, 6: the expression.
    Expr(u8, ExprId),
    /// Site 8: the argument list as written of an alias specialisation.
    Alias(ArgListId),
    /// A holder whose operands Kiln does not keep (fails closed): the
    /// entry is indeterminate for the cause.
    Unkept(Cause),
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
    /// Entries of a holder Kiln does not keep ([`Op::Unkept`]).
    pub unkept: u64,
}

/// What the operand units read of the whole translation unit, once
/// ([`tu`]): Kiln keeps neither a constrained placeholder's
/// type-constraint once it deduced the type nor a declarator's qualifier
/// as written, but the types they wrote are in the type arena.
#[derive(Debug, Default)]
pub(crate) struct Tu {
    /// The arena holds a constrained placeholder whose written
    /// arguments hold an operand ([`holds`]), not instantiation-dependent.
    placeholder: bool,
    /// The classes (first declarations) that a type in the arena names by
    /// a template-id whose written arguments hold an operand, or by a
    /// `decltype` or `typeof` of an expression, not instantiation-dependent:
    /// what a declarator's qualifier with an operand can name.
    named: HashSet<DeclId>,
    /// The canonical types of the types in the arena that hold an operand
    /// ([`holds`]), not instantiation-dependent: what an explicit
    /// instantiation's written argument or declarator may have spelt.
    held: HashSet<TypeId>,
    /// The class templates (first declarations) an alias template's
    /// target names, `template <class T> using A = Q<T>;`: a declarator's
    /// qualifier `A<sizeof(f(x))>::` leaves no type in the arena (Kiln
    /// substitutes the alias at once), so a specialisation of `Q` can be
    /// named by one. A class an alias template names outright
    /// (`using A = P;`) is in [`Tu::named`].
    aliased: HashSet<DeclId>,
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
                // A guard: a stream walked and dropped
                // (`demand::immediates`, a tied candidate in
                // `units::read_decl`) starts from a fresh unit state
                // (`Walk::take_unit`), whose operand state is off, so no
                // fixture reaches this test with `dropped` above 0 and
                // the state on. Removing it (mutation R11, WP-156 fix
                // round 1) changes no unit of the digest suites
                // (`differ/_/operands-dropped-walk` is the shape).
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
/// its pattern's; an enumeration's fixed underlying type; a
/// using-declaration's qualifier; a template head ([`head`]); a class
/// definition's base specifiers; a friend's type; the argument list as
/// written of a specialisation. A variable whose type a placeholder
/// deduced has no written type (S3). Where Kiln does not keep what is
/// written, an [`Op::Unkept`] entry: a constrained placeholder
/// ([`PLACEHOLDER`]), an explicit instantiation of a function, variable
/// or member ([`EXPLICIT_INSTANTIATION`]), a declarator's qualifier
/// ([`QUALIFIER`]).
#[allow(
    clippy::too_many_lines,
    reason = "one arm per declaration kind, exhaustive (WP-156 fix round 1)"
)]
#[allow(
    clippy::match_same_arms,
    reason = "the kinds that hold no written type are listed apart, by reason"
)]
fn signature(w: &mut Walk<'_>, d: DeclId, params: bool, out: &mut Vec<Op>) {
    let ast = w.ast;
    let n = ast.decls.get(d);
    if qualifier_unkept(w, d) {
        out.push(Op::Unkept(QUALIFIER));
    }
    match n.kind() {
        DeclKind::Function
        | DeclKind::CXXMethod
        | DeclKind::CXXConstructor
        | DeclKind::CXXDestructor
        | DeclKind::CXXConversion
        | DeclKind::CXXDeductionGuide => {
            if explicit_instantiation_unkept(w, d) {
                out.push(Op::Unkept(EXPLICIT_INSTANTIATION));
            }
            if ast.decls.function(d).is_some_and(|f| f.spec.return_deduced()) && tu(w).placeholder {
                out.push(Op::Unkept(PLACEHOLDER));
            }
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
        DeclKind::Var
        | DeclKind::VarTemplateSpecialization
        | DeclKind::VarTemplatePartialSpecialization => {
            if explicit_instantiation_unkept(w, d) {
                out.push(Op::Unkept(EXPLICIT_INSTANTIATION));
            }
            if ast.decls.var(d).is_some_and(|v| v.spec.placeholder() != VarPlaceholder::None) {
                if counting(w) {
                    w.op_counts.deduced += 1;
                }
                if tu(w).placeholder {
                    out.push(Op::Unkept(PLACEHOLDER));
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
            if let FriendTarget::Type { ty, owned_tag: _ } =
                slip_kiln::written::friend_target(ast, d)
            {
                scan(w, ty, out, 0);
            }
        }
        // The fixed underlying type of an enumeration whose definition is
        // its resolved unit `definition` is that unit's ([`underlying`]).
        DeclKind::Enum => {
            if !crate::demand::definition_class(w, d) {
                underlying_of(w, d, out);
            }
        }
        // The bases of a class whose definition is its resolved unit
        // `definition` (digest-kinds §11.9) are that unit's ([`bases`]).
        DeclKind::CXXRecord
        | DeclKind::ClassTemplateSpecialization
        | DeclKind::ClassTemplatePartialSpecialization => {
            if ast.templates.args_as_written(d).is_none() && explicit_instantiation_unkept(w, d) {
                // A member class's (`template struct Q<…>::In;`).
                out.push(Op::Unkept(EXPLICIT_INSTANTIATION));
            }
            if !crate::demand::definition_class(w, d) {
                bases_of(w, d, out);
            }
        }
        DeclKind::Using | DeclKind::UnresolvedUsingValue | DeclKind::UnresolvedUsingTypename => {
            if let Some(q) = ast.using_qualifier(d) {
                scan_nns(w, q, out, 0);
            }
        }
        // `using enum Q<sizeof(make(x))>::E;`: Kiln keeps the enumeration
        // only.
        DeclKind::UsingEnum => {
            if let Some(e) = ast.using_target(d)
                && in_named_class(w, ast.decls.get(e).ctx(), None)
            {
                out.push(Op::Unkept(QUALIFIER));
            }
        }
        // A template's head and pattern are read below; a parameter's by
        // its template's head ([`head`]).
        DeclKind::ClassTemplate
        | DeclKind::FunctionTemplate
        | DeclKind::VarTemplate
        | DeclKind::TypeAliasTemplate
        | DeclKind::Concept
        | DeclKind::TemplateTypeParm
        | DeclKind::NonTypeTemplateParm
        | DeclKind::TemplateTemplateParm => {}
        // No written type: a namespace's or a scope's, a name of a
        // namespace (whose qualifier names namespaces only), what a
        // declaration introduces implicitly, a structured binding's
        // (its variable's placeholder), an enumerator, an indirect field,
        // a label, a static assertion's or a consteval block's (their
        // expressions are the stream's).
        DeclKind::TranslationUnit
        | DeclKind::Namespace
        | DeclKind::NamespaceAlias
        | DeclKind::LinkageSpec
        | DeclKind::Export
        | DeclKind::Empty
        | DeclKind::FileScopeAsm
        | DeclKind::UsingDirective
        | DeclKind::UsingShadow
        | DeclKind::ConstructorUsingShadow
        | DeclKind::UsingPack
        | DeclKind::UnresolvedUsingIfExists
        | DeclKind::EnumConstant
        | DeclKind::IndirectField
        | DeclKind::AccessSpec
        | DeclKind::FriendTemplate
        | DeclKind::ImplicitParam
        | DeclKind::Binding
        | DeclKind::BuiltinTemplate
        | DeclKind::ImplicitConceptSpecialization
        | DeclKind::RequiresExprBody
        | DeclKind::LifetimeExtendedTemporary
        | DeclKind::TemplateParamObject
        | DeclKind::StaticAssert
        | DeclKind::Label
        | DeclKind::ContractResultName
        | DeclKind::ConstevalBlock => {}
    }
    if let Some(i) = ast.template_info(d).copied() {
        head(w, i.params, out, 0);
        if params && let Some(p) = i.pattern {
            signature(w, p, true, out);
        }
    }
    if let Some(l) = ast.templates.args_as_written(d) {
        args(w, l, out, 0);
    }
}

/// The operands of template parameter list `l` (a template's head, or a
/// template template parameter's): each non-type parameter's type, each
/// default type argument, and a template template parameter's own list.
/// A default non-type argument is walked by the unit's own stream
/// (`templ::head`).
fn head(w: &mut Walk<'_>, l: TemplateParamListId, out: &mut Vec<Op>, depth: u32) {
    if depth > MAX_DEPTH {
        return;
    }
    let ast = w.ast;
    let list = ast.templates.param_list(l);
    let ps: Vec<DeclId> = ast.lists.iter::<DeclId>(list.params).collect();
    for p in ps {
        match ast.decls.get(p).kind() {
            DeclKind::NonTypeTemplateParm => {
                if let Some(q) = ast.decls.get(p).ty() {
                    scan(w, q, out, 0);
                }
            }
            DeclKind::TemplateTemplateParm => {
                if let Some(inner) = ast.template_parm(p).and_then(|i| i.params) {
                    head(w, inner, out, depth + 1);
                }
            }
            _ => {}
        }
        if let Some(a) = ast.templates.defaults.get(p).copied()
            && let ArgView::Type(t) = a.view()
        {
            scan(w, t, out, 0);
        }
    }
}

/// Whether `d` is a function, variable or member that an explicit
/// instantiation of its own names (not one its class's covers).
fn own_explicit_instantiation(w: &Walk<'_>, d: DeclId) -> bool {
    let ast = w.ast;
    matches!(
        ast.governing_explicit_inst(d),
        Some((x, SpecKind::ExplicitInstantiationDeclaration | SpecKind::ExplicitInstantiationDefinition))
            if x == ast.first(d)
    )
}

/// Whether `d` is named by an explicit instantiation of its own whose
/// written parts may hold an operand Kiln does not keep: its qualifier
/// names a class of [`Tu::named`]; it is a template's specialisation with
/// a non-type argument (whose expression is not kept) or a type argument
/// of [`Tu::held`]; its type, a parameter's or its return type is of
/// [`Tu::held`] (its declarator's spelling).
fn explicit_instantiation_unkept(w: &mut Walk<'_>, d: DeclId) -> bool {
    let ast = w.ast;
    if !own_explicit_instantiation(w, d) {
        return false;
    }
    let ctx = ast.decls.get(d).ctx();
    if in_named_class(w, ctx, None) {
        return true;
    }
    let types = &ast.types;
    let mut written: Vec<QualType> = Vec::new();
    if let Some(q) = ast.decls.get(d).ty() {
        // A function's own type is not one the directive spells as a
        // whole: its result and parameter types are. Its exception
        // specification is the specialisation's, substituted from the
        // pattern (`noexcept(sizeof(T) > 1)` holds an operand once `T` is
        // `int`), and its operands belong to the `exception_spec` unit
        // (`spec/digest.md` §2.4), so the function type is not tested.
        if let TypeView::Function(p) = types.canonical_view(q) {
            written.push(p.ret());
            written.extend_from_slice(types.params(&p));
        } else {
            written.push(q);
        }
    }
    let lists: Vec<ArgListId> = ast.spec_info(d).map(|(_, i)| i.args).into_iter().collect();
    args_unkept(w, lists, written)
}

/// Whether a template-id whose converted arguments are `lists` (Kiln
/// keeps them without what was written), or a declarator whose types are
/// `written`, may have spelt an operand: a non-type argument (its value
/// alone is kept), or a type that is (canonically) a type of the arena
/// that holds an operand ([`Tu::held`]).
fn args_unkept(w: &mut Walk<'_>, mut lists: Vec<ArgListId>, mut written: Vec<QualType>) -> bool {
    let ast = w.ast;
    let types = &ast.types;
    let mut value = false;
    let mut n = 0;
    while let Some(l) = lists.pop() {
        n += 1;
        if n > MAX_DEPTH {
            return true;
        }
        for a in types.args(l) {
            match a.view() {
                ArgView::Type(t) => written.push(t),
                ArgView::Pack(p) => lists.push(p),
                ArgView::Value { .. } | ArgView::Expr { .. } | ArgView::Profile(_) => value = true,
                ArgView::Null | ArgView::Template(_) | ArgView::TemplateExpansion { .. } => {}
            }
        }
    }
    if value {
        return true;
    }
    let held = &tu(w).held;
    written.iter().any(|&q| held.contains(&types.canonical(q).ty()))
}

/// Whether declaration `d` is written outside its class, inside a class
/// that a template-id or `decltype` with an operand names somewhere in
/// the translation unit ([`Tu::named`]): its declarator's qualifier may
/// hold that operand, which Kiln does not keep. An instantiation's
/// declaration is not written.
fn qualifier_unkept(w: &mut Walk<'_>, d: DeclId) -> bool {
    let ast = w.ast;
    let n = ast.decls.get(d);
    let Some(lexical) = ast.decls.lexical_ctx(d) else { return false };
    if n.ctx() == Some(lexical) || w.is_local(d) {
        return false;
    }
    if matches!(
        crate::demand::effective_kind(w, d),
        Some(
            SpecKind::ImplicitInstantiation
                | SpecKind::ExplicitInstantiationDeclaration
                | SpecKind::ExplicitInstantiationDefinition
        )
    ) {
        return false;
    }
    in_named_class(w, n.ctx(), Some(lexical))
}

/// Whether context `c`, or a class around it up to `stop`, is a class of
/// [`Tu::named`], a specialisation of a class template of
/// [`Tu::aliased`], or a class template specialisation whose arguments
/// may have been spelt with an operand ([`args_unkept`]): a qualifier
/// that names a specialisation is a template-id, and Kiln leaves no type
/// for one written in a declarator (`template <> int Q<sizeof(f(x))>::s`).
fn in_named_class(w: &mut Walk<'_>, mut c: Option<DeclId>, stop: Option<DeclId>) -> bool {
    let ast = w.ast;
    let mut classes = Vec::new();
    for _ in 0..MAX_DEPTH {
        let Some(x) = c else { break };
        if Some(x) == stop || ast.decls.record(x).is_none() {
            break;
        }
        classes.push(ast.first(x));
        c = ast.decls.get(x).ctx();
    }
    if classes.is_empty() {
        return false;
    }
    for x in classes {
        let tu = tu(w);
        let spec = ast.spec_info(x).map(|(_, i)| (ast.first(i.template), i.args));
        if tu.named.contains(&x) || spec.is_some_and(|(t, _)| tu.aliased.contains(&t)) {
            return true;
        }
        if let Some((_, l)) = spec
            && args_unkept(w, vec![l], Vec::new())
        {
            return true;
        }
    }
    false
}

/// The translation unit's facts ([`Tu`]), computed on first use.
fn tu<'w>(w: &'w mut Walk<'_>) -> &'w Tu {
    let ast = w.ast;
    w.op_tu.get_or_insert_with(|| tu_of(ast))
}

/// [`Tu`] of `ast`: one pass over the type arena.
fn tu_of(ast: &Ast) -> Tu {
    let types = &ast.types;
    let mut memo = HashMap::new();
    let mut out = Tu::default();
    for i in 1..types.len() {
        let Ok(i) = u32::try_from(i) else { break };
        let t = TypeId::new(i);
        if types.flags(t).is_instantiation_dependent() {
            continue;
        }
        if holds(ast, QualType::unqual(t), &mut memo, 0) {
            out.held.insert(types.canonical(QualType::unqual(t)).ty());
        }
        match types.view(t) {
            TypeView::Auto { args: Some(l), .. } => {
                if !out.placeholder && args_hold(ast, l, &mut memo, 0) {
                    out.placeholder = true;
                }
            }
            TypeView::TemplateSpecialization { args: l, .. } => {
                if args_hold(ast, l, &mut memo, 0) {
                    named(ast, t, &mut out);
                }
            }
            TypeView::Decltype { expr, .. }
            | TypeView::TypeOf { operand: TypeOfOperand::Expr(expr), .. } => {
                if !literal(ast, expr) {
                    named(ast, t, &mut out);
                }
            }
            _ => {}
        }
    }
    // An alias template's target (the template-id that names it, with its
    // arguments, is substituted away when it is a declarator's qualifier).
    for i in 1..=ast.decls.len() {
        let Ok(i) = u32::try_from(i) else { break };
        let d = DeclId::new(i);
        if ast.decls.get(d).kind() != DeclKind::TypeAliasTemplate {
            continue;
        }
        let Some(target) = ast
            .template_info(d)
            .and_then(|i| i.pattern)
            .and_then(|p| ast.decls.get(p).ty())
            .and_then(|q| match types.view(q.ty()) {
                TypeView::Typedef { underlying, .. } => Some(underlying),
                _ => None,
            })
        else {
            continue;
        };
        match types.canonical_view(target) {
            TypeView::Record(c) => {
                out.named.insert(ast.first(c));
            }
            TypeView::TemplateSpecialization { name, .. } => {
                if let TemplateName::Template(t) =
                    types.template_name_view(types.canonical_template_name(name))
                {
                    out.aliased.insert(ast.first(t));
                }
            }
            // Another target names no class, or one a dependent name
            // finds (`typename T::type`): a limit (`spec/digest.md` §2.4).
            _ => {}
        }
    }
    out
}

/// Adds to [`Tu::named`] the class type `t` names, if it names one.
fn named(ast: &Ast, t: TypeId, out: &mut Tu) {
    if let TypeView::Record(c) = ast.types.canonical_view(QualType::unqual(t)) {
        out.named.insert(ast.first(c));
    }
}

/// Whether the written argument list `l` holds an operand [`scan`] would
/// collect, or a constant expression that is no literal (a superset:
/// what it decides fails closed).
fn args_hold(ast: &Ast, l: ArgListId, memo: &mut HashMap<TypeId, bool>, depth: u32) -> bool {
    if depth > MAX_DEPTH {
        return true;
    }
    ast.types.args(l).iter().any(|a| match a.view() {
        ArgView::Type(t) => holds(ast, t, memo, depth + 1),
        ArgView::Expr { expr, .. } => {
            !ast.exprs.get(expr).deps().contains(ExprDeps::INSTANTIATION) && !literal(ast, expr)
        }
        ArgView::Pack(p) => args_hold(ast, p, memo, depth + 1),
        ArgView::Null
        | ArgView::Value { .. }
        | ArgView::Template(_)
        | ArgView::TemplateExpansion { .. }
        | ArgView::Profile(_) => false,
    })
}

/// Whether `e` is a literal (through parentheses, implicit conversions, a
/// constant expression's wrapper, a unary operator, a substituted
/// parameter): it writes no reference.
fn literal(ast: &Ast, mut e: ExprId) -> bool {
    for _ in 0..MAX_DEPTH {
        match ast.exprs.get(e).kind() {
            K::IntegerLiteral
            | K::FloatingLiteral
            | K::CharacterLiteral
            | K::StringLiteral
            | K::CXXBoolLiteralExpr
            | K::CXXNullPtrLiteralExpr
            | K::GNUNullExpr
            | K::SubstNonTypeTemplateParmExpr => return true,
            K::ImplicitCastExpr | K::ParenExpr | K::ConstantExpr | K::UnaryOperator => {
                e = ast.exprs.operand(e);
            }
            // `sizeof(0)`, `sizeof(int)`, as `std::size_t` is declared.
            K::UnaryExprOrTypeTraitExpr => match ast.exprs.trait_type_arg(e) {
                Some(t) => return matches!(ast.types.view(t.ty()), TypeView::Builtin(_)),
                None => match ExprId::try_new(ast.exprs.get(e).a()) {
                    Some(x) => e = x,
                    None => return false,
                },
            },
            _ => return false,
        }
    }
    false
}

/// Whether the written type `q` holds an operand [`scan`] would collect
/// (a superset, memoised by type).
fn holds(ast: &Ast, q: QualType, memo: &mut HashMap<TypeId, bool>, depth: u32) -> bool {
    if depth > MAX_DEPTH {
        return true;
    }
    let t = q.ty();
    if let Some(&h) = memo.get(&t) {
        return h;
    }
    let d = depth + 1;
    let types = &ast.types;
    let not_dep = |e: ExprId| !ast.exprs.get(e).deps().contains(ExprDeps::INSTANTIATION);
    let h = match types.view(t) {
        TypeView::Decltype { expr, .. }
        | TypeView::TypeOf { operand: TypeOfOperand::Expr(expr), .. } => {
            not_dep(expr) && !literal(ast, expr)
        }
        TypeView::TemplateSpecialization { args: l, alias, .. } => {
            (alias && !types.flags(t).is_instantiation_dependent()) || args_hold(ast, l, memo, d)
        }
        TypeView::Auto { args: Some(l), .. }
        | TypeView::DependentTemplateSpecialization { args: l, .. } => args_hold(ast, l, memo, d),
        TypeView::Function(p) => {
            holds(ast, p.ret(), memo, d)
                || types.params(&p).iter().any(|&x| holds(ast, x, memo, d))
                || matches!(types.exc_spec(&p), ExcSpec::NoexceptExpr { expr, .. } if not_dep(expr))
        }
        TypeView::Pointer(x)
        | TypeView::LValueRef(x)
        | TypeView::RValueRef(x)
        | TypeView::IncompleteArray(x)
        | TypeView::Paren(x)
        | TypeView::Complex(x)
        | TypeView::Atomic(x)
        | TypeView::Vector { elem: x, .. }
        | TypeView::DependentVector { elem: x, .. }
        | TypeView::DependentSizedArray { elem: x, .. }
        | TypeView::VariableArray { elem: x, .. }
        | TypeView::DependentAddrSpace { pointee: x, .. }
        | TypeView::ExtQual { base: x, .. }
        | TypeView::Attributed { modified: x, .. }
        | TypeView::Decayed { original: x, .. }
        | TypeView::PackExpansion { pattern: x, .. }
        | TypeView::PackIndexing { pattern: x, .. }
        | TypeView::UnaryTransform { base: x, .. }
        | TypeView::TypeOf { operand: TypeOfOperand::Type(x), .. } => holds(ast, x, memo, d),
        TypeView::ConstantArray(a) => holds(ast, a.elem, memo, d),
        TypeView::QualifiedArray { base, .. } => holds(ast, QualType::unqual(base), memo, d),
        TypeView::MemberPointer { pointee, class } => {
            holds(ast, pointee, memo, d) || holds(ast, QualType::unqual(class), memo, d)
        }
        TypeView::Elaborated { nns, named, .. } => {
            nns.is_some_and(|n| nns_holds(ast, n, memo, d)) || holds(ast, named, memo, d)
        }
        TypeView::DependentName { nns, .. } => nns_holds(ast, nns, memo, d),
        TypeView::Splice { .. } => true,
        TypeView::Invalid
        | TypeView::Builtin(_)
        | TypeView::BitInt { .. }
        | TypeView::DependentBitInt { .. }
        | TypeView::Record(_)
        | TypeView::Enum(_)
        | TypeView::UnresolvedUsing(_)
        | TypeView::Typedef { .. }
        | TypeView::Using { .. }
        | TypeView::TemplateTypeParm(_)
        | TypeView::CanonTemplateTypeParm { .. }
        | TypeView::SubstTemplateTypeParm { .. }
        | TypeView::SubstTemplateTypeParmPack { .. }
        | TypeView::InjectedClassName { .. }
        | TypeView::Auto { args: None, .. }
        | TypeView::DependentDecltype(_)
        | TypeView::TypeOf { operand: TypeOfOperand::Profile(_), .. }
        | TypeView::DeducedTemplateSpecialization { .. } => false,
    };
    memo.insert(t, h);
    h
}

/// [`holds`] for nested-name-specifier `n`.
fn nns_holds(ast: &Ast, n: NnsId, memo: &mut HashMap<TypeId, bool>, depth: u32) -> bool {
    if depth > MAX_DEPTH {
        return true;
    }
    match ast.types.nns_view(n) {
        Nns::Type { prefix, ty, .. } => {
            prefix.is_some_and(|p| nns_holds(ast, p, memo, depth + 1))
                || holds(ast, ty, memo, depth + 1)
        }
        Nns::Namespace { prefix, .. }
        | Nns::NamespaceAlias { prefix, .. }
        | Nns::Identifier { prefix, .. } => {
            prefix.is_some_and(|p| nns_holds(ast, p, memo, depth + 1))
        }
        Nns::Splice(_) => true,
        Nns::Global => false,
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

/// An enumeration declaration's fixed underlying type ([dcl.enum]/5).
fn underlying_of(w: &mut Walk<'_>, d: DeclId, out: &mut Vec<Op>) {
    if let Some(e) = w.ast.decls.enumeration(d)
        && e.is_fixed()
        && let Some(q) = e.int_type
    {
        scan(w, q, out, 0);
    }
}

/// The operands of the fixed underlying type of `def`, the definition of
/// an enumeration whose resolved unit `definition` holds it
/// (`decl_class::enum_definition`).
pub(crate) fn underlying(w: &mut Walk<'_>, def: DeclId) {
    if !collecting(w) {
        return;
    }
    let mut out = Vec::new();
    underlying_of(w, def, &mut out);
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
        Op::Alias(_) | Op::Unkept(_) => false,
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
#[allow(clippy::match_same_arms, reason = "one arm per TypeView variant, each with its reason")]
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
        // The bound or size (site 3) is not read: Kiln keeps it as a value
        // only at the pin this was built for (B1; stage e), and a
        // dependent one is a pattern's. The element type is.
        TypeView::ConstantArray(a) => scan(w, a.elem, out, d),
        TypeView::VariableArray { elem: x, .. }
        | TypeView::DependentSizedArray { elem: x, .. }
        | TypeView::DependentVector { elem: x, .. }
        | TypeView::DependentAddrSpace { pointee: x, .. } => scan(w, x, out, d),
        // The pack index (site 5) is not read (B7); the pattern is.
        TypeView::PackIndexing { pattern, .. } => scan(w, pattern, out, d),
        TypeView::QualifiedArray { base, .. } => scan(w, QualType::unqual(base), out, d),
        TypeView::MemberPointer { pointee, class } => {
            scan(w, pointee, out, d);
            scan(w, QualType::unqual(class), out, d);
        }
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
        // A dependent name's qualifier and arguments: an operand in them
        // that is not instantiation-dependent is no pattern's spelling
        // (`typename Q<sizeof(make(x))>::template In<T>::t`).
        TypeView::DependentName { nns, .. } => scan_nns(w, nns, out, d),
        TypeView::DependentTemplateSpecialization { nns, args: l, .. } => {
            scan_nns(w, nns, out, d);
            args(w, l, out, d);
        }
        TypeView::TemplateSpecialization { args: l, alias, .. } => {
            if alias {
                // B5: an alias specialisation's written arguments, then
                // the operands inside them. Instantiation-dependent: the
                // list is the pattern's own spelling, but an operand in it
                // that is not is read ([`push`] drops the others).
                if types.flags(q.ty()).is_instantiation_dependent() {
                    if counting(w) {
                        w.op_counts.dependent += 1;
                    }
                } else {
                    out.push(Op::Alias(l));
                }
            }
            args(w, l, out, d);
        }
        // A placeholder's type-constraint as written (a template
        // parameter's or an undeduced one's; a deduced declaration's is
        // not kept, `PLACEHOLDER`).
        TypeView::Auto { args: Some(l), .. } => args(w, l, out, d),
        TypeView::Decltype { expr, .. } => push(w, Op::Expr(DECLTYPE, expr), out),
        TypeView::TypeOf { operand: TypeOfOperand::Expr(e), .. } => {
            push(w, Op::Expr(TYPEOF, e), out);
        }
        TypeView::TypeOf { operand: TypeOfOperand::Type(t), .. } => scan(w, t, out, d),
        TypeView::Splice { .. } => out.push(Op::Unkept(SPLICE)),
        // What a name denotes (a typedef's, alias's or using-declaration's
        // type, a class, an enumeration, an injected class name), a
        // substituted parameter's replacement, a parameter, a placeholder
        // with no written argument or what one deduced, a dependent
        // `decltype`, `typeof` or bit width (a pattern's canonical form), a
        // builtin: nothing is read.
        TypeView::Invalid
        | TypeView::Builtin(_)
        | TypeView::BitInt { .. }
        | TypeView::DependentBitInt { .. }
        | TypeView::Record(_)
        | TypeView::Enum(_)
        | TypeView::UnresolvedUsing(_)
        | TypeView::Typedef { .. }
        | TypeView::Using { .. }
        | TypeView::TemplateTypeParm(_)
        | TypeView::CanonTemplateTypeParm { .. }
        | TypeView::SubstTemplateTypeParm { .. }
        | TypeView::SubstTemplateTypeParmPack { .. }
        | TypeView::InjectedClassName { .. }
        | TypeView::Auto { args: None, .. }
        | TypeView::DependentDecltype(_)
        | TypeView::TypeOf { operand: TypeOfOperand::Profile(_), .. }
        | TypeView::DeducedTemplateSpecialization { .. } => {}
    }
}

/// The operands written in template argument list `l` as written: an
/// expression argument is site 4, a type argument is scanned.
fn args(w: &mut Walk<'_>, l: ArgListId, out: &mut Vec<Op>, depth: u32) {
    if depth > MAX_DEPTH {
        return;
    }
    let views: Vec<ArgView> = w.ast.types.args(l).iter().map(TemplateArg::view).collect();
    for v in views {
        match v {
            ArgView::Type(t) => scan(w, t, out, depth + 1),
            ArgView::Expr { expr, .. } => push(w, Op::Expr(EXPR_ARG, expr), out),
            // R21: a written list holds no pack (Kiln groups a pack's
            // arguments in a converted list only); kept for a list that
            // would. Unreachable at the pin: emptying this arm changes
            // no unit of `differ/_/operands-scan-arms`, whose `vp`
            // (`VT<int, Q<…>>`, an alias pack) and `tt` (`Tup<Ts...>`
            // substituted) are the shapes that could make one.
            ArgView::Pack(p) => args(w, p, out, depth + 1),
            // A converted value, a template's name, a dependent
            // expression's canonical form: nothing written.
            ArgView::Null
            | ArgView::Value { .. }
            | ArgView::Template(_)
            | ArgView::TemplateExpansion { .. }
            | ArgView::Profile(_) => {}
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
        Nns::Namespace { prefix, .. }
        | Nns::NamespaceAlias { prefix, .. }
        | Nns::Identifier { prefix, .. } => {
            if let Some(p) = prefix {
                scan_nns(w, p, out, depth + 1);
            }
        }
        Nns::Splice(_) => out.push(Op::Unkept(SPLICE)),
        Nns::Global => {}
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
#[allow(
    clippy::too_many_lines,
    reason = "one arm per expression kind, exhaustive (WP-156 fix round 1)"
)]
#[allow(
    clippy::many_single_char_names,
    reason = "e expression, n node, k kind, x arena, as own_spelling"
)]
pub(crate) fn expr_node(w: &mut Walk<'_>, e: ExprId) {
    if !collecting(w) {
        return;
    }
    let ast = w.ast;
    let x = &ast.exprs;
    let n = *x.get(e);
    let mut out = Vec::new();
    // The written qualifier and arguments Kiln keeps beside any
    // expression (`TemplateTables` `expr_nns`, `expr_targs`: an unresolved
    // or dependent name's, a concept-id's), each once.
    let mut lists: Vec<ArgListId> = Vec::new();
    let mut quals: Vec<NnsId> = Vec::new();
    if let Some(q) = ast.templates.expr_nns.get(e).copied() {
        quals.push(q);
    }
    if let Some(l) = ast.templates.expr_targs.get(e).copied() {
        lists.push(l);
    }
    match n.kind() {
        K::DeclRefExpr | K::MemberExpr => {
            let written = [
                x.decl_ref_extra(e).map(|r| (r.qualifier, r.template_args)),
                x.cold.member_overload.get(e).map(|m| (m.qualifier, m.template_args)),
            ];
            for (q, l) in written.into_iter().flatten() {
                if let Some(q) = q
                    && !quals.contains(&q)
                {
                    quals.push(q);
                }
                if let Some(l) = l
                    && !lists.contains(&l)
                {
                    lists.push(l);
                }
            }
        }
        K::ConceptSpecializationExpr => {
            if let Some(l) = ArgListId::try_new(n.b())
                && !lists.contains(&l)
            {
                lists.push(l);
            }
        }
        // A dependent qualified name keeps its qualifier on the node, not
        // in `expr_nns` (`roles.rs`: the one expression whose field is
        // an `Nns`): `Q<sizeof(f(x))>::template In<T>::v` in a pattern.
        K::DependentScopeDeclRefExpr => {
            if let Some(q) = NnsId::try_new(n.a())
                && !quals.contains(&q)
            {
                quals.push(q);
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
            let written = x.cast_written_type(e).unwrap_or(n.ty());
            scan(w, written, &mut out, 0);
        }
        // `T(...)`, `T{...}` and `T()`: an explicit type conversion in
        // functional notation, whose node type is the type as written;
        // `__builtin_convertvector(v, T)`, whose node type is `T`.
        K::CXXTemporaryObjectExpr | K::CXXScalarValueInitExpr | K::ConvertVectorExpr => {
            scan(w, n.ty(), &mut out, 0);
        }
        K::CXXUnresolvedConstructExpr => scan(w, x.unresolved_construct_type(e), &mut out, 0),
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
        K::CXXNewExpr => {
            // A `new` of a constrained placeholder (`new C<…> auto(1)`)
            // has the deduced type: Kiln keeps no mark of it.
            if tu(w).placeholder {
                out.push(Op::Unkept(PLACEHOLDER));
            }
            scan(w, x.new_extra(e).allocated_ty, &mut out, 0);
        }
        K::CompoundLiteralExpr => scan(w, x.compound_literal_written_type(e), &mut out, 0),
        K::CXXPseudoDestructorExpr => {
            let (_, _, destroyed) = x.pseudo_destructor(e);
            scan(w, destroyed, &mut out, 0);
        }
        K::OffsetOfExpr => {
            let written = x
                .offsetof_designator(e)
                .map(|(t, _)| t)
                .or_else(|| x.offsetof_pattern(e).map(|(t, _)| t));
            if let Some(t) = written {
                scan(w, t, &mut out, 0);
            }
        }
        K::VAArgExpr => {
            let (_, t, _) = x.va_arg(e);
            scan(w, t, &mut out, 0);
        }
        K::GenericSelectionExpr => {
            if let GenericControl::Type(t, _) = x.generic_control(e, &ast.lists) {
                scan(w, t, &mut out, 0);
            }
            for a in x.generic_assocs(e, &ast.lists) {
                if let Some(t) = a.ty {
                    scan(w, t, &mut out, 0);
                }
            }
        }
        // A requires-expression's parameters (before adjustment) and type
        // requirements; its other requirements are expressions of the
        // stream.
        K::RequiresExpr => {
            if let Some(&data) = ast.requires_of(e) {
                let ps: Vec<DeclId> = ast.lists.iter::<DeclId>(data.params).collect();
                for p in ps {
                    param(w, p, &mut out);
                }
                let reqs: Vec<Requirement> = ast.templates.requirements(&data).to_vec();
                for r in reqs {
                    if let Requirement::Type(t, _) = r {
                        scan(w, t, &mut out, 0);
                    }
                }
            }
        }
        // No written type: a literal, a name that holds no qualifier or
        // arguments but in the side tables above, an operator, a call, an
        // implicit or initialising node, a statement expression (its
        // declarations are `decl`'s), a lambda (its parameters and return
        // type are its call operator's, `decl`), a pack, a fold, a
        // substituted parameter, a coroutine operator, a vector or atomic
        // builtin, a reflection or splice (unsupported: the walk is
        // indeterminate), an error.
        K::IntegerLiteral
        | K::FloatingLiteral
        | K::CharacterLiteral
        | K::StringLiteral
        | K::CXXBoolLiteralExpr
        | K::CXXNullPtrLiteralExpr
        | K::UserDefinedLiteral
        | K::ImaginaryLiteral
        | K::EmbedExpr
        | K::GNUNullExpr
        | K::CXXThisExpr
        | K::PredefinedExpr
        | K::SourceLocExpr
        | K::UnresolvedLookupExpr
        | K::UnresolvedMemberExpr
        | K::CXXDependentScopeMemberExpr
        | K::SubstNonTypeTemplateParmExpr
        | K::SubstNonTypeTemplateParmPackExpr
        | K::FunctionParmPackExpr
        | K::UnaryOperator
        | K::BinaryOperator
        | K::CompoundAssignOperator
        | K::ConditionalOperator
        | K::BinaryConditionalOperator
        | K::ArraySubscriptExpr
        | K::ParenExpr
        | K::CallExpr
        | K::CXXMemberCallExpr
        | K::CXXOperatorCallExpr
        | K::CXXRewrittenBinaryOperator
        | K::ParenListExpr
        | K::ImplicitCastExpr
        | K::InitListExpr
        | K::ImplicitValueInitExpr
        | K::DesignatedInitExpr
        | K::CXXConstructExpr
        | K::CXXInheritedCtorInitExpr
        | K::CXXDefaultArgExpr
        | K::CXXDefaultInitExpr
        | K::CXXParenListInitExpr
        | K::ArrayInitLoopExpr
        | K::ArrayInitIndexExpr
        | K::CXXStdInitializerListExpr
        | K::NoInitExpr
        | K::DesignatedInitUpdateExpr
        | K::MaterializeTemporaryExpr
        | K::CXXBindTemporaryExpr
        | K::ExprWithCleanups
        | K::OpaqueValueExpr
        | K::CXXDeleteExpr
        | K::CXXThrowExpr
        | K::CXXNoexceptExpr
        | K::ExpressionTraitExpr
        | K::SizeOfPackExpr
        | K::LambdaExpr
        | K::PackExpansionExpr
        | K::CXXFoldExpr
        | K::PackIndexingExpr
        | K::CoawaitExpr
        | K::DependentCoawaitExpr
        | K::CoyieldExpr
        | K::ConstantExpr
        | K::StmtExpr
        | K::AddrLabelExpr
        | K::ChooseExpr
        | K::AtomicExpr
        | K::ShuffleVectorExpr
        | K::ExtVectorElementExpr
        | K::CXXReflectExpr
        | K::CXXSpliceExpr
        | K::RecoveryExpr
        | K::CXXExpansionSelectExpr
        | K::PseudoObjectExpr
        | K::IntegerPackExpr => {}
    }
    let mut named = Vec::new();
    for q in quals {
        scan_nns(w, q, &mut named, 0);
    }
    for l in lists {
        args(w, l, &mut named, 0);
    }
    named.append(&mut out);
    if !named.is_empty() {
        target(w).extend(named);
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
        let ps: Vec<DeclId> = ast
            .decls
            .function(x)
            .map(|f| ast.lists.iter::<DeclId>(f.params).collect())
            .unwrap_or_default();
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
        Op::Unkept(cause) => {
            es.uv(u64::from(UNKEPT));
            es.indeterminate(cause);
            UNKEPT
        }
    };
    w.unit.operands.writing -= 1;
    let nested = std::mem::replace(&mut w.unit.operands.nested, saved).unwrap_or_default();
    let written = site == ALIAS_ARGS || site == UNKEPT || w.op_refs != refs || es.cause().is_some();
    let mine = std::mem::replace(&mut w.names, outer_names);
    if written && let (Some(outer), Some(mine)) = (w.names.as_mut(), mine) {
        for &b in mine.as_bytes() {
            outer.u8(b);
        }
    }
    if counting(w) {
        if site == UNKEPT {
            w.op_counts.unkept += 1;
        } else if written {
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
    push("unkept", c.unkept);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn written(key: u8, kind: KeyKind, cause: Option<Cause>, owner: Option<u8>) -> Written {
        let d = |b: u8| Digest([b; 32]);
        Written {
            entity: None,
            read: None,
            key: d(key),
            kind,
            digest: d(key.wrapping_add(100)),
            cause,
            class: GroupClass::Derived,
            file: Vec::new(),
            resolved: None,
            default_argument: None,
            owner: owner.map(d),
            collided: false,
            view: None,
            denotes: None,
            emitted: false,
            operands: (kind == KeyKind::Operands).then_some(KeyKind::Class),
        }
    }

    // WP-156 fix round 1 (S4, mutation R08). At Kiln's pin no digest test
    // reaches `inherit_causes` with a cause `unit` did not already copy:
    // a parent's walk cause is copied when the operand unit is made, a
    // collision reaches it as a part of its owner
    // (`units::mark_part_collisions`), and a taint
    // (`units::apply_taints`) needs a declaration of a kind whose role is
    // `Unsupported` (`UsingPack`, `FriendTemplate`, `ImplicitParam`,
    // `ImplicitConceptSpecialization`), which Kiln never makes. So the
    // rule is pinned here, on the units directly.
    #[test]
    fn an_operand_unit_inherits_its_parents_cause_after_the_walk() {
        let taint = Cause::Unsupported("DeclKind/UsingPack");
        let mut units = vec![
            written(1, KeyKind::Class, Some(taint), None),
            written(2, KeyKind::Operands, None, Some(1)),
            written(3, KeyKind::Class, None, None),
            written(4, KeyKind::Operands, Some(Cause::Unaccounted("x")), Some(3)),
            written(5, KeyKind::Operands, Some(Cause::Unaccounted("own")), Some(1)),
        ];
        inherit_causes(&mut units);
        assert_eq!(units[1].cause, Some(taint), "the parent's cause is inherited");
        assert_eq!(units[3].cause, Some(Cause::Unaccounted("x")), "no parent cause: its own");
        assert_eq!(units[4].cause, Some(merge(Some(taint), Cause::Unaccounted("own"))));
        assert_eq!(units[0].cause, Some(taint), "a parent is left as it is");
    }
}
