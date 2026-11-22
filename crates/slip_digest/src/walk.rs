//! `Walk`, the context of one walk, and `run`, which drives it.

use std::collections::{HashMap, HashSet};

use slip_kiln::kiln::ast::Ast;
use slip_kiln::kiln::ast::ids::DeclId;
use slip_kiln::kiln::source::SourceManager;
use slip_model::Digest;
use slip_model::keys::RuleTable;

use crate::keys::KeyRes;
use crate::stream::Hashed;
use crate::types::TypeRef;
use crate::units::{UnitClass, Written};
use crate::{Cause, Input, Options, Output};

/// How deep types, keys and declarations walked in place may nest before
/// the unit is indeterminate (`Cause::Unaccounted("walk/depth")`). The
/// closure `slip_kiln::snippet` runs, and the worker's walk, run outside
/// Kiln's stack guard: this bound is what keeps the walk's own stack
/// finite (WP-024 "what bounds the stack"). The stack 200 levels take is
/// measured with [`NODE_LIMIT`]'s (`lib.rs` "What bounds the stack").
pub(crate) const DEPTH_LIMIT: u32 = 200;

/// The cause of a walk deeper than [`DEPTH_LIMIT`] or [`NODE_LIMIT`].
pub(crate) const TOO_DEEP: Cause = Cause::Unaccounted("walk/depth");

/// How deep statements and expressions may nest through Rust recursion
/// before the unit is indeterminate ([`TOO_DEEP`]): each
/// [`Walk::enter_node`] is one level. The expression walk does not
/// recurse for the children of the kinds it writes itself (`expr.rs`
/// "The walk"), so an operator chain or a call chain of any length takes
/// one level; what recurses is a kind function that calls
/// `expr::child` itself, and the statements. The stack this needs is
/// [`STACK_SIZE`] (`lib.rs` "What bounds the stack").
pub(crate) const NODE_LIMIT: u32 = 10_000;

/// The stack `crate::digest` needs, in bytes (`lib.rs` "What bounds the
/// stack"): [`NODE_LIMIT`] levels of statement or expression recursion
/// and [`DEPTH_LIMIT`] levels of types, keys and declarations at the
/// frame sizes `expr::tests::frames_fit_the_stack` measures (about 9.0 MB
/// in the test profile), with a margin of more than four, which that test
/// checks.
pub const STACK_SIZE: usize = 64 << 20;

/// The state of the unit being written (`spec/digest.md` §4.3).
#[derive(Debug, Default)]
pub(crate) struct UnitState {
    /// Ordinals of the parameters, variables, bindings and labels met.
    pub locals: HashMap<DeclId, u64>,
    pub next_local: u64,
    /// Bumped whenever a stream writes a reference to a local entity of
    /// this unit: a type digest that did is memoised for this unit only
    /// (§4.2).
    pub local_uses: u64,
    /// Type digests that hold a reference to a local entity of this unit.
    pub types: HashMap<u64, TypeRef>,
    /// The expression walk's state.
    pub expr: ExprState,
}

/// The state of the expression walk in a unit (`expr.rs`): put aside with
/// the rest of [`UnitState`] for a key, a content digest or a unit
/// ([`Walk::take_unit`]), and alone for a type digest
/// ([`Walk::take_expr_state`]), whose bytes must not depend on the unit
/// that asks first.
#[derive(Debug, Default)]
pub(crate) struct ExprState {
    /// The number of each opaque value's source, in the order the walk
    /// numbers it (digest-kinds §7, "Opaque values"), and the next number.
    pub opaques: HashMap<slip_kiln::kiln::ast::ExprId, u64>,
    pub next_opaque: u64,
    /// How many instantiation-dependent expressions enclose the node the
    /// expression walk is writing, itself included
    /// (`expr_core::this`).
    pub dependent: u32,
    /// The numbering of the profile being walked (`expr_dep::profile_expr`,
    /// digest-kinds §4.4); `None` outside one.
    pub profile: Option<crate::expr_dep::Frame>,
}

