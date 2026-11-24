//! `Walk`, the context of one walk, and `run`, which drives it.

use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};

use slip_kiln::kiln::ast::Ast;
use slip_kiln::kiln::ast::RawId;
use slip_kiln::kiln::ast::ids::DeclId;
use slip_kiln::kiln::source::SourceManager;
use slip_model::Digest;
use slip_model::hash::tagged_digest;
use slip_model::keys::RuleTable;
use slip_model::wire::Encoder;

use crate::keys::KeyRes;
use crate::stream::{Hashed, Stream};
use crate::types::TypeRef;
use crate::units::{UnitClass, Written};
use crate::{Cause, Input, NamesUnit, Options, Output};

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
    /// The left-out facts of the unit's own spellings, in walk order
    /// (`own_spelling.rs`), and whether one of them makes the list due.
    pub facts: Vec<crate::own_spelling::Fact>,
    pub facts_due: bool,
    /// How many expressions the unit's reading of its spellings has met
    /// (`own_spelling::Fact::Round`).
    pub roots: u64,
    /// The unit is a resolved unit `definition` (`demand.rs`), whose
    /// stream reads every declaration of its entity, those an
    /// instantiation made out of line included (`Walk::redecls`).
    pub definition: bool,
    /// The stream's holder and the local types it declared
    /// (`local::UnitLocals`).
    pub local: crate::local::UnitLocals,
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
    /// Where the walk records the `default_imm` values it meets, when a
    /// `default_immediates` unit asks (`demand::immediates`).
    pub imms: Option<Vec<slip_kiln::kiln::ast::ConstValueId>>,
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
    /// Which files are read-only, read under a non-empty table only
    /// (`owned.rs`, WP-110).
    pub read_only: crate::ReadOnly<'a>,
    /// Ownership of entities and namespace views (`owned.rs`).
    pub owned_state: crate::owned::OwnedState,
    /// Type digests by the raw canonical `QualType`.
    pub type_memo: HashMap<u32, TypeRef>,
    /// Digests of function types written without their exception
    /// specification (`DeclNode` `ty-type`), by raw canonical `QualType`.
    pub type_memo_no_exc: HashMap<u32, TypeRef>,
    /// The typedefs carrying an alignment that layout reads through a
    /// sugared type (digest-kinds §4.2 rule 9), by raw `QualType`;
    /// `None` past the bound of the walk.
    pub aligned_sugar: HashMap<u32, Option<Box<[DeclId]>>>,
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
    /// Whether the own spelling of a typedef-name holds a left-out fact
    /// (`own_spelling.rs`), by its first declaration.
    pub alias_due: HashMap<DeclId, bool>,
    /// How many profiles the unit writes straight into its stream are
    /// being walked (`expr_dep::profile_expr`): their facts are read by
    /// that site, not by `absorb_type` for the types inside them.
    pub spelling_off: u32,
    /// Numberings of owners and the numbering mode (`local.rs`).
    pub local: crate::local::LocalState,
    /// The temporaries each variable extends, by its first declaration,
    /// built when a `value` unit first needs it (`demand.rs`).
    pub lets: Option<HashMap<DeclId, Vec<(u32, DeclId)>>>,
    /// The spellings of the operators some declaration that is not a
    /// member function names (`operator+`), built when a fold first needs
    /// it (`expr_profile::fold_left_out`).
    pub declared_operators: Option<HashSet<&'static str>>,
    /// The objects of the store that hold values of digest §9 (the
    /// source-location strings and objects), built when a constant first
    /// needs it (`value.rs`, WP-104).
    pub name_values: Option<crate::value::NameValues>,
    /// Above 0, a name-derived value is written by value, not
    /// symbolically (digest §9: where what is written is read as a
    /// constant whose result the walk does not see): in a key, in a type
    /// digest, in a block-scope variable's evaluated value and in a
    /// `static_assert` condition (`value.rs` "Values that come from
    /// names").
    pub by_value: u32,
    /// The `ConstantExpr`s the walk is inside (digest §9 "Defaults at a
    /// use"): a use of a default under one is an operand of an immediate
    /// invocation evaluated where it is written, whose value is written
    /// as every constant is, and contributes nothing more. Set to 0 around
    /// a local or closure type's stream (`local::local_type`): its
    /// members' bodies are evaluation contexts of their own.
    pub in_constant: u32,
    /// What each default expression reaches (digest §9 "Defaults at a
    /// use"), by the default's `ExprId`: it does not depend on the use,
    /// so each default is walked once (`expr_misc::reach_default`).
    pub default_reach: HashMap<slip_kiln::kiln::ast::ExprId, crate::expr_misc::Reached>,
    /// [`Walk::const_memo`] for the content digests computed with
    /// [`Walk::by_value`] above 0.
    pub const_memo_by_value: HashMap<u32, Hashed>,
    /// How each rule was used (`rule_use.rs`): `None` unless asked for
    /// under a table that is not empty.
    pub rule_use: Option<crate::rule_use::Counting>,
    /// The names digest being fed (digest §9 "The names digest", WP-112):
    /// the entries of the unit (or constant) being written, `None` where
    /// nothing is collected ([`Walk::feed`]).
    pub names: Option<Encoder>,
    /// The names digests of the constants a pointer names, by
    /// `ConstValueId` index, for the content digests of
    /// [`Walk::const_memo`] (computed with [`Walk::by_value`] at 0); a
    /// constant with no entry has none (`value::const_names`).
    pub const_names: HashMap<u32, Digest>,
}

