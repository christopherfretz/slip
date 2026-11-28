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
//! is not written as one (its instantiation's is the specialisation's),
//! but the holders inside it whose own operands are not
//! instantiation-dependent are (`W<sizeof(make(x))>` in
//! `decltype(t, W<sizeof(make(x))>())`): [`dependent`] walks it for them
//! (WP-156 fix round 2, X4–X6). A holder met while an entry is walked
//! gives its operands as entries of their own, right after that entry
//! (S7).

use std::collections::{HashMap, HashSet, VecDeque};

use slip_kiln::kiln::ast::decl::{DeclFlags, DeclKind, FriendTarget, VarPlaceholder};
use slip_kiln::kiln::ast::expr::{
    ExprDeps, ExprFlags, ExprKind as K, GenericControl, TypeidOperand,
};
use slip_kiln::kiln::ast::friend_form::{FriendFormAnswer, FriendFormKind};
use slip_kiln::kiln::ast::templates::{Requirement, SpecKind};
use slip_kiln::kiln::ast::types::{ArgView, ExcSpec, Nns, TemplateArg, TypeOfOperand, TypeView};
use slip_kiln::kiln::ast::{
    ArgListId, Ast, DeclId, ExprId, NnsId, QualType, TemplateParamListId, TypeId,
};
use slip_kiln::kiln::source::{Loc, SourceManager};
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

/// A friend read from a module image (WP-156 fix round 2): Kiln carries
/// no friend-form row there (Kiln `spec/ast.md` §23.4), so whether its
/// qualifier or template arguments hold an operand is not known.
pub(crate) const FRIEND_FORM: Cause = Cause::Unsupported("kiln/friend-form-not-carried");

/// A `_BitInt(e)` (WP-156 fix round 2, X8): Kiln keeps its width as a
/// number (`TypeView::BitInt`) or a profile (`DependentBitInt`), never the
/// expression as written, so `_BitInt(sizeof(make(x)) * 8)` cannot be read
/// (`docs/requests/KT1-written-placeholder-and-qualifier.md`).
pub(crate) const BIT_INT: Cause = Cause::Unsupported("kiln/bit-int-width");

/// A lambda or a statement expression inside an instantiation-dependent
/// operand (`decltype(t, [] { ... }())` in a pattern's signature): its
/// body's declarations and statements are not walked for holders
/// ([`dependent`]).
pub(crate) const DEPENDENT_BODY: Cause = Cause::Unsupported("operands/dependent-body");

/// A destructor named by member access outside a template whose class a
/// template-id or `decltype` with an operand names in the translation
/// unit (WP-156 fix round 2, X2: `wv.~Wr<decltype(make(x))>()`): Kiln
/// resolves the name to the destructor and keeps no type as written.
pub(crate) const DESTRUCTOR: Cause = Cause::Unsupported("kiln/destructor-name");

