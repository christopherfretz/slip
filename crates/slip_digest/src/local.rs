//! Local entities (`spec/digest.md` §4.1, §4.3, `spec/digest-kinds.md`
//! §2.4, §10.4): the ordinals of local objects, the declarations walked in
//! place, local types and their owners, the numbering of an owner, the
//! units under a local type and `Ref::outer`.
//!
//! # Numbering an owner
//!
//! The number `n` of a local type's key `local(n)` is its position among
//! the local types its owner's stream declares. [`numbering`] finds it by
//! walking the owner's stream (`Walk::decl_stream` from the declaration
//! `units::write` reads) in **numbering mode** ([`numbering_mode`]): the
//! walk is the ordinary one, statement and expression walk included, but
//! `Walk::key`, `Walk::type_digest`, `Walk::write_ref` and `value::value`
//! return at once without reading or memoising anything, the stream is a
//! sink, and [`write_decl`] records instead of numbering: a local type or
//! local object whose owner is the one being numbered is appended to the
//! record, and the members of a local type are not entered. So the
//! numbering meets declarations in the order the walk does because it is
//! the walk: nothing the mode skips decides which child comes next (the
//! one order by keys, `members-other` of a local class, orders members,
//! whose streams the numbering does not enter).
//!
//! A numbering runs with the walk's depth counters at zero and puts them
//! back after: its result is a function of the owner alone, memoised once
//! per walk. A numbering never starts another (nothing that numbers runs
//! in the mode), so the stack holds at most one numbering on top of the
//! walk that asked for it (`lib.rs` "What bounds the stack"). A numbering
//! that reaches a depth limit is **cut**: every number asked of it is
//! [`TOO_DEEP`].

use std::collections::{HashMap, HashSet};

use slip_kiln::kiln::ast::decl::{DeclFlags, DeclKind};
use slip_kiln::kiln::ast::lambda::{LambdaCapture, LambdaData, LambdaMangling};
use slip_kiln::kiln::ast::stmt::StmtKind;
use slip_kiln::kiln::ast::types::{ArgView, TypeView};
use slip_kiln::kiln::ast::{DeclId, ExprId, LambdaId, QualType, StmtId};
use slip_model::Digest;
use slip_model::keys::Parent;

use crate::Cause;
use crate::keys::{KeyRes, Role, info};
use crate::stream::{FlagWord, Stream};
use crate::tables::{capture_default, capture_kind};
use crate::walk::{TOO_DEEP, Walk};

/// A closure whose owner is a namespace, a class or the translation
/// unit: no stream numbers it (digest-kinds §10.4).
pub(crate) const CLOSURE_OWNER: Cause = Cause::Unsupported("local/closure-owner");
/// A declaration `Ast::closure_owner` answers, or a scope a local entity
/// is declared in, that §10.4 does not name.
pub(crate) const BAD_OWNER: Cause = Cause::Unaccounted("local/owner");
/// A local type the walk declares that its owner's numbering lacks or
/// has at another position.
pub(crate) const NUMBERING: Cause = Cause::Unaccounted("local/numbering");
/// A lambda-expression walked inside a key (digest §4.3).
pub(crate) const LAMBDA_IN_KEY: Cause = Cause::Unsupported("local/lambda-in-key");
/// A lambda-expression walked inside a type digest (digest §4.3).
pub(crate) const LAMBDA_IN_TYPE: Cause = Cause::Unsupported("local/lambda-in-type");
/// An entity a requires-expression's parameter scope declares: the
/// profile package's (WP-036), not an owner's.
pub(crate) const REQUIRES_SCOPE: Cause = Cause::Unsupported("local/requires-scope");
/// A numbering asked for after [`number_all`] (never, the pair tests
/// check).
pub(crate) const LATE: Cause = Cause::Unaccounted("local/late-numbering");
/// A numbering asked for while it is being computed: its owner's stream
/// source needs its own numbering.
pub(crate) const CYCLE: Cause = Cause::Unaccounted("local/numbering-cycle");
/// What a number asked for while numbering is (never: the mode returns
/// before anything asks).
pub(crate) const IN_NUMBERING: Cause = Cause::Unaccounted("local/in-numbering");
/// The parent a content-keyed owner's local types have inside the
/// owner's own content digest (digest §4.3): that owner, whose key is
/// the digest being computed.
const SELF_KEY: Digest = Digest(*b"slip/digest/local/content-self!!");
/// The exception specification of a local class's member that Kiln
/// still holds `Deferred` (an implicit or defaulted member never used):
/// no resolved unit can hold it, since the member's key is under a local
/// type, and Kiln has not computed it (`decl.rs` `own_exception_spec`).
pub(crate) const DEFERRED_EXCEPTION_SPEC: Cause =
    Cause::Unsupported("local/deferred-exception-spec");
/// How far a chain of contexts or owners is followed.
const CHAIN_LIMIT: u32 = 4096;

/// What numbering an owner found (digest §4.3 "Numbering an owner"): its
/// local types and its local objects, each in order.
#[derive(Debug, Default)]
pub(crate) struct Numbering {
    /// The local types, by declaration (`Ast::first`).
    pub types: Vec<DeclId>,
    type_index: HashMap<DeclId, u64>,
    /// The local objects in the order of `Ref::outer`'s index: the
    /// variables an init-capture declares, then the parameters, then the
    /// others in the order the numbering meets their declarations.
    pub objects: Vec<DeclId>,
    object_index: HashMap<DeclId, u64>,
    /// No number is read from it: a depth limit was reached
    /// ([`TOO_DEEP`]), or it is the empty numbering of an owner that could
    /// not be numbered (a cycle, or one asked for after [`number_all`]:
    /// [`LATE`]).
    pub cut: Option<Cause>,
}

impl Numbering {
    /// The position of local type `t`.
    pub(crate) fn type_of(&self, t: DeclId) -> Result<u64, Cause> {
        if let Some(c) = self.cut {
            return Err(c);
        }
        self.type_index.get(&t).copied().ok_or(Cause::UndeclaredLocal)
    }

    /// The index of local object `d` (`Ref::outer`).
    pub(crate) fn object_of(&self, d: DeclId) -> Result<u64, Cause> {
        if let Some(c) = self.cut {
            return Err(c);
        }
        self.object_index.get(&d).copied().ok_or(Cause::UndeclaredLocal)
    }
}

/// The record a numbering builds while it walks; its owner is the mode.
#[derive(Debug, Default)]
struct Record {
    owner: Option<DeclId>,
    types: Vec<DeclId>,
    seen: HashSet<DeclId>,
    params: Vec<DeclId>,
    others: Vec<DeclId>,
    objects_seen: HashSet<DeclId>,
}

/// The walk's state for local entities: the memo of numberings and what
/// the numbering under way records.
#[derive(Debug, Default)]
pub(crate) struct LocalState {
    record: Record,
    numberings: HashMap<DeclId, Numbering>,
    /// Owners whose numbering is being computed (an owner whose stream
    /// source needs a key that needs its own numbering).
    in_progress: HashSet<DeclId>,
    /// What [`numbering`] answers when it cannot number: cut.
    empty: Numbering,
    /// How many owners were numbered (computed, not read from the memo).
    pub numbered: u64,
    /// [`number_all`] has run: no numbering starts inside a unit's walk.
    sealed: bool,
    /// Numberings asked for after [`number_all`] (none: a test checks).
    pub late: u64,
    /// How many type digests are being written ([`LAMBDA_IN_TYPE`]).
    pub in_type: u32,
    /// The template of each template parameter, built on first use.
    param_templates: Option<HashMap<DeclId, DeclId>>,
    /// How many keys were computed under [`SELF_KEY`]: a key, type or
    /// value computed while it grows is not memoised for other units.
    pub self_rooted: u64,
    /// The consteval block at each location, built with
    /// `param_templates`: `None` for a location two blocks share.
    blocks: HashMap<slip_kiln::kiln::source::Loc, Option<DeclId>>,
    /// [`unit_of`], memoised.
    units: HashMap<DeclId, Option<DeclId>>,
    /// Tests only: each local type the walk declared in place, with its
    /// owner, in the order written.
    #[cfg(test)]
    pub written: Vec<(DeclId, DeclId)>,
}

/// The local state of the stream being written, put aside with the unit
/// (`Walk::take_unit`).
#[derive(Debug, Default)]
pub(crate) struct UnitLocals {
    /// The unit whose stream this is (`Ref::outer`): `None` for a stream
    /// that is no unit's own (a content digest).
    pub holder: Option<DeclId>,
    /// The local types this stream declared: met again, `n` alone.
    written: HashSet<DeclId>,
    /// Per owner, the number the next local type it declares must have.
    next: HashMap<DeclId, u64>,
    /// The named local types whose streams are being written in place,
    /// outermost first ([`write_scope_names`]).
    scopes: Vec<DeclId>,
}

/// The numbering mode is on: what the mode skips returns at once.
pub(crate) fn numbering_mode(w: &Walk<'_>) -> bool {
    w.local.record.owner.is_some()
}

/// `d` is a closure type.
fn is_closure(w: &Walk<'_>, d: DeclId) -> bool {
    w.ast.decls.record(d).is_some_and(|r| r.lambda.is_some())
}

/// `d` is a local type (digest §4.3): a closure type, or a class or
/// enumeration declared in a scope whose declarations are local (a
/// function body; Kiln gives a local class the function as its context
/// whatever block declares it). A class nested in a local class is a
/// member of it, not a local type.
pub(crate) fn is_local_type(w: &Walk<'_>, d: DeclId) -> bool {
    let ast = w.ast;
    if is_closure(w, d) {
        return true;
    }
    let n = ast.decls.get(d);
    let kind = n.kind();
    if kind != DeclKind::CXXRecord && kind != DeclKind::Enum {
        return false;
    }
    n.ctx().is_some_and(|c| info(ast.decls.get(c).kind()).local_scope)
}

/// The owner of the local entity `d` (digest §4.3): the entity with a
/// key whose own stream holds its declaration. `None` when no stream
/// numbers it (digest-kinds §10.4).
pub(crate) fn owner(w: &Walk<'_>, d: DeclId) -> Option<DeclId> {
    owner_res(w, d).ok()
}

/// [`owner`], with the cause when there is none.
pub(crate) fn owner_res(w: &Walk<'_>, d: DeclId) -> Result<DeclId, Cause> {
    let ast = w.ast;
    if is_closure(w, d) {
        let r = ast.first(d);
        let Some(o) = ast.closure_owner(r) else { return Err(BAD_OWNER) };
        return closure_owner(w, r, o);
    }
    let Some(c) = ast.decls.get(d).ctx() else { return Err(BAD_OWNER) };
    scope_owner(w, c)
}