/// `H("slip.names.v1", entries)`: the names digest of a unit or a constant
/// (digest §9 "The names digest").
pub(crate) const NAMES_TAG: &str = "slip.names.v1";

/// `H("slip.names.key.v1", count, digests)`: the names digest of a key
/// with several units that have one (`spec/model.md` §8.7).
pub(crate) const NAMES_KEY_TAG: &str = "slip.names.key.v1";

/// What [`Walk::take_types`] puts aside.
pub(crate) struct TypeState {
    left_out: u64,
    type_nesting: u32,
    spelling_off: u32,
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
            read_only: crate::ReadOnly::EVERY_FILE,
            owned_state: crate::owned::OwnedState::default(),
            type_memo: HashMap::new(),
            type_memo_no_exc: HashMap::new(),
            aligned_sugar: HashMap::new(),
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
            alias_due: HashMap::new(),
            spelling_off: 0,
            local: crate::local::LocalState::default(),
            lets: None,
            declared_operators: None,
            name_values: None,
            by_value: 0,
            in_constant: 0,
            default_reach: HashMap::new(),
            const_memo_by_value: HashMap::new(),
            rule_use: None,
            names: None,
            const_names: HashMap::new(),
        }
    }

    /// Where an entry of the names digest is written (digest §9 "The names
    /// digest"): the encoder of the unit or constant being written, when
    /// one is collecting, name-derived values are written symbolically
    /// ([`Walk::by_value`] at 0) and no owner is being numbered; else
    /// `None`, and nothing is fed. A key and a type digest raise
    /// `by_value`, so nothing they write is fed: they are memoised
    /// across units.
    pub(crate) fn feed(&mut self) -> Option<&mut Encoder> {
        if self.by_value > 0 || crate::local::numbering_mode(self) {
            return None;
        }
        self.names.as_mut()
    }

    /// Starts collecting a names digest, and returns what was being
    /// collected, which [`Walk::names_end`] puts back.
    pub(crate) fn names_start(&mut self) -> Option<Encoder> {
        self.names.replace(Encoder::default())
    }

    /// Stops collecting (for a stream whose strings are not the unit's:
    /// a candidate stream); [`Walk::names_end`] puts back what was being
    /// collected.
    pub(crate) fn names_off(&mut self) -> Option<Encoder> {
        self.names.take()
    }

    /// Ends what [`Walk::names_start`] or [`Walk::names_off`] began,
    /// putting `saved` back: the names digest collected meanwhile, `None`
    /// when nothing was fed.
    pub(crate) fn names_end(&mut self, saved: Option<Encoder>) -> Option<Digest> {
        let mine = std::mem::replace(&mut self.names, saved)?;
        (!mine.as_bytes().is_empty()).then(|| tagged_digest(NAMES_TAG, mine.as_bytes()))
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
    /// marked does not depend on which was computed
    /// first (WP-036, F-G). The caller puts it back with
    /// [`Walk::restore_types`].
    pub(crate) fn take_types(&mut self) -> TypeState {
        TypeState {
            left_out: self.left_out,
            type_nesting: std::mem::take(&mut self.type_nesting),
            spelling_off: std::mem::take(&mut self.spelling_off),
            abort_memo: std::mem::take(&mut self.abort_memo),
        }
    }

    /// Puts back what [`Walk::take_types`] put aside.
    pub(crate) fn restore_types(&mut self, t: TypeState) {
        self.left_out = t.left_out;
        self.type_nesting = t.type_nesting;
        self.spelling_off = t.spelling_off;
        self.abort_memo = t.abort_memo;
    }

    /// The expression walk's state alone, put aside for a type digest
    /// (memoised across units); the unit's locals stay, since a type may
    /// name one (§4.2). The caller puts it back.
    pub(crate) fn take_expr_state(&mut self) -> ExprState {
        std::mem::take(&mut self.unit.expr)
    }

    /// Whether what is written now goes straight into a unit stream: not
    /// inside a key, a type digest or a profile written into the unit
    /// (the places a unit reads its own spellings, `own_spelling.rs`).
    pub(crate) fn unit_level(&self) -> bool {
        self.in_key == 0 && self.type_nesting == 0 && self.spelling_off == 0
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
    run_kept(input, options, &[], f)
}

/// [`run_with`], counting rule use with `Declared.kept` (`kept`,
/// ascending; `spec/verify.md` §9.2).
pub(crate) fn run_kept<R>(
    input: &Input<'_>,
    options: &Options<'_>,
    kept: &[Digest],
    f: impl FnOnce(&mut Walk<'_>, &[Written], Output) -> R,
) -> R {
    let mut w = Walk::new(input, options.rules);
    w.read_only = options.read_only;
    if options.rule_use && !options.rules.is_empty() {
        w.rule_use = Some(crate::rule_use::Counting::new(kept));
        w.owned_state.record_parents = true;
    }
    crate::units::collect(&mut w);
    crate::local::number_all(&mut w);
    let mut written: Vec<Written> = Vec::new();
    // The names digests (digest §9 "The names digest"): by the index in
    // `written` of the entity's unit, which its resolved units' strings
    // are fed to as well.
    let mut fed: Vec<(usize, Digest)> = Vec::new();
    let mut i = 0;
    while i < w.queue.len() {
        let (e, class) = w.queue[i];
        i += 1;
        let saved = w.names_start();
        if let Some(u) = crate::units::write(&mut w, e, class) {
            // Its resolved units (`demand.rs`), after it.
            let mut resolved = Vec::new();
            crate::demand::resolved_units(&mut w, e, &u, &mut resolved);
            if let Some(d) = w.names_end(saved) {
                fed.push((written.len(), d));
            }
            written.push(u);
            written.append(&mut resolved);
        } else {
            w.names_end(saved);
        }
    }
    written.push(crate::units::order_unit(&mut w));
    crate::units::apply_taints(&w, &mut written);
    crate::units::view_units(&mut w, &mut written);
    let collisions = crate::units::mark_collisions(&mut written)
        + crate::keys::mark_mapped_collisions(&w, input, &mut written);
    let collisions = collisions + crate::units::mark_part_collisions(&mut written);
    let names = if options.names { crate::names::names(&w, &written) } else { Vec::new() };
    let (digests, totals) = crate::groups::build(&written, &options.detail);
    let causes = causes(&written);
    let counters = crate::groups::counters(&totals, collisions, w.bytes, &causes);
    let counting = w.rule_use.take();
    let name_strings = name_strings(&w, &written, &fed);
    // Last: the lists form keys of their own (`own_lists.rs`, inert).
    let (made, fixed) = crate::own_lists::lists(&mut w, counting.as_ref());
    let (rule_use, kept) = crate::rule_use::finish(counting);
    let output =
        Output { digests, counters, causes, names, rule_use, kept, name_strings, made, fixed };
    f(&mut w, &written, output)
}

/// `Output::name_strings` from the names digests `fed` (by index in
/// `written`): one entry per key, sorted by key. A key whose units have
/// one names digest has it; a key with several has
/// `H("slip.names.key.v1", count, digests)` of them in the order of its
/// units, by `(digest, names digest)` (`spec/model.md` §8.7). Its file and
/// name are those of the first of these units.
fn name_strings(w: &Walk<'_>, written: &[Written], fed: &[(usize, Digest)]) -> Vec<NamesUnit> {
    if fed.is_empty() {
        return Vec::new();
    }
    let mut by_key: Vec<(Digest, Digest, Digest, usize)> =
        fed.iter().map(|&(i, d)| (written[i].key, written[i].digest, d, i)).collect();
    by_key.sort_unstable();
    let firsts: Vec<Written> = by_key
        .iter()
        .enumerate()
        .filter(|&(j, x)| j == 0 || by_key[j - 1].0 != x.0)
        .map(|(_, x)| written[x.3].clone())
        .collect();
    let shown = crate::names::names(w, &firsts);
    let mut out: Vec<NamesUnit> = Vec::with_capacity(firsts.len());
    let mut j = 0;
    while j < by_key.len() {
        let key = by_key[j].0;
        let run = by_key[j..].iter().take_while(|x| x.0 == key).count();
        let names = if run == 1 {
            by_key[j].2
        } else {
            let mut e = Encoder::default();
            e.count(run);
            for x in &by_key[j..j + run] {
                e.digest(&x.2);
            }
            tagged_digest(NAMES_KEY_TAG, e.as_bytes())
        };
        let first = &written[by_key[j].3];
        let name = shown
            .binary_search_by(|(k, _)| k.cmp(&key))
            .map_or_else(|_| String::new(), |at| shown[at].1.clone());
        out.push(NamesUnit { key, names, file: first.file.clone(), name });
        j += run;
    }
    out
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