impl ExprState {
    /// Nothing numbered and nothing around: the state a type digest
    /// starts from.
    pub(crate) fn is_fresh(&self) -> bool {
        self.opaques.is_empty()
            && self.next_opaque == 0
            && self.dependent == 0
            && self.profile.is_none()
    }
}

/// The context of one walk: the memo, the key table, the worklist and the
/// unit being written.
pub(crate) struct Walk<'a> {
    pub ast: &'a Ast,
    pub sm: &'a SourceManager,
    pub subject_path: &'a dyn Fn(slip_kiln::kiln::source::FileId) -> Vec<u8>,
    pub rules: &'a RuleTable,
    /// Type digests by the raw canonical `QualType`.
    pub type_memo: HashMap<u32, TypeRef>,
    /// Digests of function types written without their exception
    /// specification (`DeclNode` `ty-type`), by raw canonical `QualType`.
    pub type_memo_no_exc: HashMap<u32, TypeRef>,
    /// Keys by entity (its first declaration).
    pub keys: HashMap<DeclId, KeyRes>,
    /// Entities whose key is being computed: a cycle is `Unaccounted`.
    pub keys_in_progress: HashSet<DeclId>,
    /// The unit digests of content-keyed entities, needed by their keys.
    pub content: HashMap<DeclId, Hashed>,
    pub content_in_progress: HashSet<DeclId>,
    /// Content digests of the constants a pointer names (`PtrBase::Const`),
    /// by `ConstValueId` index (`value::const_digest`).
    pub const_memo: HashMap<u32, Hashed>,
    /// The scopes whose unnamed types and fields have been numbered
    /// (`spec/digest.md` §3.3), and each one's ordinal.
    pub ordinal_scopes: HashSet<DeclId>,
    pub ordinal_of: HashMap<DeclId, u64>,
    /// Declarations of a kind with no key kind, with the entity around
    /// them (`None`: the top level) and their cause (`units::apply_taints`).
    pub taints: Vec<(Option<DeclId>, Cause)>,
    /// Entities queued as units.
    pub queued: HashSet<DeclId>,
    /// The worklist, in the order units are written.
    pub queue: Vec<(DeclId, UnitClass)>,
    pub unit: UnitState,
    /// How many key computations are under way: a key is computed with
    /// the unit's state put aside, and nothing in it may take a local
    /// ordinal (`write_ref`).
    pub in_key: u32,
    pub depth: u32,
    /// The levels of statement and expression recursion entered
    /// ([`Walk::enter_node`]).
    pub node_depth: u32,
    /// How many times the depth limit was hit or a cycle was met (a key or
    /// a content digest that needs itself): a memo entry is not kept for a
    /// result computed below such a hit (its cause depends on the path
    /// that reached it).
    pub aborts: u64,
    /// `digest.bytes` (`spec/digest.md` §12).
    pub bytes: u64,
}

impl std::fmt::Debug for Walk<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Walk").field("queue", &self.queue.len()).finish_non_exhaustive()
    }
}