/// The owner of closure `r` whose `Ast::closure_owner` is `o`, taken to
/// the entity with a key (digest-kinds §10.4): a parameter's function, a
/// template parameter's template, and otherwise the declaration's entity
/// (the template of a pattern).
#[allow(clippy::match_same_arms, reason = "one arm per rule of §10.4")]
fn closure_owner(w: &Walk<'_>, r: DeclId, o: DeclId) -> Result<DeclId, Cause> {
    let ast = w.ast;
    let kind = ast.decls.get(o).kind();
    // A consteval block's lambda: owned by the block, a unit of its own.
    if let Some(b) = consteval_block(w, r, o) {
        return Ok(b);
    }
    match kind {
        DeclKind::ParmVar => match ast.decls.get(o).ctx() {
            Some(f) if info(ast.decls.get(f).kind()).function => Ok(w.entity(f)),
            Some(_) | None => Err(BAD_OWNER),
        },
        DeclKind::TemplateTypeParm
        | DeclKind::NonTypeTemplateParm
        | DeclKind::TemplateTemplateParm => {
            param_template(w, o).map(|t| w.entity(t)).ok_or(CLOSURE_OWNER)
        }
        DeclKind::Field
        | DeclKind::Var
        | DeclKind::VarTemplateSpecialization
        | DeclKind::VarTemplatePartialSpecialization
        | DeclKind::TypeAlias
        | DeclKind::Typedef
        | DeclKind::Function
        | DeclKind::CXXMethod
        | DeclKind::CXXConstructor
        | DeclKind::CXXDestructor
        | DeclKind::CXXConversion
        | DeclKind::CXXDeductionGuide
        | DeclKind::Concept
        | DeclKind::FunctionTemplate
        | DeclKind::VarTemplate
        | DeclKind::TypeAliasTemplate
        | DeclKind::ClassTemplate => Ok(w.entity(o)),
        // A class is an owner only as the pattern a template head's
        // requires-clause belongs to (Kiln sets it); as the closure's
        // context it has no stream that numbers the closure.
        DeclKind::CXXRecord
        | DeclKind::ClassTemplateSpecialization
        | DeclKind::ClassTemplatePartialSpecialization => {
            let templated = ast.described_template(o).is_some()
                || kind == DeclKind::ClassTemplatePartialSpecialization;
            if templated && o != ast.closure_owner_default(r) {
                Ok(w.entity(o))
            } else {
                Err(CLOSURE_OWNER)
            }
        }
        DeclKind::TranslationUnit
        | DeclKind::Namespace
        | DeclKind::LinkageSpec
        | DeclKind::Export
        | DeclKind::Enum => Err(CLOSURE_OWNER),
        DeclKind::EnumConstant
        | DeclKind::IndirectField
        | DeclKind::Binding
        | DeclKind::Label
        | DeclKind::ContractResultName
        | DeclKind::StaticAssert
        | DeclKind::ConstevalBlock
        | DeclKind::LifetimeExtendedTemporary
        | DeclKind::Friend
        | DeclKind::AccessSpec
        | DeclKind::Empty
        | DeclKind::FileScopeAsm
        | DeclKind::NamespaceAlias
        | DeclKind::Using
        | DeclKind::UsingShadow
        | DeclKind::ConstructorUsingShadow
        | DeclKind::UnresolvedUsingValue
        | DeclKind::UnresolvedUsingTypename
        | DeclKind::UnresolvedUsingIfExists
        | DeclKind::UsingEnum
        | DeclKind::UsingDirective
        | DeclKind::UsingPack
        | DeclKind::FriendTemplate
        | DeclKind::ImplicitParam
        | DeclKind::ImplicitConceptSpecialization
        | DeclKind::BuiltinTemplate
        | DeclKind::RequiresExprBody
        | DeclKind::TemplateParamObject => Err(BAD_OWNER),
    }
}

/// The owner of what scope `c` declares (a local class or enumeration, a
/// local object): a function's entity (the call operator for a lambda's
/// body, the template of a pattern), or the variable, field or concept
/// whose initialiser holds it.
#[allow(clippy::match_same_arms, reason = "one arm per kind of scope")]
fn scope_owner(w: &Walk<'_>, c: DeclId) -> Result<DeclId, Cause> {
    let ast = w.ast;
    match ast.decls.get(c).kind() {
        DeclKind::Function
        | DeclKind::CXXMethod
        | DeclKind::CXXConstructor
        | DeclKind::CXXDestructor
        | DeclKind::CXXConversion
        | DeclKind::CXXDeductionGuide
        | DeclKind::Var
        | DeclKind::VarTemplateSpecialization
        | DeclKind::VarTemplatePartialSpecialization
        | DeclKind::Field
        | DeclKind::Concept => Ok(w.entity(c)),
        DeclKind::RequiresExprBody => Err(REQUIRES_SCOPE),
        DeclKind::TranslationUnit
        | DeclKind::Namespace
        | DeclKind::LinkageSpec
        | DeclKind::Export
        | DeclKind::CXXRecord
        | DeclKind::ClassTemplateSpecialization
        | DeclKind::ClassTemplatePartialSpecialization
        | DeclKind::Enum
        | DeclKind::EnumConstant
        | DeclKind::IndirectField
        | DeclKind::ParmVar
        | DeclKind::Binding
        | DeclKind::Label
        | DeclKind::ContractResultName
        | DeclKind::StaticAssert
        | DeclKind::ConstevalBlock
        | DeclKind::LifetimeExtendedTemporary
        | DeclKind::Friend
        | DeclKind::AccessSpec
        | DeclKind::Empty
        | DeclKind::FileScopeAsm
        | DeclKind::NamespaceAlias
        | DeclKind::Using
        | DeclKind::UsingShadow
        | DeclKind::ConstructorUsingShadow
        | DeclKind::UnresolvedUsingValue
        | DeclKind::UnresolvedUsingTypename
        | DeclKind::UnresolvedUsingIfExists
        | DeclKind::UsingEnum
        | DeclKind::UsingDirective
        | DeclKind::UsingPack
        | DeclKind::FriendTemplate
        | DeclKind::ImplicitParam
        | DeclKind::ImplicitConceptSpecialization
        | DeclKind::BuiltinTemplate
        | DeclKind::TemplateParamObject
        | DeclKind::Typedef
        | DeclKind::TypeAlias
        | DeclKind::FunctionTemplate
        | DeclKind::ClassTemplate
        | DeclKind::VarTemplate
        | DeclKind::TypeAliasTemplate
        | DeclKind::TemplateTypeParm
        | DeclKind::NonTypeTemplateParm
        | DeclKind::TemplateTemplateParm => Err(BAD_OWNER),
    }
}

/// The template whose head declares template parameter `p` (Kiln gives a
/// template parameter the template's context, not the template), from
/// the map [`param_templates`] builds.
pub(crate) fn param_template(w: &Walk<'_>, p: DeclId) -> Option<DeclId> {
    w.local.param_templates.as_ref().and_then(|m| m.get(&p).copied())
}

/// Builds the map [`param_template`] reads, once per walk, when the
/// first closure owned by a template parameter needs it.
pub(crate) fn param_templates(w: &mut Walk<'_>) {
    if w.local.param_templates.is_some() {
        return;
    }
    let ast = w.ast;
    let mut m = HashMap::new();
    let mut blocks = HashMap::new();
    for d in ast.decls.ids() {
        if ast.decls.get(d).kind() == DeclKind::ConstevalBlock {
            blocks
                .entry(ast.decls.get(d).loc())
                .and_modify(|b: &mut Option<DeclId>| *b = None)
                .or_insert(Some(d));
        }
        let list = ast
            .template_info(d)
            .map(|i| i.params)
            .or_else(|| ast.partial_spec_info(d).map(|(_, i)| i.params));
        if let Some(l) = list {
            for x in ast.lists.iter::<DeclId>(ast.templates.param_list(l).params) {
                m.entry(x).or_insert(d);
            }
        }
    }
    w.local.param_templates = Some(m);
    w.local.blocks = blocks;
}

/// The consteval block whose synthesized lambda ([dcl.pre], P3289R1)
/// closure `r` is, when Kiln gives it the block's scope as its owner:
/// both of the lambda's brackets are at the block's `consteval`
/// (`Sema::consteval_block_closure`), and the block is in scope `o`.
#[allow(
    clippy::many_single_char_names,
    reason = "the names of digest-kinds: s stream, d declaration, e entity, n number, o owner, t type"
)]
fn consteval_block(w: &Walk<'_>, r: DeclId, o: DeclId) -> Option<DeclId> {
    let ast = w.ast;
    let l = ast.decls.record(r)?.lambda?;
    let data = ast.exprs.lambdas.get(l);
    if !data.lbracket.is_valid() || data.lbracket != data.rbracket {
        return None;
    }
    let b = (*w.local.blocks.get(&data.lbracket)?)?;
    (ast.decls.get(b).ctx() == Some(o) && !w.is_local(b)).then_some(b)
}

/// [`owner_res`], with what it needs built first.
fn owner_of(w: &mut Walk<'_>, d: DeclId) -> Result<DeclId, Cause> {
    if w.local.param_templates.is_none() && is_closure(w, d) {
        param_templates(w);
    }
    owner_res(w, d)
}

/// The parent of local type `e`'s key: its owner's key (digest §4.3).
pub(crate) fn local_parent(
    w: &mut Walk<'_>,
    e: DeclId,
) -> Result<(Parent, Option<Cause>), Option<Cause>> {
    let o = owner_of(w, e).map_err(Some)?;
    if w.content_in_progress.contains(&o) {
        // Inside the owner's own content digest, which is its key (a
        // consteval block's closure): the owner is `SELF_KEY`, which
        // means the unit being digested; what depends on it is not
        // memoised (`LocalState::self_rooted`).
        w.local.self_rooted += 1;
        return Ok((Parent::Unit(SELF_KEY), None));
    }
    match w.key(o) {
        KeyRes::Key { id, cause, .. } => Ok((Parent::Unit(id), cause)),
        KeyRes::None(c) => Err(Some(c.unwrap_or(BAD_OWNER))),
    }
}

/// What numbering `owner` found: its local types and its local objects,
/// each in order. Memoised in the walk.
pub(crate) fn numbering<'a>(w: &'a mut Walk<'_>, owner: DeclId) -> &'a Numbering {
    if !w.local.numberings.contains_key(&owner) {
        if w.local.sealed {
            // Not among the owners `number_all` numbered: never numbered
            // inside a unit's walk, where the stack holds that walk.
            w.local.late += 1;
            w.local.empty.cut = Some(LATE);
            return &w.local.empty;
        }
        if numbering_mode(w) || !w.local.in_progress.insert(owner) {
            // Never while numbering; a cycle through the owner's stream
            // source: nothing to number from.
            w.local.empty.cut = Some(CYCLE);
            return &w.local.empty;
        }
        let n = compute(w, owner);
        w.local.in_progress.remove(&owner);
        w.local.numberings.insert(owner, n);
    }
    &w.local.numberings[&owner]
}

/// Numbers, before any unit is written and outside every walk, each owner
/// a unit's walk can ask a number of: the owner of every local type, and
/// the call operator of every closure (whose init-capture variables a
/// specialisation's body names by `Ref::outer`). Then seals the memo: a
/// numbering is never started inside a unit's walk, so the stack holds
/// one walk's budget at most (`lib.rs` "What bounds the stack"). A
/// translation unit without local types numbers nothing.
pub(crate) fn number_all(w: &mut Walk<'_>) {
    let ast = w.ast;
    for d in ast.decls.ids() {
        if ast.first(d) != d || !is_local_type(w, d) {
            continue;
        }
        if let Ok(o) = owner_of(w, d) {
            let _ = numbering(w, o);
        }
        let op = ast
            .decls
            .record(d)
            .and_then(|r| r.lambda)
            .and_then(|l| ast.exprs.lambdas.get(l).call_op);
        if let Some(op) = op {
            let e = w.entity(op);
            let _ = numbering(w, e);
        }
    }
    // The owner of the variable of each expanded expansion statement:
    // Kiln keeps the statement's pattern parts in an instantiated body
    // (`ExpansionData`), so the resolved unit `definition` of a function
    // template's specialisation names its pattern's variable by
    // `Ref::outer`, the pattern's template being its owner.
    for i in 1..=u32::try_from(ast.stmts.len()).unwrap_or(0) {
        let st = StmtId::new(i);
        if ast.stmts.get(st).kind() != StmtKind::CXXExpansionStmt
            || !ast.stmts.expansion_expanded(st)
        {
            continue;
        }
        let v = ast.stmts.expansion_data(st).decl;
        if let Ok(o) = owner_of(w, ast.first(v)) {
            let _ = numbering(w, o);
        }
    }
    w.local.sealed = true;
}

