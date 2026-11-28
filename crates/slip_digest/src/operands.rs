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

use slip_kiln::kiln::ast::decl::{DeclKind, FriendTarget, VarPlaceholder};
use slip_kiln::kiln::ast::expr::{
    ExprDeps, ExprFlags, ExprKind as K, GenericControl, TypeidOperand,
};
use slip_kiln::kiln::ast::friend_form::{FriendFormAnswer, FriendFormKind};
use slip_kiln::kiln::ast::templates::{Requirement, SpecKind};
use slip_kiln::kiln::ast::types::{ArgView, ExcSpec, ExprOperand};
use slip_kiln::kiln::ast::written_refs::{QualLink, RefRole, WrittenRefId};
use slip_kiln::kiln::ast::{ArgListId, Ast, DeclId, ExprId, NnsId, QualType, TemplateParamListId};
use slip_kiln::kiln::source::Loc;
use slip_model::Digest;
use slip_model::digests::GroupClass;
use slip_model::keys::{Key, KeyKind, Name, Parent};

use crate::Cause;
use crate::stream::{Hashed, Stream, UNIT_TAG};
use crate::units::{Written, merge};
use crate::walk::Walk;
use ledger::Claim;

pub(crate) mod census;
pub(crate) mod ledger;
mod slots;

pub(crate) use slots::{Part, Slot, slots};

/// The sites (digest-kinds §11.11). 3 (array bound), 5 (pack index) and 7
/// (`explicit(e)`) are reserved.
pub(crate) const DECLTYPE: u8 = 1;
pub(crate) const TYPEOF: u8 = 2;
pub(crate) const EXPR_ARG: u8 = 4;
pub(crate) const NOEXCEPT: u8 = 6;
pub(crate) const ALIAS_ARGS: u8 = 8;
/// Site 9: a written name no stream of its unit walked (WP-156 fix round
/// 3, L5): the entity it names.
pub(crate) const NAME: u8 = 9;
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

/// A splice (`[: e :]`, P2996) in a written type or nested-name-specifier:
/// its operand is not read (the expression walk makes a splice
/// unsupported too).
pub(crate) const SPLICE: Cause = Cause::Unsupported("operands/splice");

/// A friend read from a module image (WP-156 fix round 2): Kiln carries
/// no friend-form row there (Kiln `spec/ast.md` §23.4), so whether its
/// qualifier or template arguments hold an operand is not known.
pub(crate) const FRIEND_FORM: Cause = Cause::Unsupported("kiln/friend-form-not-carried");

/// A pattern's `_BitInt(e)` (`DependentBitInt`, WP-156 fix round 2, X8):
/// Kiln keeps its width as a profile, never the expression as written
/// (`docs/requests/KT1-written-placeholder-and-qualifier.md`). A
/// `_BitInt(N)` outside a pattern keeps a number: its names are the
/// ledger's (L3) and its type names L4's trigger T3's.
pub(crate) const BIT_INT: Cause = Cause::Unsupported("kiln/bit-int-width");

/// A lambda or a statement expression inside an instantiation-dependent
/// operand (`decltype(t, [] { ... }())` in a pattern's signature): its
/// body's declarations and statements are not walked for holders
/// ([`dependent`]).
pub(crate) const DEPENDENT_BODY: Cause = Cause::Unsupported("operands/dependent-body");

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
    /// Site 9 (L5): a written name of the unit that no stream of it
    /// walked, by the entity it names, and its entry (the trace's mark).
    Name(DeclId, WrittenRefId),
}

/// A group of site-9 entries written ahead of its unit's stream
/// (`<order>`'s, [`order_names`]): how many, the stream, and where each
/// entry begins in it with its written name's location (the trace's
/// marks, `spec/verify.md` §6.1).
#[derive(Debug)]
pub(crate) struct Names {
    count: usize,
    stream: Stream,
    marks: Vec<(usize, Loc)>,
}

/// The operand state of the unit being written (`UnitState::operands`).
#[derive(Debug, Default)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent facts of one unit's walk, each read by its own rule"
)]
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
    /// The unit read a [`Slot::Folded`] (Layer S): a value Kiln computed
    /// from an expression it does not keep (L4's trigger T3).
    pub(crate) folded: bool,
    /// The unit's walk met a `new` expression (L4's trigger T4: a `new`
    /// of a constrained placeholder keeps the deduced type alone).
    news: bool,
    /// Collected for the numbering of an owner (`local::compute`), whose
    /// streams are no unit's: the entries are walked so that the local
    /// types and objects in them are numbered ([`number`]).
    numbering: bool,
    /// The entity of the unit (the owner's, for a part): whose entries of
    /// the ledger [`finish`] decides (L2).
    entity: Option<DeclId>,
    /// The declaration the unit's stream is read from; `None` for a part.
    from: Option<DeclId>,
    /// Which part this is, for a part ([`set_part`]).
    part: Option<ledger::PartOf>,
    /// The name expressions the unit's kept streams wrote (L3): the
    /// location and declaration of each `DeclRefExpr` and `MemberExpr`,
    /// and of an operator call's callee ([`mark`]).
    marks: Vec<(Loc, DeclId)>,
}