impl<'a> Walk<'a> {
    pub(crate) fn new(input: &Input<'a>, rules: &'a RuleTable) -> Walk<'a> {
        Walk {
            ast: input.ast,
            sm: input.sm,
            subject_path: input.subject_path,
            rules,
            type_memo: HashMap::new(),
            type_memo_no_exc: HashMap::new(),
            keys: HashMap::new(),
            keys_in_progress: HashSet::new(),
            content: HashMap::new(),
            content_in_progress: HashSet::new(),
            const_memo: HashMap::new(),
            ordinal_scopes: HashSet::new(),
            ordinal_of: HashMap::new(),
            taints: Vec::new(),
            queued: HashSet::new(),
            queue: Vec::new(),
            unit: UnitState::default(),
            in_key: 0,
            depth: 0,
            node_depth: 0,
            aborts: 0,
            bytes: 0,
        }
    }

    /// Enters one level of nesting; `false` (and the level is not
    /// entered) past [`DEPTH_LIMIT`].
    pub(crate) fn enter(&mut self) -> bool {
        stack_probe();
        if self.depth >= DEPTH_LIMIT {
            self.aborts += 1;
            return false;
        }
        self.depth += 1;
        true
    }

    pub(crate) fn leave(&mut self) {
        debug_assert!(self.depth > 0, "leave without enter");
        self.depth -= 1;
    }

    /// Enters one level of statement or expression recursion; `false`
    /// (and the level is not entered) past [`NODE_LIMIT`]. The caller then
    /// writes a placeholder and makes the stream indeterminate with
    /// [`TOO_DEEP`].
    pub(crate) fn enter_node(&mut self) -> bool {
        stack_probe();
        if self.node_depth >= NODE_LIMIT {
            self.aborts += 1;
            return false;
        }
        self.node_depth += 1;
        true
    }

    pub(crate) fn leave_node(&mut self) {
        debug_assert!(self.node_depth > 0, "leave_node without enter_node");
        self.node_depth -= 1;
    }

    /// The unit's state, put aside for a computation that must not depend
    /// on the unit that asks for it (a key, a content digest, a unit),
    /// leaving a fresh one; the caller puts it back.
    pub(crate) fn take_unit(&mut self) -> UnitState {
        std::mem::take(&mut self.unit)
    }

    /// The expression walk's state alone, put aside for a type digest
    /// (memoised across units); the unit's locals stay, since a type may
    /// name one (§4.2). The caller puts it back.
    pub(crate) fn take_expr_state(&mut self) -> ExprState {
        std::mem::take(&mut self.unit.expr)
    }

    /// Counts the bytes of a hashed stream.
    pub(crate) fn hashed(&mut self, h: Hashed) -> Hashed {
        self.bytes += h.bytes;
        h
    }

    /// Queues `e` as a unit of class `class` unless it is queued already.
    pub(crate) fn enqueue(&mut self, e: DeclId, class: UnitClass) {
        if self.queued.insert(e) {
            self.queue.push((e, class));
        }
    }
}

#[cfg(test)]
thread_local! {
    /// The lowest stack address [`stack_probe`] saw on this thread, and
    /// the address of each depth of [`Walk::enter`] first seen: what the
    /// frame-size test reads (`expr::tests`).
    pub(crate) static STACK_LOW: std::cell::Cell<usize> = const { std::cell::Cell::new(usize::MAX) };
}

/// Records how deep the stack is here (tests only; nothing otherwise).
#[inline]
pub(crate) fn stack_probe() {
    #[cfg(test)]
    {
        let here = 0u8;
        let at = std::ptr::addr_of!(here) as usize;
        STACK_LOW.with(|low| {
            if at < low.get() {
                low.set(at);
            }
        });
    }
}

/// The walk: units, then collisions, groups, counters, causes and names.
pub(crate) fn run(input: &Input<'_>, options: &Options<'_>) -> Output {
    let mut w = Walk::new(input, options.rules);
    crate::units::collect(&mut w);
    let mut written: Vec<Written> = Vec::new();
    let mut i = 0;
    while i < w.queue.len() {
        let (e, class) = w.queue[i];
        i += 1;
        if let Some(u) = crate::units::write(&mut w, e, class) {
            written.push(u);
        }
    }
    written.push(crate::units::order_unit(&mut w));
    crate::units::apply_taints(&w, &mut written);
    let collisions = crate::units::mark_collisions(&mut written);
    let names = if options.names { crate::names::names(&w, &written) } else { Vec::new() };
    let (digests, totals) = crate::groups::build(&written, &options.detail);
    let causes = causes(&written);
    let counters = crate::groups::counters(&totals, collisions, w.bytes, &causes);
    Output { digests, counters, causes, names }
}

/// One cause per indeterminate key: of the unit first in `(key, digest)`
/// order.
fn causes(written: &[Written]) -> Vec<(Digest, Cause)> {
    let mut out: Vec<(Digest, Digest, Cause)> =
        written.iter().filter_map(|u| u.cause.map(|c| (u.key, u.digest, c))).collect();
    out.sort();
    out.dedup_by(|b, a| a.0 == b.0);
    out.into_iter().map(|(k, _, c)| (k, c)).collect()
}