/// Numbers `owner`: walks its stream in numbering mode.
fn compute(w: &mut Walk<'_>, owner: DeclId) -> Numbering {
    let ast = w.ast;
    w.local.numbered += 1;
    // The declaration the owner's stream is read from, as `units::write`
    // and `member` read it: found before the mode starts, since it may
    // need keys.
    let from = stream_source(w, owner);
    // The variables the owner's init-captures declare, if it is a call
    // operator: first in `Ref::outer`'s order (digest §4.3).
    let pattern = ast.template_info(owner).and_then(|i| i.pattern).unwrap_or(owner);
    let mut inits: Vec<DeclId> = Vec::new();
    if let Some(l) = ast.call_op_lambda(pattern) {
        for c in &ast.exprs.lambdas.get(l).captures {
            if let Some(v) = c.var
                && c.init
                && owner_res(w, v) == Ok(owner)
            {
                inits.push(ast.first(v));
            }
        }
    }
    let saved = w.take_unit();
    let (depth, node_depth, in_key, in_type) = (w.depth, w.node_depth, w.in_key, w.local.in_type);
    let (aborts, bytes) = (w.aborts, w.bytes);
    w.depth = 0;
    w.node_depth = 0;
    w.in_key = 0;
    w.local.in_type = 0;
    w.local.record = Record { owner: Some(owner), ..Record::default() };
    let mut sink = Stream::new();
    w.decl_stream(&mut sink, from, owner);
    // Then the streams of its units that hold what the own stream leaves
    // out (`demand::number_parts`).
    crate::demand::number_parts(w, &mut sink, owner);
    let record = std::mem::take(&mut w.local.record);
    let cut = (w.aborts != aborts).then_some(TOO_DEEP);
    w.unit = saved;
    w.depth = depth;
    w.node_depth = node_depth;
    w.in_key = in_key;
    w.local.in_type = in_type;
    // A numbering is a function of its owner: its own aborts do not stop
    // the memo of what asked for it, and it writes no bytes.
    w.aborts = aborts;
    w.bytes = bytes;
    let mut n = Numbering { cut, ..Numbering::default() };
    for (i, &t) in record.types.iter().enumerate() {
        n.type_index.insert(t, i as u64);
    }
    n.types = record.types;
    for d in inits.into_iter().chain(record.params).chain(record.others) {
        if let std::collections::hash_map::Entry::Vacant(v) = n.object_index.entry(d) {
            v.insert(n.objects.len() as u64);
            n.objects.push(d);
        }
    }
    n
}

/// The declaration the stream of entity `e` is read from, as
/// `units::write` reads a unit's: the canonical declaration of an entity
/// declared only by instantiations outside their classes, else
/// `units::read_decl`.
pub(crate) fn stream_source(w: &mut Walk<'_>, e: DeclId) -> DeclId {
    if crate::units::outside_only(w, e) {
        crate::units::canonical_decl(w, e)
    } else {
        crate::units::read_decl(w, e)
    }
}

/// `local(n)` of the local type `t`; `Err` with the cause when it has
/// no number (no owner, or its owner's numbering does not find it).
pub(crate) fn type_number(w: &mut Walk<'_>, t: DeclId) -> Result<u64, Cause> {
    if numbering_mode(w) {
        return Err(IN_NUMBERING);
    }
    let t = w.ast.first(t);
    let o = owner_of(w, t)?;
    numbering(w, o).type_of(t)
}

/// The number of local type `t` if its owner is numbered already (for
/// names: reads the memo, numbers nothing).
pub(crate) fn known_number(w: &Walk<'_>, t: DeclId) -> Option<(DeclId, u64)> {
    let t = w.ast.first(t);
    let o = owner_res(w, t).ok()?;
    let n = w.local.numberings.get(&o)?.type_of(t).ok()?;
    Some((o, n))
}

/// The unit whose stream holds the declaration of the local entity `d`:
/// its owner, or the owner's own unit when the owner is written in
/// place.
pub(crate) fn unit_of(w: &mut Walk<'_>, d: DeclId) -> Option<DeclId> {
    let d = w.ast.first(d);
    if let Some(u) = w.local.units.get(&d) {
        return *u;
    }
    let u = find_unit(w, d);
    w.local.units.insert(d, u);
    u
}

fn find_unit(w: &mut Walk<'_>, d: DeclId) -> Option<DeclId> {
    let ast = w.ast;
    let mut x = owner_of(w, d).ok()?;
    for _ in 0..CHAIN_LIMIT {
        if crate::units::unit_class(w, x).is_some() {
            return Some(x);
        }
        // Written in place: a member of a local type, or of a class
        // nested in one.
        let c = w.entity(ast.decls.get(x).ctx()?);
        if is_local_type(w, c) {
            x = owner_of(w, c).ok()?;
        } else if ast.decls.record(c).is_some() && w.is_local(c) {
            x = c;
        } else {
            return None;
        }
    }
    None
}

/// `e` is an entity under a local type that is a derived unit all the
/// same (digest §4.3): a specialisation of a member template of a local
/// type (a generic lambda's call operator, conversion function or
/// invoker), or an implicit member of a local class (a special member,
/// a closure's conversion function or invoker), which `units::unit_class`
/// makes a unit once defined or used. A closure's capture fields are not
/// units.
pub(crate) fn derived_under_local(w: &Walk<'_>, e: DeclId) -> bool {
    let ast = w.ast;
    let n = ast.decls.get(e);
    let kind = n.kind();
    if kind == DeclKind::Field || kind == DeclKind::IndirectField {
        return false;
    }
    let Some(c) = n.ctx() else { return false };
    if ast.decls.record(c).is_none() || !w.is_local(c) {
        return false;
    }
    ast.spec_info(e).is_some()
        || w.redecls(e).iter().all(|&x| ast.decls.get(x).has(DeclFlags::IMPLICIT))
}

/// The ordinal of a local entity of the unit being written: the next
/// one when the walk first meets it, by its declaration or by an earlier
/// reference (a `goto` before its label).
pub(crate) fn ordinal(w: &mut Walk<'_>, d: DeclId) -> u64 {
    let d = w.ast.first(d);
    if let Some(&n) = w.unit.locals.get(&d) {
        return n;
    }
    let n = w.unit.next_local;
    w.unit.next_local += 1;
    w.unit.locals.insert(d, n);
    n
}

/// What `Ref::outer` writes: the owner's key, the index, the key's cause.
pub(crate) type Outer = (Digest, u64, Option<Cause>);

/// What a reference to the local object `d` is in the stream being
/// written: `None` for `Ref::local` (the stream is its unit's, or no
/// unit's), else `Ref::outer`'s owner key, index and the key's cause
/// (digest §4.3 "Local objects of another unit").
pub(crate) fn outer(w: &mut Walk<'_>, d: DeclId) -> Option<Result<Outer, Cause>> {
    let holder = w.unit.local.holder?;
    let u = unit_of(w, d)?;
    if u == w.entity(holder) {
        return None;
    }
    let d = w.ast.first(d);
    let r = owner_of(w, d).and_then(|o| {
        let index = match direct_param(w, o, d) {
            Some(i) if w.local.sealed && !w.local.numberings.contains_key(&o) => i,
            Some(_) | None => numbering(w, o).object_of(d)?,
        };
        match w.key(o) {
            KeyRes::Key { id, cause, .. } => Ok((id, index, cause)),
            KeyRes::None(c) => Err(c.unwrap_or(BAD_OWNER)),
        }
    });
    Some(r)
}

/// The index among `o`'s local objects of `d` when `d` is a parameter of
/// `o`'s own prototype and `o` is not a call operator: its position, since
/// no init-capture variable comes before it and the prototype is walked
/// before anything else that declares an object (digest §4.3). What a
/// specialisation's unit names of its pattern (a parameter, in an
/// expansion statement's pattern) without its owner numbered by
/// [`number_all`]: an owner without local types.
#[allow(
    clippy::many_single_char_names,
    reason = "the names of digest-kinds: s stream, d declaration, e entity, n number, o owner, t type"
)]
fn direct_param(w: &Walk<'_>, o: DeclId, d: DeclId) -> Option<u64> {
    let ast = w.ast;
    if ast.decls.get(d).kind() != DeclKind::ParmVar {
        return None;
    }
    let p = ast.decls.param(d)?;
    let f = ast.decls.get(d).ctx()?;
    if p.scope_depth() != 0
        || !info(ast.decls.get(f).kind()).function
        || ast.call_op_lambda(f).is_some()
        || scope_owner(w, f) != Ok(o)
    {
        return None;
    }
    Some(u64::from(p.index))
}

/// `decl`: a declaration met inside a unit, walked in place (§2.4).
///
/// A variable, parameter, binding or label writes its ordinal and then
/// its stream (§5.1); a block-scope variable with linkage a `ref`, and a
/// block-scope function a `ref` and what its parameters hold; a
/// `static_assert`, a `consteval` block, a template parameter and a
/// lifetime-extended temporary their stream, with no ordinal (§9, §5.2
/// "parent"); a typedef or alias its stream, with no ordinal; a
/// using-declaration, using-directive or empty declaration nothing. A local class, enumeration or closure type
/// its number and its stream ([`local_type`]); a member of one its key
/// and its stream ([`member`]). A kind §2.4 does not name is
/// `Unaccounted`.
#[allow(clippy::match_same_arms, reason = "one arm per rule of §2.4")]
pub(crate) fn write_decl(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    let ast = w.ast;
    let kind = ast.decls.get(d).kind();
    match kind {
        DeclKind::Var if w.is_local_extern(d) => w.write_ref(s, d),
        DeclKind::Function => {
            w.write_ref(s, d);
            block_defaults(w, s, d);
        }
        DeclKind::Var
        | DeclKind::ParmVar
        | DeclKind::Binding
        | DeclKind::Label
        | DeclKind::ContractResultName => {
            if numbering_mode(w) {
                record_object(w, d, kind == DeclKind::ParmVar);
            }
            let n = ordinal(w, d);
            s.uv(n);
            w.decl_stream(s, d, ast.first(d));
        }
        DeclKind::StaticAssert
        | DeclKind::ConstevalBlock
        | DeclKind::TemplateTypeParm
        | DeclKind::NonTypeTemplateParm
        | DeclKind::TemplateTemplateParm
        | DeclKind::LifetimeExtendedTemporary => w.decl_stream(s, d, d),
        // Its aliased type, with what its spelling holds beyond the
        // profile (digest-kinds §4.4 "Spellings", rule 4): an unused
        // alias's lookup is still the function's.
        DeclKind::Typedef | DeclKind::TypeAlias => w.decl_stream(s, d, d),
        DeclKind::NamespaceAlias
        | DeclKind::Using
        | DeclKind::UsingShadow
        | DeclKind::ConstructorUsingShadow
        | DeclKind::UnresolvedUsingValue
        | DeclKind::UnresolvedUsingTypename
        | DeclKind::UnresolvedUsingIfExists
        | DeclKind::UsingEnum
        | DeclKind::UsingDirective
        | DeclKind::Empty
        | DeclKind::AccessSpec => {}
        DeclKind::CXXRecord | DeclKind::Enum if is_local_type(w, d) => local_type(w, s, d),
        // What a local type's lists name: a member of a local class or of
        // a class nested in one, an enumerator of a local enumeration.
        DeclKind::CXXRecord
        | DeclKind::Enum
        | DeclKind::EnumConstant
        | DeclKind::Field
        | DeclKind::IndirectField
        | DeclKind::CXXMethod
        | DeclKind::CXXConstructor
        | DeclKind::CXXDestructor
        | DeclKind::CXXConversion
        | DeclKind::Friend
        | DeclKind::FunctionTemplate => member(w, s, d),
        DeclKind::UsingPack => unsupported(s, "DeclKind/UsingPack"),
        DeclKind::FriendTemplate => unsupported(s, "DeclKind/FriendTemplate"),
        DeclKind::ImplicitParam => unsupported(s, "DeclKind/ImplicitParam"),
        DeclKind::ImplicitConceptSpecialization => {
            unsupported(s, "DeclKind/ImplicitConceptSpecialization");
        }
        // Nothing of these is declared in a block (§5.2).
        DeclKind::TranslationUnit
        | DeclKind::Namespace
        | DeclKind::LinkageSpec
        | DeclKind::Export
        | DeclKind::FileScopeAsm
        | DeclKind::ClassTemplateSpecialization
        | DeclKind::ClassTemplatePartialSpecialization
        | DeclKind::CXXDeductionGuide
        | DeclKind::VarTemplateSpecialization
        | DeclKind::VarTemplatePartialSpecialization
        | DeclKind::ClassTemplate
        | DeclKind::VarTemplate
        | DeclKind::TypeAliasTemplate
        | DeclKind::Concept
        | DeclKind::BuiltinTemplate
        | DeclKind::RequiresExprBody
        | DeclKind::TemplateParamObject => {
            s.uv(0).indeterminate(NOT_IN_BLOCK);
        }
    }
}