/// The counters of `spec/digest.md` §12 for operand units (WP-156 item 9).
#[derive(Debug, Default)]
pub(crate) struct Counts {
    pub units: u64,
    /// Written operands by site (index: the site).
    pub sites: [u64; 10],
    /// L6: the entries no unit decided (`<order>` is then indeterminate).
    pub unclaimed: u64,
    /// L6: the entries deferred to a part that was never written.
    pub undemanded: u64,
    /// Operands of sites 1, 2, 4, 6 whose walk wrote no reference (S5).
    pub unreferenced: u64,
    /// Instantiation-dependent operands, not written.
    pub dependent: u64,
    /// Declarations whose type a placeholder deduced: not read (S3).
    pub deduced: u64,
    /// Entries of a holder Kiln does not keep ([`Op::Unkept`]).
    pub unkept: u64,
    /// The site-9 entries by why they are written, by role, by holder
    /// kind, and the unmet ones by role and holder kind (the corpus
    /// command's split, item 9): `names.<key>`.
    pub names: std::collections::BTreeMap<String, u64>,
}

impl Counts {
    /// One site-9 entry of role `role` held by a declaration of kind
    /// `holder` (`None`: no holder), written as `why` (`unmet`, or the
    /// trigger: `t1` ... `t5`).
    fn name(&mut self, why: &str, role: RefRole, holder: Option<DeclKind>) {
        let holder = holder.map_or_else(|| "none".to_owned(), |k| snake(&format!("{k:?}")));
        let role = snake(&format!("{role:?}"));
        let mut bump = |k: String| *self.names.entry(k).or_insert(0) += 1;
        if why == "unmet" {
            bump("unmet".to_owned());
            bump(format!("unmet.{role}.{holder}"));
        } else {
            bump(format!("trigger.{why}"));
        }
        bump(format!("role.{role}"));
        bump(format!("holder.{holder}"));
    }
}

/// A kind's name as a counter's word (`spec/digest.md` §12: `[a-z0-9_]`):
/// `CXXRecord` is `cxx_record`, `UdSuffix` is `ud_suffix`.
fn snake(name: &str) -> String {
    let b = name.as_bytes();
    let mut out = String::with_capacity(name.len() + 4);
    for (i, &c) in b.iter().enumerate() {
        if c.is_ascii_uppercase() {
            let after_lower = i > 0 && !b[i - 1].is_ascii_uppercase();
            let before_lower = i > 0 && b.get(i + 1).is_some_and(u8::is_ascii_lowercase);
            if after_lower || (before_lower && b[i - 1].is_ascii_uppercase()) {
                out.push('_');
            }
            out.push(char::from(c.to_ascii_lowercase()));
        } else if c.is_ascii_alphanumeric() {
            out.push(char::from(c));
        } else {
            out.push('_');
        }
    }
    out
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
    // L4's T3 and T4 marks read while the signatures are built belong to
    // this unit (the state they are set on is the one replaced below).
    w.unit.operands.folded = false;
    w.unit.operands.news = false;
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
    let (folded, news) = (w.unit.operands.folded, w.unit.operands.news);
    w.unit.operands = State {
        on: true,
        covered,
        groups,
        own,
        nested: None,
        writing: 0,
        folded,
        news,
        numbering,
        entity: Some(w.entity(e)),
        from,
        part: None,
        marks: Vec::new(),
    };
}

/// Says which part the unit begun with `from` `None` is
/// (`demand::write_part`): what [`finish`] decides of the entries its
/// owner deferred (L3).
pub(crate) fn set_part(w: &mut Walk<'_>, part: ledger::PartOf) {
    w.unit.operands.part = Some(part);
}

/// The hook of `expr::write_node` (L3): records the name expression `e`
/// written in a kept stream of the unit (its own, or an operand entry's;
/// those of an entry that is not written are dropped, [`entry`]). A
/// stream walked and dropped, or for the numbering of an owner, has a
/// state of its own, which no [`finish`] reads.
pub(crate) fn mark(w: &mut Walk<'_>, e: ExprId) {
    if !collecting(w) {
        return;
    }
    let ast = w.ast;
    let n = ast.exprs.get(e);
    match n.kind() {
        K::DeclRefExpr | K::MemberExpr => {
            let d = ast.exprs.decl(e);
            w.unit.operands.marks.push((n.loc(), d));
        }
        // The operator join (Kiln §18.13.7): the entry is at the call's
        // location and names the callee's function. Its callee
        // `DeclRefExpr` is at the same token and is marked as it is
        // written; this marks the call when the callee is reached
        // otherwise.
        K::CXXOperatorCallExpr => {
            if let Some(d) = callee(ast, e) {
                w.unit.operands.marks.push((n.loc(), d));
            }
        }
        // Stage 6c of the operator join (Kiln §18.13.7): a new- or
        // delete-expression's entry names the function it selected, and
        // its qualified name begins at the expression's location (its
        // `::`, else its keyword; `decide` reads `qualified_lo` for an
        // `Operator` entry).
        K::CXXNewExpr => {
            if let Some(d) = ast.exprs.new_extra(e).operator_new {
                w.unit.operands.marks.push((n.loc(), d));
            }
        }
        K::CXXDeleteExpr => {
            if let Some(d) = ast.exprs.operator_delete(e) {
                w.unit.operands.marks.push((n.loc(), d));
            }
        }
        _ => {}
    }
}

