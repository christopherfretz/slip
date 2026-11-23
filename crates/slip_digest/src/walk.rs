//! `Walk`, the context of one walk, and `run`, which drives it.

use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};

use slip_kiln::kiln::ast::Ast;
use slip_kiln::kiln::ast::RawId;
use slip_kiln::kiln::ast::ids::DeclId;
use slip_kiln::kiln::source::SourceManager;
use slip_model::Digest;
use slip_model::keys::RuleTable;

use crate::keys::KeyRes;
use crate::stream::{Hashed, Stream};
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
    /// The nodes written first, with the state they were written in and
    /// their stream, and each one's count (`spec/digest.md` §4.3
    /// "Back-references", [`Walk::back_ref`]).
    pub nodes: NodeMemo,
}

/// A statement or expression node, as the memo of back-references knows
/// it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NodeRef {
    Expr(slip_kiln::kiln::ast::ExprId),
    Stmt(slip_kiln::kiln::ast::StmtId),
}

/// The memo of back-references ([`Walk::back_ref`]): for each stream
/// that has written a node under this expression state, the nodes it
/// wrote first and their counts. The stream being written is kept apart
/// from the rest, so the common case (one stream at a time) is one map
/// lookup; a stream left for another (an element of a `*sorted` list
/// written alone, a reference's own stream) keeps its maps until it is
/// written again or the state is dropped.
#[derive(Clone, Debug, Default)]
pub(crate) struct NodeMemo {
    /// The identity of the stream `maps` belongs to; `None` before any.
    current: Option<u64>,
    maps: StateMaps,
    others: HashMap<u64, StateMaps, BuildHasherDefault<NodeHasher>>,
}

/// One stream's first writings: a map from a node's id to its count for
/// each state a row can read, indexed by `KEY_STMT | KEY_DEPENDENT |
/// KEY_IN_KEY`. Four bytes of key and four of count per node: the memo
/// is on the path of every node of every unit (WP-049 decision 7).
type StateMaps = [HashMap<u32, u32, BuildHasherDefault<NodeHasher>>; 8];

impl NodeMemo {
    pub(crate) fn is_empty(&self) -> bool {
        self.current.is_none() && self.others.is_empty()
    }

    /// The maps of stream `id`, made the current ones.
    fn maps(&mut self, id: u64) -> &mut StateMaps {
        if self.current != Some(id) {
            let maps = self.others.remove(&id).unwrap_or_default();
            let left = std::mem::replace(&mut self.maps, maps);
            if let Some(c) = self.current
                && left.iter().any(|m| !m.is_empty())
            {
                self.others.insert(c, left);
            }
            self.current = Some(id);
        }
        &mut self.maps
    }
}

/// A multiplicative hasher for the memo's keys, which are words the walk
/// makes (not input an adversary picks: a node id and a counter).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct NodeHasher(u64);

impl Hasher for NodeHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.write_u64(u64::from(b));
        }
    }

    fn write_u32(&mut self, v: u32) {
        self.write_u64(u64::from(v));
    }

    fn write_u64(&mut self, v: u64) {
        self.0 = (self.0.rotate_left(5) ^ v).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }
}

/// The bits of a memo's state index (`Walk::back_ref`).
const KEY_STMT: usize = 1;
const KEY_DEPENDENT: usize = 2;
const KEY_IN_KEY: usize = 4;

impl ExprState {
    /// Nothing numbered and nothing around: the state a type digest
    /// starts from.
    pub(crate) fn is_fresh(&self) -> bool {
        self.opaques.is_empty()
            && self.next_opaque == 0
            && self.dependent == 0
            && self.profile.is_none()
            && self.nodes.is_empty()
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
    /// How many facts a profile walk left out that its spelling holds
    /// (`expr_profile.rs`): a type digest whose stream raised it is
    /// marked (`TypeRef::spelling`).
    pub left_out: u64,
    /// How many `type_digest` calls are under way.
    pub type_nesting: u32,
    /// Type digests computed below a depth-limit hit or a cycle, by raw
    /// canonical `QualType` and whether the exception specification was
    /// left out, with whether they used a local entity: kept only for the
    /// outermost `type_digest` call (their cause depends on the path that
    /// reached them, which within one call is the same), so a type met
    /// again at the limit is not walked again (WP-036, F4).
    pub abort_memo: HashMap<(u32, bool), (TypeRef, bool)>,
    /// `digest.bytes` (`spec/digest.md` §12).
    pub bytes: u64,
}

/// What [`Walk::take_types`] puts aside.
pub(crate) struct TypeState {
    left_out: u64,
    type_nesting: u32,
    abort_memo: HashMap<(u32, bool), (TypeRef, bool)>,
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
            left_out: 0,
            type_nesting: 0,
            abort_memo: HashMap::new(),
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