/// While numbering: the local object `d`, if the owner being numbered
/// declares it.
fn record_object(w: &mut Walk<'_>, d: DeclId, param: bool) {
    let d = w.ast.first(d);
    let mine = w.local.record.owner.is_some_and(|o| owner_res(w, d) == Ok(o));
    if !mine || !w.local.record.objects_seen.insert(d) {
        return;
    }
    if param {
        w.local.record.params.push(d);
    } else {
        w.local.record.others.push(d);
    }
}

/// A local class, enumeration or closure type at its declaration
/// (digest-kinds §2.4): `uv` its number, then its stream, which holds
/// its members' through its lists; met again in the same stream, the
/// number alone. While numbering: recorded if the owner being numbered
/// declares it, and its stream walked, which enters none of its members.
#[allow(
    clippy::many_single_char_names,
    reason = "the names of digest-kinds: s stream, d declaration, e entity, n number, o owner, t type"
)]
fn local_type(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    let t = w.ast.first(d);
    if numbering_mode(w) {
        if !w.local.record.seen.insert(t) {
            return;
        }
        let mine = owner_of(w, t);
        if w.local.record.owner.is_some_and(|o| mine == Ok(o)) {
            w.local.record.types.push(t);
        }
        let from = crate::units::read_decl(w, t);
        w.decl_stream(s, from, t);
        return;
    }
    let n = match type_number(w, t) {
        Ok(n) => n,
        Err(Cause::UndeclaredLocal) => {
            // The walk declares it and its owner's numbering does not.
            s.uv(0).indeterminate(NUMBERING);
            return;
        }
        Err(c) => {
            s.uv(0).indeterminate(c);
            return;
        }
    };
    s.uv(n);
    if !w.unit.local.written.insert(t) {
        return;
    }
    if let Ok(o) = owner_res(w, t) {
        let next = w.unit.local.next.entry(o).or_insert(0);
        if *next != n {
            s.indeterminate(NUMBERING);
        }
        *next = n + 1;
        #[cfg(test)]
        w.local.written.push((o, t));
    }
    let from = crate::units::read_decl(w, t);
    let named = is_named_local(w, t);
    if named {
        w.unit.local.scopes.push(t);
    }
    w.decl_stream(s, from, t);
    if named {
        w.unit.local.scopes.pop();
    }
}

/// `t` is a local class or enumeration with a name of its own (not a
/// closure, not unnamed): what a printed name shows of it.
fn is_named_local(w: &Walk<'_>, t: DeclId) -> bool {
    !is_closure(w, t) && !w.ast.decls.get(t).name().is_empty()
}

/// The names of the named local types around the code being written,
/// outermost first: those around the unit's entity (a unit under a local
/// type, such as a generic call operator's specialisation), then those
/// whose streams are being written in place. What a function name a
/// program reads (`__func__`, `__PRETTY_FUNCTION__`,
/// `source_location::function_name`) prints of them, and no rule maps,
/// since a local entity has no key (digest §4.3 "Names a program reads").
pub(crate) fn write_scope_names(w: &mut Walk<'_>, s: &mut Stream) {
    let ast = w.ast;
    let mut around = Vec::new();
    let mut c = w.unit.local.holder.and_then(|h| ast.decls.get(h).ctx());
    for _ in 0..CHAIN_LIMIT {
        let Some(x) = c else { break };
        if !w.is_local(x) && !is_local_type(w, x) {
            break;
        }
        if is_local_type(w, x) && is_named_local(w, x) {
            around.push(x);
        }
        c = ast.decls.get(x).ctx();
    }
    around.reverse();
    let all: Vec<DeclId> = around.into_iter().chain(w.unit.local.scopes.iter().copied()).collect();
    s.count(all.len());
    for t in all {
        w.write_name(s, t);
    }
}

/// The names of the named local types around entity `d`, outermost
/// first: what a function name of `d` prints of them, and a reference to
/// `d` does not hold, since a local entity has no key (digest §9: they
/// are part of a name-derived value's identity, in both modes).
pub(crate) fn write_names_around(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    let ast = w.ast;
    let mut around = Vec::new();
    let mut next = ast.decls.get(d).ctx();
    for _ in 0..CHAIN_LIMIT {
        let Some(x) = next else { break };
        if !w.is_local(x) && !is_local_type(w, x) {
            break;
        }
        if is_local_type(w, x) && is_named_local(w, x) {
            around.push(x);
        }
        next = ast.decls.get(x).ctx();
    }
    around.reverse();
    s.count(around.len());
    for t in around {
        w.write_name(s, t);
    }
}

/// The names of the named local types the type `q` mentions (itself, a
/// pointee, an element, a parameter or return type, a template argument),
/// each with the named local types around it, outermost first, in the
/// order met: what `typeid(q).name()` prints of them (digest §4.3 "Names
/// a program reads").
#[allow(
    clippy::many_single_char_names,
    reason = "the names of digest-kinds: s stream, d declaration, e entity, n number, o owner, t type"
)]
pub(crate) fn write_type_names(w: &mut Walk<'_>, s: &mut Stream, q: QualType) {
    let ast = w.ast;
    let mut found: Vec<DeclId> = Vec::new();
    let mut stack = vec![ast.types.canonical(q)];
    let mut steps = 0u32;
    let mut cut = false;
    while let Some(q) = stack.pop() {
        steps += 1;
        if steps > CHAIN_LIMIT {
            cut = true;
            break;
        }
        match ast.types.view(q.ty()) {
            TypeView::Pointer(p)
            | TypeView::LValueRef(p)
            | TypeView::RValueRef(p)
            | TypeView::IncompleteArray(p)
            | TypeView::Paren(p)
            | TypeView::Complex(p)
            | TypeView::Atomic(p) => stack.push(ast.types.canonical(p)),
            TypeView::MemberPointer { pointee, class } => {
                stack.push(QualType::unqual(class));
                stack.push(ast.types.canonical(pointee));
            }
            TypeView::ConstantArray(a) => stack.push(ast.types.canonical(a.elem)),
            TypeView::Function(p) => {
                for &t in ast.types.params(&p).iter().rev() {
                    stack.push(ast.types.canonical(t));
                }
                stack.push(ast.types.canonical(p.ret()));
            }
            TypeView::Record(d) | TypeView::Enum(d) => {
                let d = ast.first(d);
                // Around it, outermost first, then itself.
                let mut chain = Vec::new();
                let mut x = Some(d);
                for _ in 0..CHAIN_LIMIT {
                    let Some(y) = x else { break };
                    if is_local_type(w, y) && is_named_local(w, y) {
                        chain.push(ast.first(y));
                    } else if !w.is_local(y) && !is_local_type(w, y) {
                        break;
                    }
                    x = ast.decls.get(y).ctx();
                }
                for t in chain.into_iter().rev() {
                    if !found.contains(&t) {
                        found.push(t);
                    }
                }
                if let Some((_, si)) = ast.spec_info(d) {
                    let args: Vec<QualType> = ast
                        .types
                        .args(ast.types.canonical_args(si.args))
                        .iter()
                        .filter_map(|a| match a.view() {
                            ArgView::Type(t) => Some(ast.types.canonical(t)),
                            _ => None,
                        })
                        .collect();
                    for t in args.into_iter().rev() {
                        stack.push(t);
                    }
                }
            }
            _ => {}
        }
    }
    s.count(found.len());
    for t in found {
        w.write_name(s, t);
    }
    if cut {
        s.indeterminate(TOO_DEEP);
    }
}

/// A member of a local type written in place (digest §4.3: it has an
/// ordinary key under the type): its key, then its stream (§5.1); a
/// member keyed by content its key alone, which holds its digest. While
/// numbering, nothing: the numbering does not enter a local type's
/// members.
fn member(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    if numbering_mode(w) {
        call_op_prototype(w, d);
        return;
    }
    let e = w.entity(d);
    match info(w.ast.decls.get(e).kind()).role {
        Role::Entity(_) => {
            w.write_key(s, e);
            let from = stream_source(w, e);
            w.decl_stream(s, from, e);
        }
        Role::Content(_) => w.write_key(s, e),
        Role::Nothing
        | Role::Transparent
        | Role::Root
        | Role::Local
        | Role::InParent
        | Role::Unsupported(_) => {
            s.uv(0).indeterminate(Cause::Unaccounted("local/member"));
        }
    }
}

/// While numbering, at member `d` of a local type: if `d` is a closure's
/// call operator (or its template), what of its prototype Kiln gives the
/// owner of the closure rather than the call operator (digest §4.3): its
/// parameters' default arguments, walked as its stream walks them. Its
/// body is the call operator's own; a lambda in its trailing
/// requires-clause is in its key (`LAMBDA_IN_KEY`), and one in a
/// noexcept operand is written as the operand's value only.
fn call_op_prototype(w: &mut Walk<'_>, d: DeclId) {
    let ast = w.ast;
    let f = match ast.decls.get(d).kind() {
        DeclKind::FunctionTemplate => match ast.template_info(d).and_then(|i| i.pattern) {
            Some(f) => f,
            None => return,
        },
        DeclKind::CXXMethod => d,
        _ => return,
    };
    if ast.call_op_lambda(f).is_none() {
        return;
    }
    let params: Vec<DeclId> = ast
        .decls
        .function(f)
        .map(|x| ast.lists.iter::<DeclId>(x.params).collect())
        .unwrap_or_default();
    let mut scratch = Stream::new();
    for p in params {
        let (_, default) = crate::decl_func::written_default(w, p);
        crate::expr::child_opt(w, &mut scratch, default);
    }
}

/// A kind Kiln does not build (`Unsupported("<short name>/<row>")`).
fn unsupported(s: &mut Stream, row: &'static str) {
    s.uv(0).indeterminate(Cause::Unsupported(row));
}