/// The function the callee of operator call `e` (its first child) names,
/// through implicit casts.
fn callee(ast: &Ast, e: ExprId) -> Option<DeclId> {
    let mut x = ast.exprs.children(e, &ast.lists).first().copied()?;
    for _ in 0..8 {
        match ast.exprs.get(x).kind() {
            K::DeclRefExpr => return Some(ast.exprs.decl(x)),
            K::ImplicitCastExpr => x = ast.exprs.children(x, &ast.lists).first().copied()?,
            _ => return None,
        }
    }
    None
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
/// written type (S3). What Kiln does not keep as written (a constrained
/// placeholder, an explicit instantiation's template-id and declarator,
/// a declarator's qualifier, a friend specialisation's declarator) is
/// the ledger's: its names are met or written at site 9 (L3, L4).
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
    match n.kind() {
        DeclKind::Function
        | DeclKind::CXXMethod
        | DeclKind::CXXConstructor
        | DeclKind::CXXDestructor
        | DeclKind::CXXConversion
        | DeclKind::CXXDeductionGuide => {
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
            if ast.decls.var(d).is_some_and(|v| v.spec.placeholder() != VarPlaceholder::None) {
                if counting(w) {
                    w.op_counts.deduced += 1;
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
        // only; the qualifier's names are the ledger's (L3, L4).
        DeclKind::UsingEnum => {}
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
    // A partial specialisation's own head (Layer S's census: Kiln keeps it
    // on `PartialSpecInfo`, not on a template's info).
    if let Some((_, pi)) = ast.partial_spec_info(d) {
        let params = pi.params;
        head(w, params, out, 0);
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
            // A specialisation's declarator is linked to nothing (its
            // function is the specialisation, with substituted types): its
            // names are written by L4's trigger T5 ([`decide`]).
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

/// The type `q` of function declaration `d` as written: its return type,
/// `noexcept(e)` and the types of a dynamic exception specification (not
/// its parameters, read from their declarations, S7); nothing when the type
/// is a typedef's (`F f;`). Read through [`slots`].
#[allow(clippy::match_same_arms, reason = "the parameters apart from what is not written here")]
fn function_type(w: &mut Walk<'_>, d: DeclId, q: QualType, out: &mut Vec<Op>, depth: u32) {
    if depth > MAX_DEPTH {
        return;
    }
    let ast = w.ast;
    slots(&ast.types, q, &mut |s| match s {
        // Sugar around the function type (`Paren`, `Attributed`).
        Slot::Type(x, Part::Inner) => function_type(w, d, x, out, depth + 1),
        Slot::Type(r, Part::Result) => scan(w, r, out, depth + 1),
        Slot::Type(_, Part::Param) => {}
        // A5: `throw(decltype(make(x)))`; S4 as `noexcept(e)` below.
        Slot::Type(t, Part::Thrown) => {
            if w.is_local(d) || !crate::demand::deferrable(w, d) {
                scan(w, t, out, depth + 1);
            }
        }
        Slot::Operand(NOEXCEPT, expr) => {
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
        // The function type is not written here: a `decltype(f)` or a
        // typedef's type names it (`F f;`), and what a name denotes is
        // not read.
        Slot::Operand(..)
        | Slot::Bound(_)
        | Slot::Args(_)
        | Slot::AliasArgs(_)
        | Slot::Nns(_)
        | Slot::Folded(_)
        | Slot::Splice
        | Slot::Unkept(_)
        | Slot::Denotes(_)
        | Slot::Plain(_) => {}
    });
}

/// The operands of a resolved unit `exception_spec` (S4): its
/// `noexcept(e)`, and the types of a dynamic specification.
pub(crate) fn exception_spec(w: &mut Walk<'_>, spec: ExcSpec) {
    if !w.unit.operands.on {
        return;
    }
    let ast = w.ast;
    let mut out = Vec::new();
    slots::exc_spec_slots(&ast.types, spec, &mut |s| match s {
        Slot::Operand(NOEXCEPT, expr) => push_own(w, Op::Expr(NOEXCEPT, expr), &mut out),
        Slot::Type(t, _) => scan(w, t, &mut out, 0),
        _ => {}
    });
    if !out.is_empty() {
        target(w).extend(out);
    }
}

/// Whether `op` is instantiation-dependent: a pattern's operand, which
/// is not written (its canonical form is a profile).
fn is_dependent(w: &Walk<'_>, op: Op) -> bool {
    match op {
        Op::Expr(_, e) => w.ast.exprs.get(e).deps().contains(ExprDeps::INSTANTIATION),
        Op::Alias(_) | Op::Unkept(_) | Op::Name(..) => false,
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
/// (the module documentation): the consumer of [`slots`] (Layer S, WP-156
/// fix round 3), the one recursion over a written type.
pub(crate) fn scan(w: &mut Walk<'_>, q: QualType, out: &mut Vec<Op>, depth: u32) {
    if depth > MAX_DEPTH {
        return;
    }
    let ast = w.ast;
    slots(&ast.types, q, &mut |s| consume(w, Some(q), s, out, depth + 1));
}

/// What [`scan`], [`args`] and [`scan_nns`] do with one slot of a written
/// type `q` (the type the slot is of; `None` for a list's or a
/// qualifier's): descend into what is written, push an operand, walk an
/// instantiation-dependent bound for its holders, mark a folded value.
#[allow(clippy::many_single_char_names, reason = "w walk, q type, s slot, d depth, as scan")]
fn consume(w: &mut Walk<'_>, q: Option<QualType>, s: Slot, out: &mut Vec<Op>, d: u32) {
    match s {
        Slot::Type(x, _) => scan(w, x, out, d),
        Slot::Operand(site, e) => push(w, Op::Expr(site, e), out, d),
        // A bound that is instantiation-dependent: its holders are read,
        // as for any dependent operand. Otherwise the names in it are the
        // ledger's (L3); sites 3 and 5 stay reserved.
        Slot::Bound(ExprOperand::Expr(e)) => {
            if w.ast.exprs.get(e).deps().contains(ExprDeps::INSTANTIATION) {
                dependent(w, e, out, d);
            }
        }
        Slot::Bound(ExprOperand::Profile(_)) | Slot::Denotes(_) | Slot::Plain(_) => {}
        Slot::Args(l) => args(w, l, out, d),
        // B5: an alias specialisation's written arguments, then the
        // operands inside them. Instantiation-dependent: the list is the
        // pattern's own spelling, but an operand in it that is not is read
        // ([`push`] drops the others).
        Slot::AliasArgs(l) => {
            if q.is_some_and(|q| w.ast.types.flags(q.ty()).is_instantiation_dependent()) {
                if counting(w) {
                    w.op_counts.dependent += 1;
                }
            } else {
                out.push(Op::Alias(l));
            }
            args(w, l, out, d);
        }
        Slot::Nns(n) => scan_nns(w, n, out, d),
        // L4's trigger T3 ([`decide`]): a bit-precise integer's width, a
        // vector's count, an address space, as written, are not kept.
        Slot::Folded(_) => w.unit.operands.folded = true,
        Slot::Splice => out.push(Op::Unkept(SPLICE)),
        Slot::Unkept(c) => out.push(Op::Unkept(c)),
    }
}

/// The operands written in template argument list `l` as written: an
/// expression argument is site 4, a type argument is scanned.
fn args(w: &mut Walk<'_>, l: ArgListId, out: &mut Vec<Op>, depth: u32) {
    if depth > MAX_DEPTH {
        return;
    }
    let ast = w.ast;
    slots::arg_slots(&ast.types, l, &mut |s| consume(w, None, s, out, depth + 1));
}

/// The operands written in nested-name-specifier `n`: the types it names.
fn scan_nns(w: &mut Walk<'_>, n: NnsId, out: &mut Vec<Op>, depth: u32) {
    if depth > MAX_DEPTH {
        return;
    }
    let ast = w.ast;
    slots::nns_slots(&ast.types, n, &mut |s| consume(w, None, s, out, depth + 1));
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
#[allow(
    clippy::too_many_lines,
    reason = "one arm per expression kind, exhaustive (WP-156 fix round 1)"
)]
#[allow(
    clippy::many_single_char_names,
    reason = "e expression, n node, k kind, x arena, as own_spelling"
)]
fn holders(w: &mut Walk<'_>, e: ExprId, sink: &mut Vec<Op>, depth: u32) {
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
            // has the deduced type: Kiln keeps no mark of it (L4's trigger
            // T4, [`decide`]).
            w.unit.operands.news = true;
            scan(w, x.new_extra(e).allocated_ty, &mut out, depth);
        }
        K::CompoundLiteralExpr => scan(w, x.compound_literal_written_type(e), &mut out, depth),
        K::CXXPseudoDestructorExpr => {
            // The first type-name of `p->S::F::~Q()` with a dependent `S`
            // (`TemplateTables` `pseudo_dtor_first`), then the destroyed.
            if let Some(&(first, _)) = ast.templates.pseudo_dtor_first.get(e) {
                scan(w, first, &mut out, depth);
            }
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
    // L3 and L5: the written names of the unit (or deferred to this part)
    // that no stream of it walked, as one more group, in table order.
    let names = decide(w);
    if !names.is_empty() {
        let mut gs = Stream::new();
        let mut count = 0usize;
        let mut at = Vec::new();
        for op in names {
            entries += 1;
            if entries > MAX_ENTRIES {
                s.indeterminate(TOO_MANY);
                break;
            }
            let (es, _, _) = entry(w, op);
            count += 1;
            if let Op::Name(_, r) = op {
                at.push((gs.as_bytes().len(), w.ast.written_refs.name_lo(r)));
            }
            gs.raw(es.as_bytes()).absorb(es.cause());
        }
        s.count(count);
        for (ahead, loc) in at {
            s.mark_name(ahead, loc);
        }
        s.raw(gs.as_bytes()).absorb(gs.cause());
        total += count;
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

/// The ledger of the translation unit ([`ledger::build`]), built on first
/// use (`walk::run_kept` builds it before the first unit).
pub(crate) fn ensure_ledger(w: &mut Walk<'_>) {
    if w.ledger.is_none() {
        let l = ledger::build(w);
        w.ledger = Some(l);
    }
}

/// L3 at [`finish`] of the unit being written, whose kept streams are
/// written: decides each entry of the ledger the unit holds (for a part,
/// each its owner deferred to it) and returns, in table order, the site-9
/// operands (L5) of those no stream of it walked.
///
/// - **met**: a mark of the unit has the entry's `name_lo` and entity;
///   nothing is written;
/// - **deferred** (the unit's own `finish`): its `name_lo` is that of a
///   name expression under a root a part of the unit walks (the
///   `noexcept` operand of `exception_spec`, a `default_argument`
///   unit's expression); the part decides it;
/// - **unmet**: written here.
///
/// An entry of a role L3 does not join (a type, template or concept name)
/// is claimed by the unit's own `finish`, and written where one of L4's
/// triggers covers it ([`Triggers`]). In a trigger's range a joined entry
/// is decided as anywhere else: met (its stream walked it, so its entity
/// is written there), deferred, or written.
fn decide(w: &mut Walk<'_>) -> Vec<Op> {
    let st = &w.unit.operands;
    let (Some(e), part, from) = (st.entity, st.part, st.from) else {
        return Vec::new();
    };
    // A part `demand::write_part` did not name (none is begun so), or a
    // part nothing is deferred to.
    if matches!((part, from), (None, None) | (Some(ledger::PartOf::Other), _)) {
        return Vec::new();
    }
    ensure_ledger(w);
    let Some(mut book) = w.ledger.take() else { return Vec::new() };
    let ast = w.ast;
    let refs = &ast.written_refs;
    let mut marks = std::mem::take(&mut w.unit.operands.marks);
    marks.sort_unstable();
    marks.dedup();
    let count = counting(w);
    let holder = |r: &ledger::Row| r.holder.map(|h| ast.decls.get(h).kind());
    let mut out = Vec::new();
    let mut roots: Option<HashMap<Loc, ledger::PartOf>> = None;
    let range = book.of(Some(e));
    let triggers = if part.is_none() {
        Triggers::of(w, &book.rows[range.clone()], e)
    } else {
        Triggers::default()
    };
    for i in range {
        let row = &mut book.rows[i];
        let met = || {
            let at = |l: Loc| marks.binary_search(&(l, row.entity)).is_ok();
            at(refs.name_lo(row.entry))
                || (refs.get(row.entry).role == RefRole::Operator
                    && at(refs.qualified_lo(row.entry)))
        };
        match part {
            None => {
                if matches!(row.claim, Claim::ByPart(_)) {
                    continue;
                }
                if !row.joined {
                    row.claim = Claim::Claimed;
                    if let Some(why) = triggers.covers(row) {
                        if count {
                            w.op_counts.name(why, refs.get(row.entry).role, holder(row));
                        }
                        out.push(Op::Name(row.entity, row.entry));
                    }
                    continue;
                }
                if met() {
                    row.claim = Claim::Claimed;
                    continue;
                }
                let at = roots.get_or_insert_with(|| deferral_roots(w, e, from));
                if let Some(&p) = at.get(&refs.name_lo(row.entry)) {
                    row.claim = Claim::Deferred(p);
                    continue;
                }
                row.claim = Claim::Claimed;
                if count {
                    w.op_counts.name("unmet", refs.get(row.entry).role, holder(row));
                }
                out.push(Op::Name(row.entity, row.entry));
            }
            Some(p) => {
                match row.claim {
                    Claim::Deferred(q) | Claim::ByPart(q) if q == p => {}
                    Claim::Open | Claim::Claimed | Claim::Deferred(_) | Claim::ByPart(_) => {
                        continue;
                    }
                }
                let met = met();
                row.claim = Claim::ByPart(p);
                if !met {
                    if count {
                        w.op_counts.name("unmet", refs.get(row.entry).role, holder(row));
                    }
                    out.push(Op::Name(row.entity, row.entry));
                }
            }
        }
    }
    w.ledger = Some(book);
    w.unit.operands.marks = marks;
    out
}

/// L4: where the entries of a role L3 does not join (a type, template or
/// concept name: types are interned and keep no location) are written,
/// for the unit of entity `e` (its own `finish`). Each trigger is read
/// from the AST and the table, with no location; each replaces a refusal
/// of fix rounds 1 and 2.
///
/// - **T1**: the unit is a namespace's (or `<order>`'s,
///   [`order_names`]): every entry. What a namespace or the translation
///   unit holds directly declares nothing (an explicit instantiation of
///   a function or variable).
/// - **T1b**: a class one of whose members is named by an explicit
///   instantiation of its own: every entry the class itself holds.
/// - **T2**: a declaration whose `Redecl` entry has a qualifier chain
///   with a component that is not a namespace, a class that is no
///   specialisation or an enumeration (a template-id, an alias, a
///   typedef, a `decltype` specifier): from the declaration's first entry
///   through the last entry of the chain (the coordinator's amendment:
///   Kiln records a function definition's `Redecl` after its body's).
/// - **T3**: the unit read a [`Slot::Folded`]: every entry.
/// - **T4**: a declaration whose type a placeholder deduced that holds a
///   `Concept` entry: its entries up to its last `Concept` entry; a unit
///   whose walk met a `new` and that holds a `Concept` entry: every
///   entry. Until the pin moves to Kiln WP-3640.
/// - **T5**: a friend whose form is `Specialization`: every entry it
///   holds (its own unit), and every entry of its holder, the
///   befriending class ([`befriends_specialisation`]).
#[derive(Debug, Default)]
struct Triggers {
    /// T1, T3, T4 for `new`, T5 for the befriending class: every entry
    /// of the unit (the trigger's name, for the counters).
    all: Option<&'static str>,
    /// T1b, T2, T4, T5: the entries of a holder from the first index
    /// through the second (table indexes), and the trigger's name.
    ranges: HashMap<DeclId, (usize, usize, &'static str)>,
}

impl Triggers {
    /// The triggers of the unit of entity `e`, whose rows are `rows`.
    fn of(w: &Walk<'_>, rows: &[ledger::Row], e: DeclId) -> Triggers {
        let ast = w.ast;
        let refs = &ast.written_refs;
        let st = &w.unit.operands;
        let concept = rows.iter().any(|r| refs.get(r.entry).role == RefRole::Concept);
        let all = if ast.decls.get(e).kind() == DeclKind::Namespace {
            Some("t1")
        } else if st.folded {
            Some("t3")
        } else if st.news && concept {
            Some("t4")
        } else if befriends_specialisation(w, e) {
            Some("t5")
        } else {
            None
        };
        let mut ranges = HashMap::new();
        if all.is_some() {
            return Triggers { all, ranges };
        }
        // Per holder: its first entry, its last `Concept` entry, its
        // `Redecl` entry (rows are in table order).
        let mut first: HashMap<DeclId, usize> = HashMap::new();
        let mut concepts: HashMap<DeclId, usize> = HashMap::new();
        let mut redecls: HashMap<DeclId, WrittenRefId> = HashMap::new();
        for r in rows {
            let Some(h) = r.holder else { continue };
            first.entry(h).or_insert(r.entry.index());
            match refs.get(r.entry).role {
                RefRole::Concept => {
                    concepts.insert(h, r.entry.index());
                }
                RefRole::Redecl => {
                    redecls.entry(h).or_insert(r.entry);
                }
                _ => {}
            }
        }
        let mut class_holds = None;
        for (&h, &lo) in &first {
            // T5.
            if friend_specialisation(ast, h) {
                ranges.insert(h, (lo, usize::MAX, "t5"));
                continue;
            }
            // T1b.
            if ast.decls.record(h).is_some() && ast.first(h) == ast.first(e) {
                let explicit = *class_holds.get_or_insert_with(|| {
                    w.redecls(e)
                        .into_iter()
                        .any(|x| ast.members(x).any(|m| own_explicit_instantiation(w, m)))
                });
                if explicit {
                    ranges.insert(h, (lo, usize::MAX, "t1b"));
                    continue;
                }
            }
            let mut hi = None;
            let mut which = "t2";
            // T2.
            if let Some(&r) = redecls.get(&h) {
                hi = qualifier_range(ast, r);
            }
            // T4.
            let deduced =
                ast.decls.var(h).is_some_and(|v| v.spec.placeholder() != VarPlaceholder::None)
                    || ast.decls.function(h).is_some_and(|f| f.spec.return_deduced());
            if deduced && let Some(&c) = concepts.get(&h) {
                if hi.is_none_or(|x| c > x) {
                    which = "t4";
                }
                hi = Some(hi.map_or(c, |x: usize| x.max(c)));
            }
            if let Some(hi) = hi {
                ranges.insert(h, (lo, hi, which));
            }
        }
        Triggers { all, ranges }
    }

    /// The trigger whose range row `r` is in, if any.
    fn covers(&self, r: &ledger::Row) -> Option<&'static str> {
        self.all.or_else(|| {
            r.holder.and_then(|h| self.ranges.get(&h)).and_then(|&(lo, hi, which)| {
                let i = r.entry.index();
                (lo <= i && i <= hi).then_some(which)
            })
        })
    }
}

/// T5 ([`Triggers`]): whether `d` is a friend whose form is
/// `Specialization`.
fn friend_specialisation(ast: &Ast, d: DeclId) -> bool {
    ast.decls.get(d).kind() == DeclKind::Friend
        && matches!(ast.friend_form(d), FriendFormAnswer::Row(v) if v.kind() == FriendFormKind::Specialization)
}

/// T5 ([`Triggers`]), the friend's holder: whether class `e` has a
/// friend whose form is `Specialization`. Kiln gives the parameters
/// written in such a friend's declarator a `ParmVar` whose context is the
/// class, linked to no function (the friend's function has substituted
/// parameters of its own), so their entries are the class unit's.
fn befriends_specialisation(w: &Walk<'_>, e: DeclId) -> bool {
    let ast = w.ast;
    ast.decls.record(e).is_some()
        && w.redecls(e).into_iter().any(|x| ast.members(x).any(|m| friend_specialisation(ast, m)))
}

/// T2 ([`Triggers`]): the index of the last entry of `Redecl` entry
/// `r`'s qualifier chain when a component of it is not a namespace, a
/// class that is no specialisation or an enumeration named directly
/// (a template-id, an alias or typedef, a template parameter, a name
/// with no entity, a `decltype` or splice specifier); `None` otherwise.
fn qualifier_range(ast: &Ast, r: WrittenRefId) -> Option<usize> {
    let refs = &ast.written_refs;
    let mut link = refs.get(r).qualifier.link();
    let mut last: Option<usize> = None;
    let mut fires = false;
    for _ in 0..MAX_DEPTH {
        match link {
            QualLink::None | QualLink::Global => break,
            QualLink::Specifier => {
                fires = true;
                break;
            }
            QualLink::Entry(c) => {
                last = Some(last.map_or(c.index(), |x| x.max(c.index())));
                let x = refs.get(c);
                let plain = x.entity.is_some_and(|d| {
                    matches!(
                        ast.decls.get(d).kind(),
                        DeclKind::Namespace
                            | DeclKind::NamespaceAlias
                            | DeclKind::CXXRecord
                            | DeclKind::Enum
                    )
                });
                fires |= !plain;
                link = x.qualifier.link();
            }
        }
    }
    // A chain of a specifier alone: its operand's entries come before the
    // `Redecl` entry; the range is the declaration's entries up to it.
    fires.then(|| last.unwrap_or(r.index()))
}

/// The locations of the name expressions under the roots the parts of the
/// unit of entity `e`, read from `from`, walk (L3 "deferred"): the
/// `noexcept` operand the resolved unit `exception_spec` writes
/// (`demand::exception_spec_operand`) and the expressions of its
/// `default_argument` units (`demand::default_argument_exprs`), found
/// with `ExprArena::children` from each root.
fn deferral_roots(w: &Walk<'_>, e: DeclId, from: Option<DeclId>) -> HashMap<Loc, ledger::PartOf> {
    let mut at = HashMap::new();
    let Some(from) = from else { return at };
    let ast = w.ast;
    let mut roots = Vec::new();
    if let Some(x) = crate::demand::exception_spec_operand(w, from) {
        roots.push((x, ledger::PartOf::ExceptionSpec));
    }
    for (i, x) in crate::demand::default_argument_exprs(w, e, from) {
        roots.push((x, ledger::PartOf::DefaultArgument(i)));
    }
    for (root, p) in roots {
        let mut stack = vec![root];
        let mut nodes = 0usize;
        while let Some(x) = stack.pop() {
            nodes += 1;
            if nodes > MAX_ENTRIES {
                // Not deferred: written by the unit (a duplicate at most).
                break;
            }
            let n = ast.exprs.get(x);
            if matches!(
                n.kind(),
                K::DeclRefExpr | K::MemberExpr | K::CXXOperatorCallExpr | K::UserDefinedLiteral
            ) {
                at.entry(n.loc()).or_insert(p);
            }
            stack.extend(ast.exprs.children(x, &ast.lists));
        }
    }
    at
}

/// L5 for `<order>`: the entries the translation unit holds, and those of
/// a declaration no unit holds (L2). `<order>`'s stream walks no
/// expression, so each is written: one of a role L3 joins as unmet, any
/// other by L4's trigger T1. Called once the
/// queue is drained (`walk::run_kept`), in a unit state of its own, so
/// that the units the references demand are then written; [`order`]
/// writes the group.
pub(crate) fn order_names(w: &mut Walk<'_>) {
    ensure_ledger(w);
    let Some(mut book) = w.ledger.take() else { return };
    let mut ops = Vec::new();
    // T1 (L4): every entry, of every role.
    let count = counting(w);
    for i in book.of(None) {
        let row = &mut book.rows[i];
        row.claim = Claim::Claimed;
        if count {
            let holder = row.holder.map(|h| w.ast.decls.get(h).kind());
            w.op_counts.name("t1", w.ast.written_refs.get(row.entry).role, holder);
        }
        ops.push(Op::Name(row.entity, row.entry));
    }
    w.ledger = Some(book);
    let saved = w.take_unit();
    let mut stream = Stream::new();
    let mut marks = Vec::new();
    for op in ops {
        let (es, _, _) = entry(w, op);
        if let Op::Name(_, r) = op {
            marks.push((stream.as_bytes().len(), w.ast.written_refs.name_lo(r)));
        }
        stream.raw(es.as_bytes()).absorb(es.cause());
    }
    w.unit = saved;
    w.order_names = Some(Names { count: marks.len(), stream, marks });
}

/// L6: an entry of the ledger no unit decided (its unit was never
/// written). `<order>`'s operand unit is indeterminate with it.
pub(crate) const UNCLAIMED: Cause = Cause::Unaccounted("operands/unclaimed-name");

/// L5's named over-refusal (WP-156 fix round 3, the coordinator's ruling
/// on commit 4's STOP): a site-9 entry names a local entity that the
/// unit's numbering never met, because it is declared inside an
/// expression no stream walks (a local class declared in a lambda inside
/// an array bound). Its entry fails closed; a walked bound, or the
/// lambda's locals numbered in the enclosing function, would make it
/// determinate (KT1 2.9).
pub(crate) const UNWALKED_LOCAL: Cause = Cause::Unsupported("operands/unwalked-local");

/// Writes one entry: its stream, whether it is written (S5), and the
/// operands its walk met (S7).
fn entry(w: &mut Walk<'_>, op: Op) -> (Stream, bool, Vec<Op>) {
    let saved = w.unit.operands.nested.replace(Vec::new());
    w.unit.operands.writing += 1;
    let refs = w.op_refs;
    let marks = w.unit.operands.marks.len();
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
        // L5: the entity, in the unit's state (a parameter or a local has
        // the unit's number); one `write_ref` cannot write is
        // indeterminate.
        //
        // A local that no numbering of the unit met (it is declared inside
        // an expression no stream walks: a local class of a lambda in an
        // array bound) is the named limit `UNWALKED_LOCAL`.
        Op::Name(d, _) => {
            es.uv(u64::from(NAME));
            let mut ns = Stream::new();
            w.write_ref(&mut ns, d);
            if ns.cause() == Some(Cause::UndeclaredLocal) {
                es.indeterminate(UNWALKED_LOCAL);
            } else {
                es.raw(ns.as_bytes()).absorb(ns.cause());
            }
            NAME
        }
    };
    w.unit.operands.writing -= 1;
    let nested = std::mem::replace(&mut w.unit.operands.nested, saved).unwrap_or_default();
    let written = site == ALIAS_ARGS
        || site == UNKEPT
        || site == NAME
        || w.op_refs != refs
        || es.cause().is_some();
    // S5: what the walk of an entry that is not written met was not kept
    // (L3).
    if !written {
        w.unit.operands.marks.truncate(marks);
    }
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
/// written), after the sweep of the ledger (L6): its groups are one
/// [`Op::Unkept`] entry of [`NOT_RECORDED`] when the written-reference
/// table is not on or not whole (L0), the site-9 entries of
/// [`order_names`] (L5), and one [`Op::Unkept`] entry of [`UNCLAIMED`]
/// when an entry of the ledger was decided by no unit (L6); `None` when
/// there is none of these.
pub(crate) fn order(w: &mut Walk<'_>, parent: &Written) -> Option<Written> {
    let refs = &w.ast.written_refs;
    let not_recorded = !(refs.is_on() && refs.unrecorded().is_empty());
    let (unclaimed, undemanded) = w.ledger.as_ref().map_or((0, 0), ledger::Ledger::sweep);
    w.op_counts.unclaimed = unclaimed;
    w.op_counts.undemanded = undemanded;
    let names = w.order_names.take();
    let mut s = Stream::new();
    if order_stream(w, &mut s, not_recorded, names, unclaimed) == 0 {
        return None;
    }
    let h = s.finish(UNIT_TAG);
    let h = w.hashed(h);
    Some(unit_of(w, None, parent, h))
}

/// The second writing of `<order>`'s operand unit (the trace, §6.1): its
/// site-9 entries again ([`order_names`] claims what it claimed), into a
/// traced stream.
pub(crate) fn order_again(w: &mut Walk<'_>) -> crate::stream::Recording {
    let refs = &w.ast.written_refs;
    let not_recorded = !(refs.is_on() && refs.unrecorded().is_empty());
    order_names(w);
    let (unclaimed, _) = w.ledger.as_ref().map_or((0, 0), ledger::Ledger::sweep);
    let names = w.order_names.take();
    let mut s = Stream::traced();
    order_stream(w, &mut s, not_recorded, names, unclaimed);
    s.into_recording()
}

/// The groups of `<order>`'s operand unit ([`order`]); how many entries.
fn order_stream(
    w: &mut Walk<'_>,
    s: &mut Stream,
    not_recorded: bool,
    names: Option<Names>,
    unclaimed: u64,
) -> usize {
    let mut total = 0usize;
    if not_recorded {
        let (es, _, _) = entry(w, Op::Unkept(NOT_RECORDED));
        s.count(1);
        s.raw(es.as_bytes()).absorb(es.cause());
        total += 1;
    }
    if let Some(n) = names
        && n.count > 0
    {
        s.count(n.count);
        for (ahead, loc) in n.marks {
            s.mark_name(ahead, loc);
        }
        s.raw(n.stream.as_bytes()).absorb(n.stream.cause());
        total += n.count;
    }
    if unclaimed > 0 {
        let (es, _, _) = entry(w, Op::Unkept(UNCLAIMED));
        s.count(1);
        s.raw(es.as_bytes()).absorb(es.cause());
        total += 1;
    }
    total
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
    for site in [DECLTYPE, TYPEOF, EXPR_ARG, NOEXCEPT, ALIAS_ARGS, NAME] {
        push(&format!("site.{site}"), c.sites[usize::from(site)]);
    }
    push("unreferenced", c.unreferenced);
    push("dependent", c.dependent);
    push("deduced", c.deduced);
    push("unkept", c.unkept);
    push("names.undemanded", c.undemanded);
    for (k, &v) in &c.names {
        push(&format!("names.{k}"), v);
    }
    // Always (L6): 0 on both sides of every fixture (`tests/common`).
    out.push(slip_model::Counter {
        name: "digest.operands.names.unclaimed".to_owned(),
        value: c.unclaimed,
    });
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