    /// The type digests' state, put aside for a key or a content digest
    /// (memoised): what they compute neither depends on the enclosing
    /// type digest nor raises its `left_out`, so whether a unit is
    /// `Pending(Area::Spelling)` does not depend on which was computed
    /// first (WP-036, F-G). The caller puts it back with
    /// [`Walk::restore_types`].
    pub(crate) fn take_types(&mut self) -> TypeState {
        TypeState {
            left_out: self.left_out,
            type_nesting: std::mem::take(&mut self.type_nesting),
            abort_memo: std::mem::take(&mut self.abort_memo),
        }
    }

    /// Puts back what [`Walk::take_types`] put aside.
    pub(crate) fn restore_types(&mut self, t: TypeState) {
        self.left_out = t.left_out;
        self.type_nesting = t.type_nesting;
        self.abort_memo = t.abort_memo;
    }

    /// The expression walk's state alone, put aside for a type digest
    /// (memoised across units); the unit's locals stay, since a type may
    /// name one (§4.2). The caller puts it back.
    pub(crate) fn take_expr_state(&mut self) -> ExprState {
        std::mem::take(&mut self.unit.expr)
    }

    /// `spec/digest.md` §4.3 "Back-references": the count of `node` when
    /// stream `s` has written it already in the state the walk is in now,
    /// which the caller then writes as `tag BACKREF`, `uv` the count, in
    /// place of the node. Otherwise `None`, and `node` is counted as a
    /// first writing of `s` in this state, unless a profile is being
    /// walked (outside the memo: neither counted nor referred to).
    ///
    /// The state is what any row reads of the walk besides the node
    /// (`spec/landed/digest-kinds/WP-049.md` "The state"): whether an
    /// instantiation-dependent expression encloses the node and whether a
    /// key is being computed. The memo lives in [`ExprState`], so it is
    /// put aside and fresh with it for a type digest, a profile, a key, a
    /// content digest and a unit; the stream's identity keeps a
    /// back-reference inside the stream that wrote the node, so a stream
    /// written alone (an element of a `*sorted` list, a discriminator)
    /// neither names nor is named by another.
    pub(crate) fn back_ref(&mut self, s: &mut Stream, node: NodeRef) -> Option<u64> {
        #[cfg(test)]
        if NO_BACKREFS.with(std::cell::Cell::get) {
            return None;
        }
        let expr = &mut self.unit.expr;
        if expr.profile.is_some() {
            return None;
        }
        let (id, mut state) = match node {
            NodeRef::Expr(e) => (e.raw(), 0),
            NodeRef::Stmt(st) => (st.raw(), KEY_STMT),
        };
        if expr.dependent > 0 {
            state |= KEY_DEPENDENT;
        }
        if self.in_key > 0 {
            state |= KEY_IN_KEY;
        }
        match expr.nodes.maps(s.id())[state].entry(id) {
            std::collections::hash_map::Entry::Occupied(n) => {
                #[cfg(test)]
                BACKREFS.with(|c| c.set(c.get() + 1));
                Some(u64::from(*n.get()))
            }
            std::collections::hash_map::Entry::Vacant(v) => {
                // A count past `u32` (never met: a stream would hold four
                // billion nodes) is counted and not remembered: the node
                // is written in full if reached again, which is the same
                // for every writing of the stream.
                if let Ok(n) = u32::try_from(s.count_node()) {
                    v.insert(n);
                }
                None
            }
        }
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
#[path = "backref_tests.rs"]
mod backref_tests;

#[cfg(test)]
thread_local! {
    /// How many back-references [`Walk::back_ref`] gave on this thread
    /// (`backref_tests`).
    pub(crate) static BACKREFS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    /// Set: [`Walk::back_ref`] gives none and counts nothing, for a test
    /// that measures an encoding as it would be without back-references.
    static NO_BACKREFS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Runs `f` with back-references off on this thread (`NO_BACKREFS`).
#[cfg(test)]
pub(crate) fn without_back_refs<T>(f: impl FnOnce() -> T) -> T {
    NO_BACKREFS.with(|c| c.set(true));
    let t = f();
    NO_BACKREFS.with(|c| c.set(false));
    t
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
    run_with(input, options, |_, _, o| o)
}

/// The walk, with its context and its written units still at hand.
pub(crate) fn run_with<R>(
    input: &Input<'_>,
    options: &Options<'_>,
    f: impl FnOnce(&mut Walk<'_>, &[Written], Output) -> R,
) -> R {
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
    f(&mut w, &written, Output { digests, counters, causes, names })
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