/// What the parameters of a block-scope function declaration `d` hold,
/// after its `ref`: `count`, then per parameter one `child?`, its default
/// argument, and its attributes and annotations (`attrs::own_attrs`).
/// They are that declaration's own ([dcl.fct.default]/4), which the
/// entity's unit leaves out (`spec/landed/digest-kinds/WP-026.md` and
/// `WP-100.md`, additions to §2.4).
fn block_defaults(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    let ast = w.ast;
    let params: Vec<DeclId> = ast
        .decls
        .function(d)
        .map(|f| ast.lists.iter::<DeclId>(f.params).collect())
        .unwrap_or_default();
    s.count(params.len());
    for p in params {
        let default = ast.decls.param(p).and_then(|x| x.default_arg);
        crate::expr::child_opt(w, s, default);
        // Its attributes and annotations, which are not the entity's
        // either (`spec/landed/digest-kinds/WP-100.md`).
        crate::attrs::own_attrs(w, s, p);
    }
}

/// A declaration `decl` met that §2.4 does not say can be in a block.
const NOT_IN_BLOCK: Cause = Cause::Unaccounted("decl/kind");

/// `LambdaExpr` (digest-kinds §7, row `LambdaExpr.a`): the closure type,
/// by `decl` (§2.4). Inside a key or a type digest the lambda-expression
/// makes what holds it indeterminate instead (digest §4.3).
pub(crate) fn lambda_expr(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    let n = crate::expr::node(w, e);
    crate::expr::check_bits(s, &n, 0, Cause::Unaccounted("ExprNode/LambdaExpr.bits"));
    crate::expr::check_aux(s, &n, 0, Cause::Unaccounted("ExprNode/LambdaExpr.aux"));
    if n.b != 0 {
        s.indeterminate(Cause::Unaccounted("ExprNode/LambdaExpr.b"));
    }
    let closure = LambdaId::try_new(n.a)
        .filter(|l| l.index() as usize <= w.ast.exprs.lambdas.len())
        .map(|l| w.ast.exprs.lambdas.get(l).closure);
    let Some(closure) = closure else {
        s.uv(0).indeterminate(Cause::Unaccounted("ExprNode/LambdaExpr.a"));
        return;
    };
    if w.in_key > 0 {
        s.uv(0).indeterminate(LAMBDA_IN_KEY);
        return;
    }
    if w.local.in_type > 0 {
        s.uv(0).indeterminate(LAMBDA_IN_TYPE);
        return;
    }
    write_decl(w, s, closure);
}

/// `record LambdaData` (§10.4): the `lambda` row of a closure type's
/// `RecordData`.
pub(crate) fn lambda_data(w: &mut Walk<'_>, s: &mut Stream, l: LambdaId) {
    let ast = w.ast;
    if l.index() as usize > ast.exprs.lambdas.len() {
        s.uv(0).indeterminate(Cause::Unaccounted("LambdaData/missing"));
        return;
    }
    let LambdaData {
        closure: _,
        call_op: _,
        default,
        default_loc: _,
        ref captures,
        inits,
        body: _,
        lbracket: _,
        rbracket: _,
        explicit_params: _,
        explicit_result,
        mutable,
        mangling,
    } = *ast.exprs.lambdas.get(l);
    // default
    s.tag(capture_default::row(&default));
    // captures: record LambdaCapture*, in closure member order.
    s.count(captures.len());
    for c in captures {
        let LambdaCapture { kind, var, field, loc: _, ellipsis, implicit, init } = *c;
        s.tag(capture_kind::row(&kind));
        w.write_ref_opt(s, var);
        w.write_ref_opt(s, field);
        // ellipsis-present, implicit, init; `ellipsis-position` is out.
        s.flags(FlagWord::new().bit(ellipsis.is_valid()).bit(implicit).bit(init));
    }
    // inits: child*?
    s.opt(inits.is_some());
    if let Some(list) = inits {
        let es: Vec<ExprId> = ast.lists.iter::<ExprId>(list).collect();
        crate::expr::children(w, s, &es);
    }
    // explicit_result, mutable: one flag word.
    s.flags(FlagWord::new().bit(explicit_result).bit(mutable));
    // mangling: record LambdaMangling.
    let LambdaMangling { number, context } = mangling;
    s.uv(u64::from(number));
    w.write_ref_opt(s, context);
}

#[cfg(test)]
#[allow(
    clippy::many_single_char_names,
    reason = "the names of digest-kinds: s stream, d declaration, e entity, n number, o owner, t type"
)]
mod tests {
    use std::fmt::Write as _;

    use slip_kiln::kiln::ast::decl::DeclKind;

    use crate::decl_func::tests::{named, of_kind, with_walk};
    use crate::keys::KeyRes;
    use crate::stream::Stream;

    fn write(w: &mut Walk<'_>, d: DeclId) -> Stream {
        let mut s = Stream::new();
        super::write_decl(w, &mut s, d);
        s
    }

    // digest-kinds §2.4, digest §4.3: a parameter writes its ordinal, the
    // next one, then its stream.
    #[test]
    fn a_parameter_writes_its_ordinal_first() {
        with_walk("void f(int a, int b);", |w| {
            let a = named(w, "a", DeclKind::ParmVar)[0];
            let b = named(w, "b", DeclKind::ParmVar)[0];
            let sa = write(w, a);
            let sb = write(w, b);
            assert_eq!(sa.as_bytes()[0], 0);
            assert_eq!(sb.as_bytes()[0], 1);
            assert_eq!(sa.as_bytes()[1..], sb.as_bytes()[1..]);
            // Met again, the same ordinal.
            assert_eq!(write(w, a).as_bytes(), sa.as_bytes());
        });
    }

    // digest-kinds §2.4: a block-scope extern variable writes a `ref`; a
    // block-scope function a `ref`, then a count and one `child?` per
    // parameter, then its attributes and annotations: what that
    // declaration holds (WP-026, WP-100).
    #[test]
    fn a_block_scope_entity_with_linkage_is_a_ref() {
        let src = "void h(int, int); extern int v;\n\
                   void g() { void h(int, int = 1); extern int v; }";
        with_walk(src, |w| {
            let v = *named(w, "v", DeclKind::Var).last().expect("the block-scope v");
            let mut want = Stream::new();
            w.write_ref(&mut want, v);
            assert_eq!(write(w, v).as_bytes(), want.as_bytes());
            let h = *named(w, "h", DeclKind::Function).last().expect("the block-scope h");
            let params: Vec<_> =
                w.ast.lists.iter::<DeclId>(w.ast.decls.function(h).expect("h").params).collect();
            let second = w.ast.decls.param(params[1]).and_then(|p| p.default_arg);
            assert!(second.is_some());
            let mut want = Stream::new();
            w.write_ref(&mut want, h);
            want.count(2);
            crate::expr::child_opt(w, &mut want, None);
            crate::attrs::own_attrs(w, &mut want, params[0]);
            crate::expr::child_opt(w, &mut want, second);
            crate::attrs::own_attrs(w, &mut want, params[1]);
            assert_eq!(write(w, h).as_bytes(), want.as_bytes());
            // The namespace-scope declaration holds no default.
            let outer = named(w, "h", DeclKind::Function)[0];
            assert_ne!(write(w, outer).as_bytes(), want.as_bytes());
            assert_eq!(w.unit.next_local, 0);
        });
    }

    // digest-kinds §2.4 (WP-100): a block-scope function declaration
    // writes the attributes on its parameters, which the entity's unit
    // leaves out (`attrs::tests::s10_1_param_chain_leaves_out_block_scope`).
    #[test]
    fn a_block_scope_parameter_writes_its_attributes() {
        let block = |src: &str| {
            with_walk(src, |w| {
                let h = *named(w, "h", DeclKind::Function).last().expect("the block-scope h");
                write(w, h).as_bytes().to_vec()
            })
        };
        let with = block("void h(int *);\nvoid g() { void h(int *p __attribute__((nonnull))); }");
        let without = block("void h(int *);\nvoid g() { void h(int *p); }");
        assert_ne!(with, without);
    }

    // digest-kinds §2.4: a using-declaration or using-directive writes
    // nothing; a typedef or alias writes its rows (its aliased type).
    #[test]
    fn a_name_only_declaration_writes_nothing() {
        let src = "namespace n { int i; } void g() { typedef int T; using U = int; using n::i; using namespace n; }";
        with_walk(src, |w| {
            for (kind, empty) in [
                (DeclKind::Typedef, false),
                (DeclKind::TypeAlias, false),
                (DeclKind::Using, true),
                (DeclKind::UsingDirective, true),
            ] {
                let d = of_kind(w, kind)[0];
                let s = write(w, d);
                assert_eq!(s.as_bytes().is_empty(), empty, "{kind:?}");
                assert_eq!(s.cause(), None, "{kind:?}");
            }
        });
    }

    // digest-kinds §2.4, §10.4: a local class writes its number in its
    // owner's numbering, then its stream, never pending; a kind not
    // declared in a block is unaccounted, never a panic.
    #[test]
    fn local_types_are_numbered_and_strangers_unaccounted() {
        with_walk("namespace n {} void g() { struct K {}; struct L {}; }", |w| {
            let k = named(w, "K", DeclKind::CXXRecord)[0];
            let l = named(w, "L", DeclKind::CXXRecord)[0];
            assert_eq!(write(w, k).as_bytes()[0], 0);
            let s = write(w, l);
            assert_eq!(s.as_bytes()[0], 1);
            assert_eq!(s.cause(), None);
            let n = named(w, "n", DeclKind::Namespace)[0];
            assert_eq!(write(w, n).cause(), Some(super::NOT_IN_BLOCK));
        });
    }

    // digest-kinds §2.4 (WP-026): a block-scope `consteval` block, legal
    // C++26, writes its stream with no ordinal, as a `static_assert` does.
    #[test]
    fn a_consteval_block_in_a_block() {
        with_walk("void g() { consteval { } }", |w| {
            let d = of_kind(w, DeclKind::ConstevalBlock)[0];
            let mut want = Stream::new();
            w.decl_stream(&mut want, d, d);
            // The block holds a closure, which a unit writes in full once:
            // write it again in a unit of its own.
            drop(w.take_unit());
            let got = write(w, d);
            assert_eq!(got.as_bytes(), want.as_bytes());
            assert_ne!(got.cause(), Some(super::NOT_IN_BLOCK));
            assert_eq!(w.unit.next_local, 0);
        });
    }

    // digest §4.3 "An owner keyed by its content": a namespace-scope
    // consteval block owns its closure; the key of the closure's call
    // operator is the same whether it or the block's is asked for first,
    // and has no cause (inside the block's content it is under
    // `SELF_KEY`, never memoised).
    #[test]
    fn a_consteval_block_owns_its_closure() {
        let src = "consteval { int a = 1; (void)a; }";
        let key = |first_block: bool| {
            with_walk(src, |w| {
                let b = of_kind(w, DeclKind::ConstevalBlock)[0];
                let op = of_kind(w, DeclKind::CXXMethod)[0];
                let r = w.ast.decls.get(op).ctx().expect("the closure");
                assert_eq!(super::owner_of(w, r), Ok(b));
                if first_block {
                    let _ = w.key(b);
                }
                match w.key(op) {
                    KeyRes::Key { id, cause, .. } => (id, cause),
                    KeyRes::None(c) => panic!("no key: {c:?}"),
                }
            })
        };
        let (a, b) = (key(true), key(false));
        assert_eq!(a, b);
        assert_eq!(a.1, None);
    }

    // digest-kinds §2.4, §9: a static_assert in a block writes its stream
    // with no ordinal.
    #[test]
    fn a_static_assert_has_no_ordinal() {
        with_walk("void g() { static_assert(true); }", |w| {
            let d = of_kind(w, DeclKind::StaticAssert)[0];
            let mut want = Stream::new();
            w.decl_stream(&mut want, d, d);
            assert_eq!(write(w, d).as_bytes(), want.as_bytes());
            assert_eq!(w.unit.next_local, 0);
        });
    }