/// A friend function template specialisation (WP-156 fix round 2, X7)
/// whose function type a declarator with an operand may have spelt: Kiln
/// links its declarator's types to nothing ([`Tu::functions`]).
pub(crate) const FRIEND_DECLARATOR: Cause =
    Cause::Unsupported("kiln/friend-specialization-declarator");

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
/// ([`tu`]). Kiln keeps neither a constrained placeholder's
/// type-constraint once it deduced the type, nor a declarator's qualifier,
/// nor an explicit instantiation's template-id and declarator as written,
/// nor a friend specialisation's declarator, nor a destructor's name; the
/// types they wrote are in the type arena, linked to nothing. A type is
/// **linked** when a declaration holds it as written ([`linked`]): what
/// [`signature`] reads. The facts below are read from the types that are
/// not linked, hold an operand ([`holds`]), are not
/// instantiation-dependent and hold no substituted parameter (a
/// substitution's type, which no declaration spelt: [`substituted`]).
#[derive(Debug, Default)]
pub(crate) struct Tu {
    /// The locations (expansion locations, sorted) of the operands in the
    /// written arguments of such a constrained placeholder: a declaration
    /// whose type it deduced ([`placeholder_unkept`]) or a `new` of it
    /// spelt them.
    placeholders: Vec<Loc>,
    /// The locations (expansion locations, sorted) that bound what one
    /// declaration spelt ([`bounds`]), computed on first use.
    bounds: Option<Vec<Loc>>,
    /// The locations (expansion locations, sorted) of the expressions that
    /// are no literal ([`literal`]), computed on first use
    /// ([`qualifier_window_unkept`]).
    exprs: Option<Vec<Loc>>,
    /// The classes (first declarations) that such a template-id names
    /// (an alias template's, or one whose arguments hold an operand): what
    /// a declarator's qualifier with an operand names (Kiln keeps the
    /// template-id's type, `AL<sizeof(make(x))>`, when it is an alias's or
    /// its arguments are types).
    named: HashSet<DeclId>,
    /// The classes that such a `decltype` or `typeof` of an expression
    /// that is no literal names: what a `using enum`'s qualifier names.
    decltyped: HashSet<DeclId>,
    /// The canonical types of such types: what an explicit
    /// instantiation's argument, a declarator's qualifier's argument or a
    /// destructor's name may have spelt.
    held: HashSet<TypeId>,
    /// The canonical result and parameter types of such function types:
    /// what an explicit instantiation's declarator (its own `noexcept(e)`
    /// included) or a friend specialisation's may have spelt.
    functions: HashSet<Vec<QualType>>,
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
/// definition's base specifiers; a friend's type and its friend-form
/// row ([`friend_form`]); the argument list as written of a
/// specialisation. A variable whose type a placeholder deduced has no
/// written type (S3). Where Kiln does not keep what is written, an
/// [`Op::Unkept`] entry: a constrained placeholder ([`PLACEHOLDER`]), an
/// explicit instantiation of a function, variable or member
/// ([`EXPLICIT_INSTANTIATION`]), a declarator's qualifier
/// ([`QUALIFIER`]), a friend specialisation's declarator
/// ([`FRIEND_DECLARATOR`]). [`Walk::op_at`] is `d` while it is read
/// ([`bit_int_unkept`]).
fn signature(w: &mut Walk<'_>, d: DeclId, params: bool, out: &mut Vec<Op>) {
    let saved = w.op_at.replace(At::Decl(d));
    signature_of(w, d, params, out);
    w.op_at = saved;
}

/// [`signature`], once [`At`] is set.
#[allow(
    clippy::too_many_lines,
    reason = "one arm per declaration kind, exhaustive (WP-156 fix round 1)"
)]
#[allow(
    clippy::match_same_arms,
    reason = "the kinds that hold no written type are listed apart, by reason"
)]
fn signature_of(w: &mut Walk<'_>, d: DeclId, params: bool, out: &mut Vec<Op>) {
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
            if ast.decls.function(d).is_some_and(|f| f.spec.return_deduced())
                && placeholder_unkept(w, d)
            {
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
                if placeholder_unkept(w, d) {
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
            let target = slip_kiln::written::friend_target(ast, d);
            if let FriendTarget::Type { ty, owned_tag: _ } = target {
                scan(w, ty, out, 0);
            }
            friend_form(w, d, out);
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
                && using_enum_unkept(w, ast.decls.get(e).ctx())
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

/// The written parts of friend `d` beyond its target (WP-156 fix round 2,
/// X1, X7), from Kiln's friend-form row (Kiln `spec/ast.md` §23): its
/// own template heads, its declarator's qualifier (`friend int
/// Q<sizeof(make(x))>::f();`; a `Nns::Type` component keeps its sugar)
/// and its template-id's arguments as written (`friend void
/// ft<sizeof(make(x))>();`). A class friend's qualifier is its target
/// type's, read above. A friend read from a module image carries no row
/// ([`FRIEND_FORM`]).
fn friend_form(w: &mut Walk<'_>, d: DeclId, out: &mut Vec<Op>) {
    let ast = w.ast;
    match ast.friend_form(d) {
        FriendFormAnswer::Row(v) => {
            let heads: Vec<TemplateParamListId> = v.heads().collect();
            for h in heads {
                head(w, h, out, 0);
            }
            if v.kind() != FriendFormKind::Class
                && let Some(q) = v.qualifier()
            {
                scan_nns(w, q, out, 0);
            }
            if let Some(l) = v.args() {
                args(w, l, out, 0);
            }
            // A specialisation's declarator is linked to nothing: its
            // function is the specialisation, with substituted types.
            if v.kind() == FriendFormKind::Specialization
                && let FriendTarget::Decl(f) = slip_kiln::written::friend_target(ast, d)
                && let Some(q) = ast.decls.get(f).ty()
                && function_key(ast, q).is_some_and(|k| tu(w).functions.contains(&k))
            {
                out.push(Op::Unkept(FRIEND_DECLARATOR));
            }
        }
        FriendFormAnswer::NotCarried => out.push(Op::Unkept(FRIEND_FORM)),
        FriendFormAnswer::None => {}
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
/// written parts may hold an operand Kiln does not keep (it keeps the
/// specialisation, not the directive's template-id or declarator): its
/// qualifier names a class [`in_named_class`]; its template arguments
/// may ([`args_unkept`]); a function's type is, up to its exception
/// specification, one of [`Tu::functions`] (a declarator the directive
/// spelt with an operand, its own `noexcept(e)` included: X3); another's
/// is of [`Tu::held`]. A function type the directive spelt is one no
/// declaration links; one the pattern's substitution made is not tested
/// (`noexcept(sizeof(T) > 1)` holds an operand once `T` is `int`, and
/// belongs to the `exception_spec` unit, `spec/digest.md` §2.4).
fn explicit_instantiation_unkept(w: &mut Walk<'_>, d: DeclId) -> bool {
    let ast = w.ast;
    if !own_explicit_instantiation(w, d) {
        return false;
    }
    let ctx = ast.decls.get(d).ctx();
    if in_named_class(w, ctx, None, None) {
        return true;
    }
    let types = &ast.types;
    let mut written: Vec<QualType> = Vec::new();
    if let Some(q) = ast.decls.get(d).ty() {
        if matches!(types.canonical_view(q), TypeView::Function(_)) {
            if function_key(ast, q).is_some_and(|k| tu(w).functions.contains(&k)) {
                return true;
            }
        } else {
            written.push(q);
        }
    }
    let lists: Vec<ArgListId> = ast.spec_info(d).map(|(_, i)| i.args).into_iter().collect();
    args_unkept(w, lists, written, None)
}

/// Whether a template-id whose converted arguments are `lists` (Kiln
/// keeps them without what was written), or a declarator whose types are
/// `written`, may have spelt an operand: a type that is (canonically) of
/// [`Tu::held`], or a non-type argument (its value alone is kept: `Q<1>`
/// and `Q<sizeof(make(x))>` are one). For the qualifier of declaration
/// `at`, a non-type argument counts only if an expression that is no
/// literal was written before its name ([`qualifier_window_unkept`]);
/// Kiln keeps no location for an explicit instantiation's directive or a
/// `using enum`'s qualifier, so there it counts.
fn args_unkept(
    w: &mut Walk<'_>,
    mut lists: Vec<ArgListId>,
    mut written: Vec<QualType>,
    at: Option<DeclId>,
) -> bool {
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
                ArgView::Value { .. } | ArgView::Profile(_) => value = true,
                ArgView::Expr { expr, .. } => value |= !literal(ast, expr),
                ArgView::Null | ArgView::Template(_) | ArgView::TemplateExpansion { .. } => {}
            }
        }
    }
    if value && at.is_none_or(|d| qualifier_window_unkept(w, d)) {
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
    in_named_class(w, n.ctx(), Some(lexical), Some(d))
}

/// Whether context `c`, or a class around it up to `stop`, is a class of
/// [`Tu::named`] or a class template specialisation whose arguments may
/// have been spelt with an operand ([`args_unkept`]): a qualifier that
/// names a specialisation is a template-id, and Kiln leaves no type for
/// one written in a declarator (`template <> int Q<sizeof(f(x))>::s`).
/// An alias template's template-id (`A3<sizeof(f(x))>::s`) is kept as a
/// type (its arguments as written), so it is of [`Tu::named`] when its
/// arguments hold an operand; an alias template by itself is nothing.
/// `at` is the declaration whose qualifier it is, if it has one
/// ([`args_unkept`]).
fn in_named_class(
    w: &mut Walk<'_>,
    mut c: Option<DeclId>,
    stop: Option<DeclId>,
    at: Option<DeclId>,
) -> bool {
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
        if tu(w).named.contains(&x) {
            return true;
        }
        if let Some((_, i)) = ast.spec_info(x)
            && args_unkept(w, vec![i.args], Vec::new(), at)
        {
            return true;
        }
    }
    false
}

/// Whether a `using enum`'s enumeration, of context `c`, may have been
/// named through a qualifier with an operand: [`in_named_class`], or a
/// class that a `decltype` or `typeof` with an operand names
/// ([`Tu::decltyped`]: `using enum decltype(make(x))::E;`).
fn using_enum_unkept(w: &mut Walk<'_>, c: Option<DeclId>) -> bool {
    let ast = w.ast;
    if let Some(x) = c
        && ast.decls.record(x).is_some()
        && tu(w).decltyped.contains(&ast.first(x))
    {
        return true;
    }
    in_named_class(w, c, None, None)
}

/// The translation unit's facts ([`Tu`]), computed on first use.
fn tu<'w>(w: &'w mut Walk<'_>) -> &'w Tu {
    let (ast, sm) = (w.ast, w.sm);
    w.op_tu.get_or_insert_with(|| tu_of(ast, sm))
}

/// [`Tu`] of `ast`: one pass over the declarations ([`linked`]), one over
/// the type arena.
fn tu_of(ast: &Ast, sm: &SourceManager) -> Tu {
    let types = &ast.types;
    let linked = linked(ast);
    let mut memo = HashMap::new();
    let mut subst = HashMap::new();
    let mut out = Tu::default();
    let mut exprs = Vec::new();
    for i in 1..=types.len() {
        let Ok(i) = u32::try_from(i) else { break };
        let t = TypeId::new(i);
        let q = QualType::unqual(t);
        if types.flags(t).is_instantiation_dependent()
            || linked.contains(&t)
            || !holds(ast, q, &mut memo, 0)
            || substituted(ast, q, &mut subst, 0)
        {
            continue;
        }
        out.held.insert(types.canonical(q).ty());
        match types.view(t) {
            TypeView::Auto { args: Some(l), .. } => args_exprs(ast, l, &mut exprs, 0),
            TypeView::TemplateSpecialization { .. } => named(ast, t, &mut out.named),
            TypeView::Decltype { .. } | TypeView::TypeOf { .. } => {
                named(ast, t, &mut out.decltyped);
            }
            TypeView::Function(_) => {
                if let Some(k) = function_key(ast, q) {
                    out.functions.insert(k);
                }
            }
            _ => {}
        }
    }
    if !exprs.is_empty() {
        for e in exprs {
            for l in [ast.begin_loc(e), ast.end_loc(e)] {
                out.placeholders.push(sm.expansion_loc(l));
            }
        }
        out.placeholders.sort_unstable();
    }
    out
}

/// The bounds of what one declaration spelt ([`Tu::bounds`]): the name
/// of each [`anchor`], and where what it writes after its name ends that
/// Kiln keeps a location for (a function's body's `}`, a variable's or
/// member's initializer's last token). Between two bounds is at most one
/// anchor's name and what its declaration wrote around it (with no
/// bound for what ends a declaration the next window is the wider).
fn bounds_of(ast: &Ast, sm: &SourceManager) -> Vec<Loc> {
    let mut out = Vec::new();
    for i in 1..=ast.decls.len() {
        let Ok(i) = u32::try_from(i) else { break };
        let d = DeclId::new(i);
        if !anchor(ast, d) {
            continue;
        }
        out.push(sm.expansion_loc(ast.decls.get(d).loc()));
        let end = if let Some(b) = ast.decls.function(d).and_then(|f| f.body) {
            (ast.stmts.get(b).kind() == slip_kiln::kiln::ast::stmt::StmtKind::CompoundStmt)
                .then(|| ast.stmts.rbrace(b))
        } else if let Some(e) = ast.decls.var(d).and_then(|v| v.init) {
            Some(ast.end_loc(e))
        } else {
            ast.decls.field(d).and_then(|f| f.init).map(|e| ast.end_loc(e))
        };
        if let Some(l) = end
            && l.is_valid()
        {
            out.push(sm.expansion_loc(l));
        }
    }
    out.sort_unstable();
    out
}

/// The locations of the expressions of `ast` that are no literal
/// ([`Tu::exprs`]).
fn exprs_of(ast: &Ast, sm: &SourceManager) -> Vec<Loc> {
    let mut out = Vec::new();
    for i in 1..=ast.exprs.len() {
        let Ok(i) = u32::try_from(i) else { break };
        let e = ExprId::new(i);
        if !literal(ast, e) {
            out.push(sm.expansion_loc(ast.begin_loc(e)));
        }
    }
    out.sort_unstable();
    out
}

/// The bounds around location `at` ([`bounds_of`]): the last before it
/// and the first after it.
fn window(bounds: &[Loc], at: Loc) -> (Option<Loc>, Option<Loc>) {
    let i = bounds.partition_point(|&x| x < at);
    let lo = i.checked_sub(1).map(|i| bounds[i]);
    let hi = bounds.get(bounds.partition_point(|&x| x <= at)).copied();
    (lo, hi)
}

/// Whether one of the sorted locations `xs` is strictly between `lo` and
/// `hi` (an absent bound is none).
fn between(xs: &[Loc], lo: Option<Loc>, hi: Option<Loc>) -> bool {
    let first = match lo {
        Some(lo) => xs.partition_point(|&x| x <= lo),
        None => 0,
    };
    xs.get(first).is_some_and(|&x| hi.is_none_or(|hi| x < hi))
}

/// Whether the declarator of `d`, a declaration written outside its
/// class, may have spelt an expression that is no literal before its
/// name ([`Tu::exprs`] between the bound before it and its name): what
/// its qualifier's non-type arguments may have been written with (Kiln
/// keeps their values: `Q<1>::s` and `Q<sizeof(make(x))>::s` are one).
/// A location that is not valid fails closed.
fn qualifier_window_unkept(w: &mut Walk<'_>, d: DeclId) -> bool {
    let (ast, sm) = (w.ast, w.sm);
    let at = sm.expansion_loc(ast.decls.get(d).loc());
    if !at.is_valid() {
        return true;
    }
    let tu = w.op_tu.get_or_insert_with(|| tu_of(ast, sm));
    let (lo, _) = window(tu.bounds.get_or_insert_with(|| bounds_of(ast, sm)), at);
    between(tu.exprs.get_or_insert_with(|| exprs_of(ast, sm)), lo, Some(at))
}

/// The types the declarations of `ast` hold as written, with the types
/// written inside them ([`link`]): each declaration's type (a
/// parameter's before adjustment, through its function), a typedef's or
/// alias's, a friend's, an enumeration's underlying type, a class's
/// bases, a template parameter's default, a qualifier of a
/// using-declaration or friend, an argument list as written of a
/// specialisation or friend. What a declaration holds is read by its
/// operand unit's groups; a type of the arena outside the set was
/// written where Kiln links nothing to it (or in an expression: then the
/// set is smaller than what is read, and a fact read from it wider, which
/// fails closed). A type that holds an operand is the type of one
/// spelling (Kiln interns a `decltype` by its expression, a template-id
/// by its argument list, whose expression arguments are their
/// expressions), so a spelling Kiln does not link is not in the set.
fn linked(ast: &Ast) -> HashSet<TypeId> {
    let mut out = HashSet::new();
    for i in 1..=ast.decls.len() {
        let Ok(i) = u32::try_from(i) else { break };
        let d = DeclId::new(i);
        let n = ast.decls.get(d);
        match n.kind() {
            // Through its function: a parameter Kiln links to none (a
            // friend specialisation's) is not read.
            DeclKind::ParmVar => {}
            DeclKind::Typedef | DeclKind::TypeAlias => {
                if let Some(t) = ast.decls.typedef(d) {
                    link(ast, t.underlying, &mut out, 0);
                }
            }
            DeclKind::Friend => {
                if let FriendTarget::Type { ty, owned_tag: _ } =
                    slip_kiln::written::friend_target(ast, d)
                {
                    link(ast, ty, &mut out, 0);
                }
                if let FriendFormAnswer::Row(v) = ast.friend_form(d) {
                    if let Some(q) = v.qualifier() {
                        link_nns(ast, q, &mut out, 0);
                    }
                    if let Some(l) = v.args() {
                        link_args(ast, l, &mut out, 0);
                    }
                }
            }
            DeclKind::Enum => {
                if let Some(q) = ast.decls.enumeration(d).and_then(|e| e.int_type) {
                    link(ast, q, &mut out, 0);
                }
            }
            DeclKind::Using
            | DeclKind::UnresolvedUsingValue
            | DeclKind::UnresolvedUsingTypename => {
                if let Some(q) = ast.using_qualifier(d) {
                    link_nns(ast, q, &mut out, 0);
                }
            }
            _ => {
                if let Some(q) = n.ty() {
                    link(ast, q, &mut out, 0);
                }
            }
        }
        if let Some(f) = ast.decls.function(d) {
            for p in ast.lists.iter::<DeclId>(f.params) {
                let q = ast
                    .decls
                    .param(p)
                    .and_then(|x| x.original_ty)
                    .or_else(|| ast.decls.get(p).ty());
                if let Some(q) = q {
                    link(ast, q, &mut out, 0);
                }
            }
        }
        for b in ast.decls.bases(d, &ast.lists) {
            link(ast, b.ty, &mut out, 0);
        }
        if let Some(l) = ast.templates.args_as_written(d) {
            link_args(ast, l, &mut out, 0);
        }
        if let Some(a) = ast.templates.defaults.get(d).copied()
            && let ArgView::Type(t) = a.view()
        {
            link(ast, t, &mut out, 0);
        }
    }
    out
}

/// Adds written type `q` and the types written inside it to `out`
/// ([`linked`]); a name's underlying type and a `decltype`'s computed one
/// are not written.
fn link(ast: &Ast, q: QualType, out: &mut HashSet<TypeId>, depth: u32) {
    let t = q.ty();
    if depth > MAX_DEPTH || !out.insert(t) {
        return;
    }
    let d = depth + 1;
    let types = &ast.types;
    match types.view(t) {
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
        | TypeView::TypeOf { operand: TypeOfOperand::Type(x), .. } => link(ast, x, out, d),
        TypeView::ConstantArray(a) => link(ast, a.elem, out, d),
        TypeView::QualifiedArray { base, .. } => link(ast, QualType::unqual(base), out, d),
        TypeView::MemberPointer { pointee, class } => {
            link(ast, pointee, out, d);
            link(ast, QualType::unqual(class), out, d);
        }
        TypeView::Function(p) => {
            link(ast, p.ret(), out, d);
            for &x in types.params(&p) {
                link(ast, x, out, d);
            }
        }
        TypeView::Elaborated { nns, named, .. } => {
            if let Some(n) = nns {
                link_nns(ast, n, out, d);
            }
            link(ast, named, out, d);
        }
        TypeView::DependentName { nns, .. } => link_nns(ast, nns, out, d),
        TypeView::DependentTemplateSpecialization { nns, args: l, .. } => {
            link_nns(ast, nns, out, d);
            link_args(ast, l, out, d);
        }
        TypeView::TemplateSpecialization { args: l, .. } | TypeView::Auto { args: Some(l), .. } => {
            link_args(ast, l, out, d);
        }
        _ => {}
    }
}

/// [`link`] for the type arguments of argument list `l`.
fn link_args(ast: &Ast, l: ArgListId, out: &mut HashSet<TypeId>, depth: u32) {
    if depth > MAX_DEPTH {
        return;
    }
    for a in ast.types.args(l) {
        match a.view() {
            ArgView::Type(t) => link(ast, t, out, depth + 1),
            ArgView::Pack(p) => link_args(ast, p, out, depth + 1),
            _ => {}
        }
    }
}

/// [`link`] for the types nested-name-specifier `n` names.
fn link_nns(ast: &Ast, n: NnsId, out: &mut HashSet<TypeId>, depth: u32) {
    if depth > MAX_DEPTH {
        return;
    }
    match ast.types.nns_view(n) {
        Nns::Type { prefix, ty, .. } => {
            if let Some(p) = prefix {
                link_nns(ast, p, out, depth + 1);
            }
            link(ast, ty, out, depth + 1);
        }
        Nns::Namespace { prefix, .. }
        | Nns::NamespaceAlias { prefix, .. }
        | Nns::Identifier { prefix, .. } => {
            if let Some(p) = prefix {
                link_nns(ast, p, out, depth + 1);
            }
        }
        Nns::Splice(_) | Nns::Global => {}
    }
}

/// Whether type `q` holds a substituted template parameter
/// (`SubstTemplateTypeParm`) in what it writes: a type a substitution
/// made (a pattern's, instantiated), which no explicit instantiation,
/// qualifier or destructor name spells (none is in a template). Its
/// expressions are not read (a fact read from such a type fails closed).
fn substituted(ast: &Ast, q: QualType, memo: &mut HashMap<TypeId, bool>, depth: u32) -> bool {
    if depth > MAX_DEPTH {
        return true;
    }
    let t = q.ty();
    if let Some(&s) = memo.get(&t) {
        return s;
    }
    let d = depth + 1;
    let types = &ast.types;
    let any = |xs: &[QualType], memo: &mut HashMap<TypeId, bool>| {
        xs.iter().any(|&x| substituted(ast, x, memo, d))
    };
    let s = match types.view(t) {
        TypeView::SubstTemplateTypeParm { .. } | TypeView::SubstTemplateTypeParmPack { .. } => true,
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
        | TypeView::TypeOf { operand: TypeOfOperand::Type(x), .. } => any(&[x], memo),
        TypeView::ConstantArray(a) => any(&[a.elem], memo),
        TypeView::QualifiedArray { base, .. } => any(&[QualType::unqual(base)], memo),
        TypeView::MemberPointer { pointee, class } => {
            any(&[pointee, QualType::unqual(class)], memo)
        }
        TypeView::Function(p) => {
            let mut xs = vec![p.ret()];
            xs.extend_from_slice(types.params(&p));
            any(&xs, memo)
        }
        TypeView::Elaborated { named, .. } => any(&[named], memo),
        TypeView::TemplateSpecialization { args: l, .. } | TypeView::Auto { args: Some(l), .. } => {
            let mut xs = Vec::new();
            arg_types(ast, l, &mut xs, 0);
            any(&xs, memo)
        }
        _ => false,
    };
    memo.insert(t, s);
    s
}

/// The type arguments of argument list `l` (through packs), into `out`.
fn arg_types(ast: &Ast, l: ArgListId, out: &mut Vec<QualType>, depth: u32) {
    if depth > MAX_DEPTH {
        return;
    }
    for a in ast.types.args(l) {
        match a.view() {
            ArgView::Type(t) => out.push(t),
            ArgView::Pack(p) => arg_types(ast, p, out, depth + 1),
            _ => {}
        }
    }
}

/// The operand expressions written in argument list `l` (an expression
/// argument, a `decltype` or `typeof` operand and an expression argument
/// in a type argument), into `out`.
fn args_exprs(ast: &Ast, l: ArgListId, out: &mut Vec<ExprId>, depth: u32) {
    if depth > MAX_DEPTH {
        return;
    }
    for a in ast.types.args(l) {
        match a.view() {
            ArgView::Type(t) => type_exprs(ast, t, out, depth + 1),
            ArgView::Expr { expr, .. } => out.push(expr),
            ArgView::Pack(p) => args_exprs(ast, p, out, depth + 1),
            _ => {}
        }
    }
}

/// [`args_exprs`] for written type `q`.
fn type_exprs(ast: &Ast, q: QualType, out: &mut Vec<ExprId>, depth: u32) {
    if depth > MAX_DEPTH {
        return;
    }
    let d = depth + 1;
    match ast.types.view(q.ty()) {
        TypeView::Decltype { expr, .. }
        | TypeView::TypeOf { operand: TypeOfOperand::Expr(expr), .. } => {
            out.push(expr);
        }
        TypeView::Pointer(x)
        | TypeView::LValueRef(x)
        | TypeView::RValueRef(x)
        | TypeView::Paren(x)
        | TypeView::ExtQual { base: x, .. }
        | TypeView::Attributed { modified: x, .. }
        | TypeView::Elaborated { named: x, .. }
        | TypeView::ConstantArray(slip_kiln::kiln::ast::types::ConstArray { elem: x, .. })
        | TypeView::TypeOf { operand: TypeOfOperand::Type(x), .. } => type_exprs(ast, x, out, d),
        TypeView::TemplateSpecialization { args: l, .. } | TypeView::Auto { args: Some(l), .. } => {
            args_exprs(ast, l, out, d);
        }
        TypeView::Function(p) => {
            type_exprs(ast, p.ret(), out, d);
            for &x in ast.types.params(&p) {
                type_exprs(ast, x, out, d);
            }
        }
        _ => {}
    }
}

/// The canonical result and parameter types of function type `q` (no
/// exception specification: an explicit instantiation's own need not be
/// spelt alike), the key of [`Tu::functions`].
fn function_key(ast: &Ast, q: QualType) -> Option<Vec<QualType>> {
    let types = &ast.types;
    match types.canonical_view(q) {
        TypeView::Function(p) => {
            let mut k = vec![types.canonical(p.ret())];
            k.extend(types.params(&p).iter().map(|&x| types.canonical(x)));
            Some(k)
        }
        _ => None,
    }
}

/// Whether declaration `d` bounds what another declaration spells
/// ([`placeholder_unkept`]): a declaration written in the source (not
/// implicit, no parameter) that is in no function, lambda or
/// requires-expression, whose declarations can sit inside an operand
/// (`C<decltype([] { int k = 0; return k; }())> auto v`).
fn anchor(ast: &Ast, d: DeclId) -> bool {
    let n = ast.decls.get(d);
    // A class or enumeration that is no definition can be declared inside
    // an operand or a type-constraint's arguments (`C<decltype(make(x)),
    // struct Z*> auto v`), between what it bounds and the declaration.
    let tag = ast.decls.record(d).is_some() || n.kind() == DeclKind::Enum;
    if n.has(DeclFlags::IMPLICIT)
        || !n.loc().is_valid()
        || (tag && !n.misc().is_definition())
        || matches!(
            n.kind(),
            DeclKind::ParmVar
                | DeclKind::ImplicitParam
                | DeclKind::TemplateTypeParm
                | DeclKind::NonTypeTemplateParm
                | DeclKind::TemplateTemplateParm
                | DeclKind::Binding
                | DeclKind::TranslationUnit
                | DeclKind::RequiresExprBody
                | DeclKind::LifetimeExtendedTemporary
                | DeclKind::TemplateParamObject
                | DeclKind::ImplicitConceptSpecialization
        )
    {
        return false;
    }
    let mut around = Some(d);
    for _ in 0..MAX_DEPTH {
        let Some(x) = around else { return true };
        if ast.decls.get(x).kind() == DeclKind::RequiresExprBody
            || ast.decls.record(x).is_some_and(|r| r.lambda.is_some())
            || (x != d && ast.decls.function(x).is_some())
        {
            return false;
        }
        around = ast.decls.lexical_ctx(x);
    }
    false
}

/// Whether declaration `d`, whose type a placeholder deduced, may have
/// spelt a constrained placeholder whose arguments hold an operand: one
/// of [`Tu::placeholders`] is between the bounds around `d` ([`window`]:
/// its type-constraint is before its name, or after it in a trailing
/// return type). Kiln keeps the deduced type alone (`PLACEHOLDER`).
fn placeholder_unkept(w: &mut Walk<'_>, d: DeclId) -> bool {
    let (ast, sm) = (w.ast, w.sm);
    let at = sm.expansion_loc(ast.decls.get(d).loc());
    let tu = w.op_tu.get_or_insert_with(|| tu_of(ast, sm));
    if tu.placeholders.is_empty() {
        return false;
    }
    if !at.is_valid() {
        return true;
    }
    let (lo, hi) = window(tu.bounds.get_or_insert_with(|| bounds_of(ast, sm)), at);
    between(&tu.placeholders, lo, hi)
}

/// Whether `new` expression `e` may be of a constrained placeholder whose
/// arguments hold an operand: one of [`Tu::placeholders`] is inside it.
fn new_placeholder_unkept(w: &mut Walk<'_>, e: ExprId) -> bool {
    let ast = w.ast;
    let (lo, hi) = (w.sm.expansion_loc(ast.begin_loc(e)), w.sm.expansion_loc(ast.end_loc(e)));
    let p = &tu(w).placeholders;
    if p.is_empty() {
        return false;
    }
    if !lo.is_valid() || !hi.is_valid() {
        return true;
    }
    p.get(p.partition_point(|&x| x < lo)).is_some_and(|&x| x <= hi)
}

/// Whether member access `e` names a destructor (`wv.~Wr<…>()`) of a
/// class of [`Tu::named`] or [`Tu::decltyped`]: its name may have been a
/// template-id or `decltype` with an operand, which Kiln does not keep.
fn destructor_unkept(w: &mut Walk<'_>, e: ExprId) -> bool {
    let ast = w.ast;
    let Some(d) = DeclId::try_new(ast.exprs.get(e).b()) else { return false };
    if ast.decls.get(d).kind() != DeclKind::CXXDestructor {
        return false;
    }
    let Some(c) = ast.decls.get(d).ctx() else { return false };
    let c = ast.first(c);
    let tu = tu(w);
    tu.named.contains(&c) || tu.decltyped.contains(&c)
}

/// Where the type [`scan`] reads was written ([`Walk::op_at`]): in a
/// declaration's signature ([`signature`]) or an expression ([`holders`]).
#[derive(Clone, Copy, Debug)]
pub(crate) enum At {
    /// In declaration `d`'s.
    Decl(DeclId),
    /// In expression `e` (its own node).
    Expr(ExprId),
}

/// Whether a `_BitInt(N)` read where [`Walk::op_at`] says may have had
/// its width written with an expression that is no literal (Kiln keeps the
/// number alone, and interns `_BitInt(8)` and `_BitInt(sizeof(make(x)))`
/// alike): in a declaration, one of [`Tu::exprs`] is between the bounds
/// around its name ([`window`]); in an expression, one begins inside it
/// after its first token that is not below it. Unknown: it may.
fn bit_int_unkept(w: &mut Walk<'_>) -> bool {
    let (ast, sm) = (w.ast, w.sm);
    let at = w.op_at;
    let tu = w.op_tu.get_or_insert_with(|| tu_of(ast, sm));
    let exprs = tu.exprs.get_or_insert_with(|| exprs_of(ast, sm));
    match at {
        Some(At::Decl(d)) => {
            let at = sm.expansion_loc(ast.decls.get(d).loc());
            if !at.is_valid() {
                return true;
            }
            let (lo, hi) = window(tu.bounds.get_or_insert_with(|| bounds_of(ast, sm)), at);
            between(exprs, lo, hi)
        }
        Some(At::Expr(e)) => {
            let (lo, hi) = (sm.expansion_loc(ast.begin_loc(e)), sm.expansion_loc(ast.end_loc(e)));
            if !lo.is_valid() || !hi.is_valid() {
                return true;
            }
            // Strictly after its first token: what begins there is it or
            // around it (`(_BitInt(8))1 << 2`), and no width does.
            let inside = exprs.partition_point(|&x| x <= hi) - exprs.partition_point(|&x| x <= lo);
            let mut own = 0usize;
            let mut stack = vec![e];
            while let Some(x) = stack.pop() {
                let l = sm.expansion_loc(ast.begin_loc(x));
                if !literal(ast, x) && lo < l && l <= hi {
                    own += 1;
                }
                ast.exprs.children_into(x, &ast.lists, &mut stack);
            }
            inside > own
        }
        None => true,
    }
}

/// Adds to `out` the class type `t` names, if it names one.
fn named(ast: &Ast, t: TypeId, out: &mut HashSet<DeclId>) {
    if let TypeView::Record(c) = ast.types.canonical_view(QualType::unqual(t)) {
        out.insert(ast.first(c));
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
                || matches!(types.exc_spec(&p), ExcSpec::NoexceptExpr { expr, .. }
                    if not_dep(expr) && !literal(ast, expr))
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
        TypeView::Splice { .. } | TypeView::BitInt { .. } | TypeView::DependentBitInt { .. } => {
            true
        }
        TypeView::Invalid
        | TypeView::Builtin(_)
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
                // A dependent one is walked in full by the declaration's
                // stream (`Walk::own_exception_spec`), but a local
                // declaration's, which is written as Kiln holds it (a
                // profile): its holders are read here.
                if w.is_local(d) {
                    push(w, Op::Expr(NOEXCEPT, expr), out, depth);
                } else if !crate::demand::deferrable(w, d) {
                    push_own(w, Op::Expr(NOEXCEPT, expr), out);
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
        push_own(w, Op::Expr(NOEXCEPT, expr), &mut out);
        target(w).extend(out);
    }
}

/// Whether `op` is instantiation-dependent: a pattern's operand, which
/// is not written (its canonical form is a profile).
fn is_dependent(w: &Walk<'_>, op: Op) -> bool {
    match op {
        Op::Expr(_, e) => w.ast.exprs.get(e).deps().contains(ExprDeps::INSTANTIATION),
        Op::Alias(_) | Op::Unkept(_) => false,
    }
}

/// Appends `op` to `out`; an instantiation-dependent operand is not
/// written (counted), but the holders inside it that are not are
/// ([`dependent`]).
fn push(w: &mut Walk<'_>, op: Op, out: &mut Vec<Op>, depth: u32) {
    match op {
        Op::Expr(_, e) if is_dependent(w, op) => dependent(w, e, out, depth + 1),
        _ => out.push(op),
    }
}

/// Appends `op` to `out` unless it is instantiation-dependent (counted):
/// an operand the unit's own stream walks in full when it is dependent
/// (a declaration's own `noexcept(e)`, `Walk::write_exc_spec`), whose
/// holders the stream's walk meets ([`expr_node`]).
fn push_own(w: &mut Walk<'_>, op: Op, out: &mut Vec<Op>) {
    if is_dependent(w, op) {
        if counting(w) {
            w.op_counts.dependent += 1;
        }
    } else {
        out.push(op);
    }
}

/// The holders inside instantiation-dependent expression `e`, a pattern's
/// operand that is not written ([`push`]; counted): a pattern's
/// canonical form keeps the expression's profile, in which a type is its
/// canonical form and a constant its value, so a holder inside it whose
/// operand is not instantiation-dependent (`decltype(t,
/// W2<sizeof(make(x))>())`, `W2<sizeof(T) + sizeof(Wr<decltype(make(x))>)>`)
/// is written nowhere else (WP-156 fix round 2, X4–X6). Each node's
/// holders are read as [`expr_node`] reads them, in pre-order; a
/// requirement's expressions are walked, a lambda's or a statement
/// expression's body is not ([`DEPENDENT_BODY`]).
fn dependent(w: &mut Walk<'_>, e: ExprId, out: &mut Vec<Op>, depth: u32) {
    if counting(w) {
        w.op_counts.dependent += 1;
    }
    if depth > MAX_DEPTH {
        out.push(Op::Unkept(TOO_MANY));
        return;
    }
    let ast = w.ast;
    let mut stack = vec![e];
    let mut nodes = 0usize;
    while let Some(x) = stack.pop() {
        nodes += 1;
        if nodes > MAX_ENTRIES {
            out.push(Op::Unkept(TOO_MANY));
            return;
        }
        match ast.exprs.get(x).kind() {
            K::LambdaExpr | K::StmtExpr => {
                out.push(Op::Unkept(DEPENDENT_BODY));
                continue;
            }
            // Its expressions (Kiln gives a requires-expression no
            // children); its parameters and type requirements are
            // [`holders`]'.
            K::RequiresExpr => {
                if let Some(&data) = ast.requires_of(x) {
                    for r in ast.templates.requirements(&data).iter().rev() {
                        match *r {
                            Requirement::Simple(y) | Requirement::Nested(y) => stack.push(y),
                            Requirement::Compound { expr, constraint, .. } => {
                                stack.extend(constraint);
                                stack.push(expr);
                            }
                            Requirement::Type(..) => {}
                        }
                    }
                }
            }
            _ => {}
        }
        holders(w, x, out, depth);
        let kids = ast.exprs.children(x, &ast.lists);
        stack.extend(kids.into_iter().rev());
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
                push(w, Op::Expr(NOEXCEPT, expr), out, d);
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
        TypeView::Decltype { expr, .. } => push(w, Op::Expr(DECLTYPE, expr), out, d),
        TypeView::TypeOf { operand: TypeOfOperand::Expr(e), .. } => {
            push(w, Op::Expr(TYPEOF, e), out, d);
        }
        TypeView::TypeOf { operand: TypeOfOperand::Type(t), .. } => scan(w, t, out, d),
        TypeView::Splice { .. } => out.push(Op::Unkept(SPLICE)),
        // A bit-precise integer's width as written is not kept: a
        // pattern's is an expression (it fails closed), another's fails
        // closed where an expression could have been written
        // ([`bit_int_unkept`]).
        TypeView::DependentBitInt { .. } => out.push(Op::Unkept(BIT_INT)),
        TypeView::BitInt { .. } => {
            if bit_int_unkept(w) {
                out.push(Op::Unkept(BIT_INT));
            }
        }
        // What a name denotes (a typedef's, alias's or using-declaration's
        // type, a class, an enumeration, an injected class name), a
        // substituted parameter's replacement, a parameter, a placeholder
        // with no written argument or what one deduced, a dependent
        // `decltype` or `typeof` (the canonical form of a pattern's, never
        // a written type: a written one is `Decltype`, whose dependent
        // operand [`push`] walks), a builtin: nothing is read.
        TypeView::Invalid
        | TypeView::Builtin(_)
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
            ArgView::Expr { expr, .. } => push(w, Op::Expr(EXPR_ARG, expr), out, depth + 1),
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
pub(crate) fn expr_node(w: &mut Walk<'_>, e: ExprId) {
    if !collecting(w) {
        return;
    }
    let mut out = Vec::new();
    holders(w, e, &mut out, 0);
    if !out.is_empty() {
        target(w).extend(out);
    }
}

/// The operands of the holders of expression `e` (its own node only) into
/// `out`: [`expr_node`], and [`dependent`] for each node of a dependent
/// operand.
fn holders(w: &mut Walk<'_>, e: ExprId, sink: &mut Vec<Op>, depth: u32) {
    let saved = w.op_at.replace(At::Expr(e));
    holders_of(w, e, sink, depth);
    w.op_at = saved;
}

/// [`holders`], once [`At`] is set.
#[allow(
    clippy::too_many_lines,
    reason = "one arm per expression kind, exhaustive (WP-156 fix round 1)"
)]
#[allow(
    clippy::many_single_char_names,
    reason = "e expression, n node, k kind, x arena, as own_spelling"
)]
fn holders_of(w: &mut Walk<'_>, e: ExprId, sink: &mut Vec<Op>, depth: u32) {
    if depth > MAX_DEPTH {
        sink.push(Op::Unkept(TOO_MANY));
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
            if n.kind() == K::MemberExpr && destructor_unkept(w, e) {
                out.push(Op::Unkept(DESTRUCTOR));
            }
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
            scan(w, written, &mut out, depth);
        }
        // `T(...)`, `T{...}` and `T()`: an explicit type conversion in
        // functional notation, whose node type is the type as written;
        // `__builtin_convertvector(v, T)`, whose node type is `T`.
        K::CXXTemporaryObjectExpr | K::CXXScalarValueInitExpr | K::ConvertVectorExpr => {
            scan(w, n.ty(), &mut out, depth);
        }
        K::CXXUnresolvedConstructExpr => scan(w, x.unresolved_construct_type(e), &mut out, depth),
        K::UnaryExprOrTypeTraitExpr => {
            if n.has(ExprFlags::ARG_IS_TYPE)
                && let Some(t) = x.trait_type_arg(e)
            {
                scan(w, t, &mut out, depth);
            }
        }
        K::CXXTypeidExpr => {
            if let TypeidOperand::Type(t) = x.typeid_operand(e) {
                scan(w, t, &mut out, depth);
            }
        }
        K::TypeTraitExpr => {
            let ts: Vec<QualType> = ast.lists.iter::<QualType>(x.type_trait_args(e)).collect();
            for t in ts {
                scan(w, t, &mut out, depth);
            }
        }
        K::ArrayTypeTraitExpr => scan(w, x.array_trait_type(e), &mut out, depth),
        K::CXXNewExpr => {
            // A `new` of a constrained placeholder (`new C<…> auto(1)`)
            // has the deduced type: Kiln keeps no mark of it.
            if new_placeholder_unkept(w, e) {
                out.push(Op::Unkept(PLACEHOLDER));
            }
            scan(w, x.new_extra(e).allocated_ty, &mut out, depth);
        }
        K::CompoundLiteralExpr => scan(w, x.compound_literal_written_type(e), &mut out, depth),
        K::CXXPseudoDestructorExpr => {
            let (_, _, destroyed) = x.pseudo_destructor(e);
            scan(w, destroyed, &mut out, depth);
        }
        K::OffsetOfExpr => {
            let written = x
                .offsetof_designator(e)
                .map(|(t, _)| t)
                .or_else(|| x.offsetof_pattern(e).map(|(t, _)| t));
            if let Some(t) = written {
                scan(w, t, &mut out, depth);
            }
        }
        K::VAArgExpr => {
            let (_, t, _) = x.va_arg(e);
            scan(w, t, &mut out, depth);
        }
        K::GenericSelectionExpr => {
            if let GenericControl::Type(t, _) = x.generic_control(e, &ast.lists) {
                scan(w, t, &mut out, depth);
            }
            for a in x.generic_assocs(e, &ast.lists) {
                if let Some(t) = a.ty {
                    scan(w, t, &mut out, depth);
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
                        scan(w, t, &mut out, depth);
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
    for q in quals {
        scan_nns(w, q, sink, depth);
    }
    for l in lists {
        args(w, l, sink, depth);
    }
    sink.append(&mut out);
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
    unit_of(w, Some(e), parent, h)
}

/// Kiln's written-reference table (Kiln `spec/ast.md` §18) is not
/// recorded, or not whole (a snapshot or module image made without it):
/// the ledger of written names cannot be checked (WP-156 fix round 3,
/// L0), so the translation unit's `<order>` unit has an operand unit with
/// this cause. Every digested parse records the table.
pub(crate) const NOT_RECORDED: Cause = Cause::Unsupported("kiln/written-refs-not-recorded");

/// The operand unit of the translation unit's `<order>` unit (`parent`,
/// written): one [`Op::Unkept`] entry of [`NOT_RECORDED`] when the
/// written-reference table is not on or not whole (L0); `None` otherwise.
pub(crate) fn order(w: &mut Walk<'_>, parent: &Written) -> Option<Written> {
    let refs = &w.ast.written_refs;
    if refs.is_on() && refs.unrecorded().is_empty() {
        return None;
    }
    let (es, _, _) = entry(w, Op::Unkept(NOT_RECORDED));
    let mut s = Stream::new();
    s.count(1);
    s.raw(es.as_bytes()).absorb(es.cause());
    let h = s.finish(UNIT_TAG);
    let h = w.hashed(h);
    Some(unit_of(w, None, parent, h))
}

fn unit_of(w: &mut Walk<'_>, e: Option<DeclId>, parent: &Written, h: Hashed) -> Written {
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
                parent.read.or(e).map_or_else(Vec::new, |d| crate::units::origin_file(w, d))
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
        entity: e,
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