    // ---- WP-040: owners, numbering, local types, closures ----

    use std::collections::HashMap;

    use slip_kiln::kiln::ast::{DeclId, LambdaId};
    use slip_model::Digest;
    use slip_model::keys::RuleTable;

    use crate::units::Written;
    use crate::walk::Walk;
    use crate::{Cause, Detail, Input, Options, Output};

    /// Parses `files` (`main.cpp` first) under `std` and runs `f` on the
    /// finished walk, on a thread of `STACK_SIZE`. `None` when the source
    /// has errors.
    fn run_files<R: Send>(
        files: &[(&str, &str)],
        std: &str,
        f: impl FnOnce(&mut Walk<'_>, &[Written], &Output) -> R + Send,
    ) -> Option<R> {
        std::thread::scope(|sc| {
            std::thread::Builder::new()
                .stack_size(crate::STACK_SIZE)
                .spawn_scoped(sc, || {
                    let snippet = slip_kiln::snippet::Snippet {
                        files,
                        main: "main.cpp",
                        include: &[""],
                        std,
                    };
                    slip_kiln::snippet::parse(&snippet, |p| {
                        if p.errors > 0 {
                            return None;
                        }
                        let path = |_| Vec::new();
                        let input = Input { ast: p.ast, sm: p.sm, subject_path: &path };
                        let rules = RuleTable::empty();
                        let options = Options { rules: &rules, detail: Detail::None, names: true };
                        Some(crate::walk::run_with(&input, &options, |w, u, o| f(w, u, &o)))
                    })
                    .ok()
                    .flatten()
                })
                .expect("a thread")
                .join()
                .expect("the walk")
        })
    }

    /// [`run_files`] over one C++26 `main.cpp`; panics on errors.
    fn run<R: Send>(
        src: &str,
        f: impl FnOnce(&mut Walk<'_>, &[Written], &Output) -> R + Send,
    ) -> R {
        run_files(&[("main.cpp", src)], "c++26", f).expect("the snippet parses without errors")
    }

    /// `Kind name` of `d`, ` spec` for a specialisation or a member of one.
    fn describe(w: &Walk<'_>, d: DeclId) -> String {
        let ast = w.ast;
        let n = ast.decls.get(d);
        let name = n.name().as_ident().map_or("-", |s| ast.symbols().as_str(s));
        let mut out = format!("{:?} {name}", n.kind());
        let mut x = Some(d);
        let mut spec = false;
        while let Some(y) = x {
            spec |= ast.spec_info(y).is_some();
            x = ast.decls.get(y).ctx();
        }
        if spec {
            out.push_str(" spec");
        }
        out
    }

    /// Every closure's owner, by the line of its `[`, in creation order.
    fn owners(w: &mut Walk<'_>) -> Vec<(u32, String)> {
        let ast = w.ast;
        let mut out = Vec::new();
        for i in 1..=ast.exprs.lambdas.len() {
            let l = ast.exprs.lambdas.get(LambdaId::new(u32::try_from(i).expect("an index")));
            let line = w.sm.line_col(l.lbracket).map_or(0, |lc| lc.line);
            let o = match super::owner_of(w, l.closure) {
                Ok(o) => describe(w, o),
                Err(c) => format!("{c:?}"),
            };
            out.push((line, o));
        }
        out
    }

    // digest §4.3, digest-kinds §10.4: the owner of a closure in each place
    // Kiln's own test of `Ast::closure_owner` puts one
    // (`kiln/crates/kiln_parse/tests/closure_owner.rs` SRC), taken to the
    // entity with a key: a parameter's function, a template parameter's
    // template; a namespace-scope `static_assert`'s closure has none.
    #[test]
    fn the_owner_of_each_closure() {
        const SRC: &str = "struct S {\n\
          void f(int a = [] { return 1; }());\n\
          int m = [] { return 2; }();\n\
        };\n\
        inline auto iv = [] { return 3; };\n\
        template <class T> auto tv = [] { return T(); };\n\
        template <class T> auto sig(T) -> decltype([] { return T(); }()) { return {}; }\n\
        template <class T = decltype([] {})> struct D {};\n\
        void nd(decltype([] {}) p);\n\
        auto nr() -> decltype([] { return 4; }());\n\
        template <class T> requires (sizeof([] {}) > 0) void rq(T) {}\n\
        template <class T> void trq(T) requires (sizeof([] {}) > 0) {}\n\
        template <class T> using A = decltype([] {});\n\
        void fd(int x = [] { return 5; }());\n\
        auto px = [] { return 6; };\n\
        void body() { auto l = [] {}; }\n\
        inline auto nest = [] { return [] {}; };\n\
        template <class T> void tb() { auto l = [] {}; }\n\
        template <class T> struct C {\n\
          int m = [] { return 7; }();\n\
          void g(int a = [] { return 8; }()) {}\n\
        };\n\
        static_assert(sizeof([] {}) > 0);\n\
        using NA = decltype([] {});\n\
        template <class T> struct M { void h(decltype([] {}) p); };\n\
        template <class T> concept K = requires { [] {}; };\n\
        void lc() { struct L { int q = [] { return 9; }(); }; }\n\
        template <class T> requires (sizeof([] {}) > 0) struct RC {};\n\
        void use() {\n\
          tv<int>();\n\
          sig(1);\n\
          D<> d;\n\
          rq(1);\n\
          trq(1);\n\
          A<int> a;\n\
          tb<int>();\n\
          C<int> c;\n\
          c.g();\n\
          M<int> mm;\n\
          RC<int> rc;\n\
        }\n";
        let got = run(SRC, |w, _, _| owners(w));
        let want: &[(u32, &str)] = &[
            // An in-class default argument: the member function.
            (2, "CXXMethod f"),
            // A default member initialiser: the field.
            (3, "Field m"),
            // An inline variable; a variable template (its template).
            (5, "Var iv"),
            (6, "VarTemplate tv"),
            // A function template's signature: the template.
            (7, "FunctionTemplate sig"),
            // A default template argument: the template it belongs to.
            (8, "ClassTemplate D"),
            // `decltype` in a non-template signature: the function.
            (9, "Function nd"),
            (10, "Function nr"),
            // Requires-clauses: the templated function's template.
            (11, "FunctionTemplate rq"),
            (12, "FunctionTemplate trq"),
            // An alias template.
            (13, "TypeAliasTemplate A"),
            // A namespace-scope function's default argument: the function.
            (14, "Function fd"),
            (15, "Var px"),
            // A function body: the function.
            (16, "Function body"),
            // A lambda's body: the call operator.
            (17, "Var nest"),
            (17, "CXXMethod -"),
            (18, "FunctionTemplate tb"),
            (21, "CXXMethod g"),
            (20, "Field m"),
            // A namespace-scope `static_assert`: no declaration with a key
            // holds it but the content-keyed one, under which no local type
            // can be keyed (digest §4.3).
            (23, "Unsupported(\"local/closure-owner\")"),
            (24, "TypeAlias NA"),
            (25, "CXXMethod h"),
            // A requires-expression's parameter scope is passed.
            (26, "Concept K"),
            // A local class's member: the member.
            (27, "Field q"),
            (28, "ClassTemplate RC"),
            // Instantiations: the specialisation where one is instantiated,
            // else the pattern's owner.
            (6, "Var tv spec"),
            (7, "Function sig spec"),
            (8, "ClassTemplate D"),
            (11, "FunctionTemplate rq"),
            (12, "FunctionTemplate trq"),
            (13, "TypeAliasTemplate A"),
            (20, "Field m spec"),
            (21, "CXXMethod g spec"),
            (25, "CXXMethod h spec"),
            (28, "ClassTemplate RC"),
            (18, "Function tb spec"),
        ];
        let got: Vec<(u32, &str)> = got.iter().map(|(l, d)| (*l, d.as_str())).collect();
        assert_eq!(got, want);
    }

    /// The pair-test sources: each side of every `t.cpp` under
    /// `tests/{differ,equal,demand,indeterminate}` that the snippet front
    /// end parses without errors and without `// args:` or `// target:`.
    fn pair_sources() -> Vec<(String, String, bool)> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
        let mut out = Vec::new();
        let mut stack: Vec<std::path::PathBuf> =
            ["differ", "equal", "demand", "indeterminate"].iter().map(|d| root.join(d)).collect();
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.file_name().is_some_and(|n| n == "t.cpp") {
                    let Ok(text) = std::fs::read_to_string(&p) else { continue };
                    let head: Vec<&str> = text.lines().take(6).collect();
                    if head.iter().any(|l| {
                        l.starts_with("// args:")
                            || l.starts_with("// target:")
                            || l.starts_with("// errors:")
                    }) {
                        continue;
                    }
                    let std = head
                        .iter()
                        .find_map(|l| l.strip_prefix("// std:"))
                        .map_or("c++20", str::trim)
                        .to_owned();
                    out.push((text.clone(), std.clone(), false));
                    out.push((text, std, true));
                }
            }
        }
        out.sort();
        out
    }

    // digest §4.3 "One order": over every pair-test source, both sides, the
    // local types the walk declares in place, per owner, are its owner's
    // numbering (once per stream that writes the owner's stream), and a
    // unit with none of them numbers nothing it does not need.
    #[test]
    fn the_walk_declares_what_the_numbering_found() {
        let meta = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/meta.h"),
        )
        .unwrap_or_default();
        let mut checked = 0;
        let mut with_types = 0;
        for (src, std, b) in pair_sources() {
            let side = if b { "#define B 1\n" } else { "" };
            let files = [("main.cpp", src.as_str()), ("side.h", side), ("meta.h", meta.as_str())];
            let r = run_files(&files, &std, |w, _, _| {
                let written = std::mem::take(&mut w.local.written);
                let mut by_owner: HashMap<DeclId, Vec<DeclId>> = HashMap::new();
                for (o, t) in written {
                    by_owner.entry(o).or_default().push(t);
                }
                let mut owners: Vec<DeclId> = by_owner.keys().copied().collect();
                owners.sort();
                let mut problems = Vec::new();
                for o in owners {
                    let got = &by_owner[&o];
                    let n = super::numbering(w, o);
                    let want = n.types.clone();
                    let ok = !want.is_empty()
                        && got.len().is_multiple_of(want.len())
                        && got.chunks(want.len()).all(|c| c == want.as_slice());
                    if !ok && n.cut.is_none() {
                        problems
                            .push(format!("{}: wrote {got:?}, numbered {want:?}", describe(w, o)));
                    }
                }
                // `direct_param` gives the index the numbering gives.
                let numbered: Vec<(DeclId, Vec<DeclId>)> =
                    w.local.numberings.iter().map(|(&o, n)| (o, n.objects.clone())).collect();
                for (o, objects) in numbered {
                    for (i, &d) in objects.iter().enumerate() {
                        if let Some(j) = super::direct_param(w, o, d)
                            && j != i as u64
                        {
                            problems.push(format!(
                                "{}: parameter {d:?} at {i}, direct {j}",
                                describe(w, o)
                            ));
                        }
                    }
                }
                (problems, !by_owner.is_empty(), w.local.late)
            });
            // Asserted out here: a panic inside the parse's callback reads
            // as a source that does not parse.
            let Some((problems, any, late)) = r else { continue };
            assert_eq!(late, 0, "a numbering inside a unit's walk:\n{src}");
            assert!(problems.is_empty(), "side {}: {problems:?}\n{src}", if b { "B" } else { "A" });
            checked += 1;
            with_types += usize::from(any);
        }
        eprintln!("{checked} sides checked, {with_types} with local types written");
        assert!(checked > 1000 && with_types > 20, "the sources were read");
    }

    // digest §4.3 "No key, no type, no reference": numbering an owner whose
    // body names the local types of ten other owners numbers one owner.
    #[test]
    fn a_numbering_numbers_one_owner() {
        let mut src = String::new();
        let mut body = String::from("void g() {\n");
        for i in 0..10 {
            let _ = writeln!(src, "auto f{i}() {{ struct L {{ int m; }}; return L{{{i}}}; }}");
            let _ = writeln!(body, "  decltype(f{i}()) x{i} = f{i}(); int y{i} = x{i}.m;");
        }
        body.push_str("  struct Own {};\n}\n");
        src.push_str(&body);
        with_walk(&src, |w| {
            let g = named(w, "g", DeclKind::Function)[0];
            let own = named(w, "Own", DeclKind::CXXRecord)[0];
            let n = super::numbering(w, g);
            assert_eq!(n.types, vec![own]);
            assert_eq!(n.cut, None);
            assert_eq!(w.local.numbered, 1);
            // Memoised: asked again, nothing more is numbered.
            let _ = super::numbering(w, g);
            assert_eq!(w.local.numbered, 1);
        });
    }

    // digest §4.3 "Linear and bounded": a translation unit without local
    // types numbers no owner, local objects or not; one with them numbers
    // each owner once, before any unit, and none inside a unit's walk.
    #[test]
    fn the_common_case_numbers_nothing() {
        let src = "int f(int a) { int b = a; for (int i = 0; i < b; ++i) b += i; return b; }\n\
                   struct S { int m; int g(int p) { int q = p; return m + q; } };\n\
                   template <class T> T h(T t) { T u = t; return u; }\nint v = h(1);\n";
        assert_eq!(run(src, |w, _, _| w.local.numbered), 0);
        let src = "int f() { struct L { int m; }; auto l = [] { return 1; }; return L{1}.m + l(); }\n\
                   auto g() { struct M {}; return M{}; }\nauto x = g();\nauto y = g();\n";
        // f, g and the call operator of f's closure.
        assert_eq!(run(src, |w, _, _| (w.local.numbered, w.local.late)), (3, 0));
    }

    /// Every written unit's key and digest, sorted.
    fn keys_and_digests(written: &[Written]) -> Vec<(Digest, Digest, Option<Cause>)> {
        let mut v: Vec<_> = written.iter().map(|u| (u.key, u.digest, u.cause)).collect();
        v.sort();
        v
    }

    /// The walk of `run_with` by hand: `before` runs after the units are
    /// collected, and `reverse` writes the collected units last first.
    fn walk_by_hand(
        w: &mut Walk<'_>,
        before: impl FnOnce(&mut Walk<'_>),
        reverse: bool,
    ) -> Vec<(Digest, Digest, Option<Cause>)> {
        crate::units::collect(w);
        before(w);
        if reverse {
            w.queue.reverse();
        }
        let mut written = Vec::new();
        let mut i = 0;
        while i < w.queue.len() {
            let (e, class) = w.queue[i];
            i += 1;
            if let Some(u) = crate::units::write(w, e, class) {
                // Its resolved units after it, as `run_with` writes them.
                let mut resolved = Vec::new();
                crate::demand::resolved_units(w, e, &u, &mut resolved);
                written.push(u);
                written.append(&mut resolved);
            }
        }
        written.push(crate::units::order_unit(w));
        keys_and_digests(&written)
    }

    // digest §4.3 "Order of units does not matter": the walk numbers every
    // owner before any unit (`number_all`); the same keys and digests when
    // each owner is numbered where a unit's walk first asks (in unit
    // order and in reverse), and when numbered by type before any unit.
    #[test]
    fn the_order_of_units_does_not_matter() {
        let src = "auto mk() { struct P { int v; }; return P{1}; }\n\
                   int use() { return mk().v; }\n\
                   int f(int a, int b) {\n\
                     struct L { int m; int get() const { return m; } };\n\
                     auto l = [=, c = a](auto x) { struct In { int k; }; return In{int(c + b + x)}.k; };\n\
                     auto r = [&] { return L{a}.get(); };\n\
                     return l(1) + l(2L) + r();\n\
                   }\n\
                   inline auto iv = [](int x) { return x; };\n\
                   int k = iv(1);\n\
                   auto h() { return [] { enum E { e1, e2 }; return e2; }; }\n\
                   auto hv = h()();\n";
        let (normal, forward, backward) = std::thread::scope(|sc| {
            std::thread::Builder::new()
                .stack_size(crate::STACK_SIZE)
                .spawn_scoped(sc, || {
                    let files = [("main.cpp", src)];
                    let snippet = slip_kiln::snippet::Snippet {
                        files: &files,
                        main: "main.cpp",
                        include: &[""],
                        std: "c++20",
                    };
                    slip_kiln::snippet::parse(&snippet, |p| {
                        assert_eq!(p.errors, 0, "{}", p.diagnostics);
                        let path = |_| Vec::new();
                        let input = Input { ast: p.ast, sm: p.sm, subject_path: &path };
                        let rules = RuleTable::empty();
                        let options = Options { rules: &rules, detail: Detail::None, names: false };
                        let normal =
                            crate::walk::run_with(&input, &options, |_, u, _| keys_and_digests(u));
                        let mut w = Walk::new(&input, &rules);
                        let forward = walk_by_hand(
                            &mut w,
                            |w| {
                                let ast = w.ast;
                                for d in ast.decls.ids() {
                                    if super::is_local_type(w, d) {
                                        let _ = super::type_number(w, d);
                                    }
                                }
                                assert!(w.local.numbered >= 6, "{}", w.local.numbered);
                            },
                            false,
                        );
                        let mut w = Walk::new(&input, &rules);
                        let backward = walk_by_hand(&mut w, |_| {}, true);
                        assert!(w.local.numbered >= 6, "numbered inside the walk");
                        let mut w = Walk::new(&input, &rules);
                        let lazy = walk_by_hand(&mut w, |_| {}, false);
                        assert_eq!(lazy, backward);
                        (normal, forward, backward)
                    })
                    .expect("parses")
                })
                .expect("a thread")
                .join()
                .expect("the walk")
        });
        assert!(
            normal.iter().all(|(_, _, c)| !matches!(c, Some(c) if !matches!(c, Cause::Pending(_)))),
            "{normal:?}"
        );
        assert_eq!(normal, forward);
        assert_eq!(normal, backward);
    }

    /// The specialisations of the call operator of the closure of the
    /// lambda with index `l`.
    fn call_op_specs(w: &Walk<'_>, closure: DeclId) -> Vec<DeclId> {
        let ast = w.ast;
        ast.decls
            .ids()
            .filter(|&d| {
                ast.decls.get(d).kind() == DeclKind::CXXMethod
                    && ast.spec_info(d).is_some()
                    && ast.decls.get(d).ctx() == Some(closure)
            })
            .collect()
    }

    // digest §4.1, §4.3 "Local objects of another unit": the index of a
    // local object among its owner's (init-capture variables, parameters,
    // the others), and `Ref::outer` from a unit that does not hold the
    // declaration: the owner's key and that index; `Ref::local` from the
    // one that does.
    #[test]
    fn outer_references_by_owner_and_index() {
        let src = "int f(int p, int q) {\n\
                     int o = 0;\n\
                     auto [s, t] = [] { struct R { int a, b; }; return R{1, 2}; }();\n\
                     auto l = [=, c = p](auto x) { return c + p + q + o + s + x; };\n\
                     return l(1);\n\
                   }\n";
        with_walk(src, |w| {
            let ast = w.ast;
            let f = named(w, "f", DeclKind::Function)[0];
            let one = |w: &Walk<'_>, n: &str, k: DeclKind| ast.first(named(w, n, k)[0]);
            let (p, q, o) = (
                one(w, "p", DeclKind::ParmVar),
                one(w, "q", DeclKind::ParmVar),
                one(w, "o", DeclKind::Var),
            );
            let (s, l) = (one(w, "s", DeclKind::Binding), one(w, "l", DeclKind::Var));
            let c = one(w, "c", DeclKind::Var);
            let x = one(w, "x", DeclKind::ParmVar);
            let objects = super::numbering(w, f).objects.clone();
            // The structured binding's own variable is between `o` and `s`.
            assert_eq!(objects.len(), 7, "{objects:?}");
            assert_eq!(objects[..3], [p, q, o]);
            assert!(objects.contains(&s) && objects.contains(&l));
            let closure = ast.decls.ids().find(|&d| {
                super::is_closure(w, d)
                    && ast
                        .decls
                        .record(d)
                        .and_then(|r| r.lambda)
                        .is_some_and(|li| ast.exprs.lambdas.get(li).captures.len() > 1)
            });
            let closure = closure.expect("the generic closure");
            let op = super::owner_of(w, c).expect("c's owner");
            assert_eq!(describe(w, op), "FunctionTemplate -");
            assert_eq!(super::numbering(w, op).objects, vec![c, x]);
            let spec = call_op_specs(w, closure)[0];
            assert!(super::derived_under_local(w, spec));
            assert!(crate::units::unit_class(w, spec).is_some());
            let key = |w: &mut Walk<'_>, d: DeclId| match w.key(d) {
                KeyRes::Key { id, .. } => id,
                KeyRes::None(c) => panic!("no key: {c:?}"),
            };
            let (kf, kop) = (key(w, f), key(w, op));
            // From the specialisation's stream: outer.
            w.unit.local.holder = Some(spec);
            for (d, owner, index) in [(p, kf, 0), (q, kf, 1), (o, kf, 2), (c, kop, 0)] {
                let mut got = Stream::new();
                w.write_ref(&mut got, d);
                let mut want = Stream::new();
                want.uv(crate::refs::REF_OUTER).digest(&owner).uv(index);
                assert_eq!(got.as_bytes(), want.as_bytes(), "{}", describe(w, d));
            }
            let i = super::numbering(w, f).object_of(s).expect("s is f's");
            let mut got = Stream::new();
            w.write_ref(&mut got, s);
            let mut want = Stream::new();
            want.uv(crate::refs::REF_OUTER).digest(&kf).uv(i);
            assert_eq!(got.as_bytes(), want.as_bytes());
            // From f's own stream: local, by ordinal.
            drop(w.take_unit());
            w.unit.local.holder = Some(f);
            let mut got = Stream::new();
            w.write_ref(&mut got, q);
            assert_eq!(got.as_bytes()[0], u8::try_from(crate::refs::REF_LOCAL).expect("a tag"));
            // In no unit's stream (a content digest): local.
            drop(w.take_unit());
            let mut got = Stream::new();
            w.write_ref(&mut got, q);
            assert_eq!(got.as_bytes()[0], u8::try_from(crate::refs::REF_LOCAL).expect("a tag"));
        });
    }

    // digest §4.3: what would be a derived unit were the type not local is
    // one (a closure's implicit conversion and invoker, a generic call
    // operator's specialisation); a capture field and a written member are
    // not.
    #[test]
    fn derived_units_under_a_local_type() {
        let src = "int g() { auto k = [] { return 1; }; int (*fp)() = k; struct L { int m; int h() { return m; } }; return k() + fp(); }\n\
                   int f(int a) { return [a] { return a; }(); }\n";
        with_walk(src, |w| {
            let ast = w.ast;
            let conv = of_kind(w, DeclKind::CXXConversion);
            assert!(!conv.is_empty());
            for d in conv {
                assert!(super::derived_under_local(w, d), "{}", describe(w, d));
            }
            let fields: Vec<_> = of_kind(w, DeclKind::Field)
                .into_iter()
                .filter(|&d| ast.decls.get(d).ctx().is_some_and(|c| w.is_local(c)))
                .collect();
            assert!(fields.len() >= 2);
            for d in fields {
                assert!(!super::derived_under_local(w, d), "{}", describe(w, d));
                assert!(crate::units::unit_class(w, d).is_none(), "{}", describe(w, d));
            }
            let h = named(w, "h", DeclKind::CXXMethod)[0];
            assert!(!super::derived_under_local(w, h));
            assert!(crate::units::unit_class(w, h).is_none());
            let ops: Vec<_> = of_kind(w, DeclKind::CXXMethod)
                .into_iter()
                .filter(|&d| ast.call_op_lambda(d).is_some())
                .collect();
            assert_eq!(ops.len(), 2);
            for d in ops {
                assert!(!super::derived_under_local(w, d), "{}", describe(w, d));
            }
        });
    }

    // digest §4.3 "Lambda-expressions inside a key or a type": walked
    // inside a key or a type digest, a lambda-expression writes `0` and its
    // cause, and does not walk the closure.
    #[test]
    fn a_lambda_in_a_key_or_a_type() {
        with_walk("int f() { return [] { struct In {}; return 1; }(); }", |w| {
            let ast = w.ast;
            let e = (1..=ast.exprs.len())
                .filter_map(|i| slip_kiln::kiln::ast::ExprId::try_new(u32::try_from(i).ok()?))
                .find(|&e| {
                    ast.exprs.get(e).kind() == slip_kiln::kiln::ast::expr::ExprKind::LambdaExpr
                })
                .expect("the lambda");
            for (in_key, in_type, cause) in [
                (1, 0, super::LAMBDA_IN_KEY),
                (0, 1, super::LAMBDA_IN_TYPE),
                (1, 1, super::LAMBDA_IN_KEY),
            ] {
                w.in_key = in_key;
                w.local.in_type = in_type;
                let mut s = Stream::new();
                super::lambda_expr(w, &mut s, e);
                assert_eq!(s.as_bytes(), &[0]);
                assert_eq!(s.cause(), Some(cause));
                assert!(w.local.written.is_empty());
            }
            w.in_key = 0;
            w.local.in_type = 0;
            drop(w.take_unit());
            let mut s = Stream::new();
            super::lambda_expr(w, &mut s, e);
            assert_eq!(s.cause(), None);
            assert_eq!(w.local.written.len(), 2, "the closure and the class in its body");
        });
    }

    // digest-kinds §5.4 `flags-scope-depth` (the pair test cannot change it
    // alone): a lambda's parameter in a default argument is one prototype
    // scope deeper than one in a body, and its stream says so.
    #[test]
    fn a_lambda_parameter_in_a_default_argument_is_deeper() {
        let src = "void f(int a = [](int b) { return b; }(1));\nvoid g() { [](int c) { return c; }(1); }\n";
        with_walk(src, |w| {
            let ast = w.ast;
            let b = named(w, "b", DeclKind::ParmVar)[0];
            let c = named(w, "c", DeclKind::ParmVar)[0];
            let depth =
                |d| ast.decls.param(d).map(slip_kiln::kiln::ast::decl::ParamData::scope_depth);
            assert_eq!((depth(b), depth(c)), (Some(1), Some(0)));
            let mut sb = Stream::new();
            w.decl_stream(&mut sb, b, b);
            drop(w.take_unit());
            let mut sc = Stream::new();
            w.decl_stream(&mut sc, c, c);
            assert_ne!(sb.as_bytes(), sc.as_bytes());
        });
    }

    // digest §4.3 "Things to settle" (WP-040): the owners of the places
    // Kiln's test leaves out, and whether the owner's numbering finds the
    // closure (`true`) or not (its uses are `UndeclaredLocal`).
    #[test]
    fn the_owner_of_each_closure_more() {
        const SRC: &str = "struct T { static int s; };\n\
        int T::s = [] { return 1; }();\n\
        struct BF { int b : sizeof([] {}); };\n\
        enum EN { e1 = sizeof([] {}) };\n\
        struct BS : decltype([] {}) {};\n\
        template <class T> struct BT : decltype([] {}) {};\n\
        template <class T> void fdi(int a = [] { return 1; }()) {}\n\
        void ud() { fdi<int>(); }\n\
        template <class U, int N = sizeof([] { return 1; }())> void tf(U) {}\n\
        void ut() { tf(1); tf(2L); }\n\
        void k() { auto o = [y = [] { return 1; }] { return y(); }; static_assert(sizeof([] {}) > 0); }\n\
        int pv = [] { return 2; }();\n";
        let got = run(SRC, |w, _, _| {
            let ast = w.ast;
            let mut out = Vec::new();
            for (line, o) in owners(w) {
                out.push((line, o));
            }
            let mut found = Vec::new();
            for i in 1..=ast.exprs.lambdas.len() {
                let l = ast.exprs.lambdas.get(LambdaId::new(u32::try_from(i).expect("an index")));
                found.push(super::type_number(w, l.closure).map_err(|c| format!("{c:?}")));
            }
            (out, found)
        });
        let got: Vec<(u32, &str, String)> = got
            .0
            .iter()
            .zip(&got.1)
            .map(|((l, o), n)| (*l, o.as_str(), format!("{n:?}")))
            .collect();
        let closure_owner = "Unsupported(\"local/closure-owner\")";
        let unnumbered = "Err(\"UndeclaredLocal\")".to_owned();
        let want: Vec<(u32, &str, String)> = vec![
            // A static member defined outside its class: the variable.
            (2, "Var s", "Ok(0)".into()),
            // A bit-field width: the field.
            (3, "Field b", "Ok(0)".into()),
            // An enumerator and a base-specifier: no owner with a stream.
            (4, closure_owner, format!("Err({closure_owner:?})")),
            (5, closure_owner, format!("Err({closure_owner:?})")),
            // A class template's base-specifier: the pattern is the
            // closure's context, not an owner.
            (6, closure_owner, format!("Err({closure_owner:?})")),
            // A default argument of a template: the pattern's closure is
            // the template's; the one Kiln instantiates for `fdi<int>` is
            // the specialisation's, whose stream does not walk it.
            (7, "FunctionTemplate fdi", "Ok(0)".into()),
            (7, "Function fdi spec", unnumbered.clone()),
            // A default template argument: one closure for the pattern,
            // which the template's stream walks (its head's default, in
            // full, WP-036), and one per use, which no stream walks.
            (9, "FunctionTemplate tf", "Ok(0)".into()),
            (9, "FunctionTemplate tf", unnumbered.clone()),
            (9, "FunctionTemplate tf", unnumbered),
            // An init-capture's initialiser and a block-scope
            // `static_assert`: the function.
            (11, "Function k", "Ok(1)".into()),
            (11, "Function k", "Ok(0)".into()),
            (11, "Function k", "Ok(2)".into()),
            (12, "Var pv", "Ok(0)".into()),
        ];
        assert_eq!(got, want);
    }

    // digest §4.3 "Local objects of another unit": a specialisation names
    // a parameter of its pattern (an expansion statement's pattern):
    // `Ref::outer` with the parameter's position (`direct_param`), which is
    // what the numbering gives.
    #[test]
    fn a_pattern_parameter_named_by_a_specialisation() {
        let src = "struct P { int x, y; };\n\
            template <class T> void f(int a, T t) { template for (int e : t) ; }\n\
            template void f<P>(int, P);\n";
        let (late, causes, direct, numbered) = run(src, |w, _, out| {
            let ast = w.ast;
            let t = ast
                .decls
                .ids()
                .find(|&d| {
                    ast.decls.get(d).kind() == DeclKind::ParmVar
                        && ast
                            .decls
                            .get(d)
                            .name()
                            .as_ident()
                            .is_some_and(|s| ast.symbols().as_str(s) == "t")
                        && super::owner(w, d)
                            .is_some_and(|o| ast.decls.get(o).kind() == DeclKind::FunctionTemplate)
                })
                .expect("the pattern's t");
            let o = super::owner(w, t).expect("an owner");
            let late = w.local.late;
            let causes: Vec<Cause> = out.causes.iter().map(|(_, c)| *c).collect();
            let direct = super::direct_param(w, o, t);
            // Since WP-045 `number_all` numbers the owner of an expanded
            // expansion statement's variable (the specialisation's
            // `definition` names it): `direct_param` agrees with it.
            assert!(w.local.numberings.contains_key(&o), "numbered by number_all");
            w.local.sealed = false;
            let numbered = super::numbering(w, o).object_of(t).ok();
            (late, causes, direct, numbered)
        });
        assert_eq!(late, 0);
        assert!(causes.iter().all(|c| matches!(c, Cause::Pending(_))), "{causes:?}");
        assert_eq!(direct, Some(1));
        assert_eq!(numbered, Some(1));
    }

    // `direct_param` answers for the owner's own prototype only: not for a
    // parameter of a function type in it (scope depth 1).
    #[test]
    fn a_nested_prototype_parameter_is_not_direct() {
        let got = run("void f(int a, void (*g)(int b)) {}\n", |w, _, _| {
            let ast = w.ast;
            let p = |n: &str| {
                ast.decls
                    .ids()
                    .find(|&d| {
                        ast.decls.get(d).kind() == DeclKind::ParmVar
                            && ast
                                .decls
                                .get(d)
                                .name()
                                .as_ident()
                                .is_some_and(|s| ast.symbols().as_str(s) == n)
                    })
                    .expect("the parameter")
            };
            let f = ast
                .decls
                .ids()
                .find(|&d| ast.decls.get(d).kind() == DeclKind::Function)
                .expect("f");
            let f = w.entity(f);
            [p("a"), p("g"), p("b")].map(|d| super::direct_param(w, f, d))
        });
        assert_eq!(got, [Some(0), Some(1), None]);
    }

    // WP-036's open point (WP-040 brief): Kiln gives a requires-expression's
    // parameter in a body the function-scope depth of the function's own
    // (`kiln_sema/src/decl.rs:643`: both 0 here), so `Ref::param` (depth,
    // index) of `y` and of `a` would be one; a reference to either from a
    // profile they are declared outside of must not make two different
    // bodies one digest.
    #[test]
    fn a_requires_parameter_is_not_a_function_parameter() {
        for shape in [
            "template <class T> void h(T a) {{ (void)requires(T y) {{ requires requires(T z) {{ z + {r}; }}; }}; }}\n",
            "template <class T> struct X {{}};\ntemplate <class T> void h(T a) {{ (void)requires(T y) {{ typename X<decltype({r})>; }}; }}\n",
            "template <class T> void h(T a) {{ (void)requires(T y) {{ {{ {r} + 1 }} noexcept; }}; }}\n",
            "template <class T> void h(T a) {{ (void)requires(T y) {{ sizeof(decltype({r} + 1)); }}; }}\n",
            // In the key: a trailing requires-clause (Kiln: `y` at depth 1).
            "template <class T> void h(T a) requires requires(T y) {{ requires requires(T z) {{ z + {r}; }}; }} {{}}\n",
        ] {
            let side = |r: &str| {
                let src = shape.replace("{r}", r).replace("{{", "{").replace("}}", "}");
                run(&src, |_, written, _| keys_and_digests(written))
            };
            let (a, b) = (side("y"), side("a"));
            let same: Vec<_> = a.iter().filter(|u| b.contains(u) && u.2.is_none()).collect();
            assert!(
                a.len() == b.len() && same.len() < a.len(),
                "one digest for two bodies: {shape}"
            );
        }
    }
}
