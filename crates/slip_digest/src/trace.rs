//! Traces of a unit's stream (`spec/verify.md` §6.1 "Traces", work
//! package WP-054): where in a unit's bytes the walk began each
//! statement and expression, and which of its digest fields are keys of
//! units of the image.
//!
//! Outside the trusted base in what it says (no verdict depends on a
//! place), inside it in what it touches. The rule every line keeps:
//! **tracing changes no digest.**
//!
//! # How a trace is made
//!
//! [`trace`] runs the walk as [`crate::digest`] does (`walk::run_with`),
//! so `Traced::output` is the digest's output by construction. Then, the
//! walk finished and every memo it fills filled, each wanted unit's stream
//! is written a **second time** into a stream that records
//! (`stream::Stream::traced`): the offset of each statement and
//! expression the walk begins (the hooks of `expr::write_node` and
//! `stmt::node`) and of each `digest` field. The bytes written the second
//! time are a trace only if they hash to the unit's digest
//! ([`UNIT_TAG`]); otherwise the unit has no trace (§6.1: a unit's stream
//! must not depend on when it is written, and one that does is a defect
//! of the walk, which `crates/slip_digest/tests/trace.rs` reports).
//!
//! The second writing repeats the first one's own lines, with the state
//! that writing puts aside put aside again:
//!
//! - a unit of an entity (`units::write`): the declaration it is read
//!   from (`Written::read` for an entity declared only by instantiations
//!   outside their class, else `units::read_decl`), a fresh unit state
//!   with the entity as the holder, `Walk::decl_stream`,
//!   `enclosing::write`, `own_spelling::write_unit_list`;
//! - a content-keyed unit (`Walk::content_hash`): its memo entry taken
//!   out and the entity marked in progress for the time of the writing,
//!   as it was the first time, with the unit state, the type state,
//!   `in_key` and `keys_in_progress` put aside, the entity as the
//!   content entity, `Walk::decl_stream`, `own_spelling::write_unit_list`;
//!   the first mark only;
//! - the order unit (`units::order_unit`): `order::write`; the first mark
//!   only;
//! - a resolved or `default_argument` unit (`demand.rs`): its owner's
//!   arms run again (`demand::resolved_units`) with the stream module
//!   *armed* (`stream::Armed`), so that each stream an arm makes while no
//!   traced stream is open records, and each such stream hashed as a unit
//!   is kept; the second writing of the part is the kept stream that
//!   hashes to the digest the arm's second run gave it.
//!
//! What the first writing filled and the second reads (the memo of type
//! digests, of keys, of content digests, of constants, the numbering of
//! owners) holds what a writing at any time would compute: that is the
//! walk's own invariant (`spec/digest.md` §1 rule 5), and a unit for which
//! it fails has no trace. What a second writing adds to the walk's
//! context (a unit queued by a reference, bytes counted) is never read:
//! the output was made before.
//!
//! # Marks and references
//!
//! A unit's first mark is at offset 0, named by its key kind, at its
//! place (§6.1 "A unit's place"). The hooks' marks follow, with the
//! node's kind as Kiln's `Debug` prints it and its location taken to the
//! outermost expansion and its physical line. A [`RefAt`] is found
//! afterwards: a recorded `digest` field whose 32 bytes are the key of a
//! unit of the image. Bytes another stream wrote and this one copied
//! (`Stream::raw`, the elements of a `*sorted` list) and the streams of
//! digests (types, constants) are not this stream's: they have no mark
//! and no reference of their own (§6.1).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Mutex, PoisonError};

use slip_kiln::kiln::ast::DeclId;
use slip_kiln::kiln::ast::decl::DeclFlags;
use slip_kiln::kiln::source::Loc;
use slip_model::Digest;
use slip_model::hash::tagged_digest;
use slip_model::keys::KeyKind;

use crate::keys::{Role, info};
use crate::stream::{Armed, Hook, Recording, Stream, UNIT_TAG};
use crate::units::Written;
use crate::walk::{NodeRef, Walk};
use crate::{Input, Options, Output};

/// The `node` of the mark where the rows merged over an entity's
/// declarations begin (§6.1): neither a node kind nor a key kind.
pub const DECLARATIONS: &str = "declarations";

/// The `node` of the mark where a site-9 entry of an operand unit begins
/// (§6.1, WP-156 fix round 3): at the written name, neither a node kind
/// nor a key kind.
pub const WRITTEN_NAME: &str = "written name";

/// Where the walk began to write a node of a unit's stream (§6.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mark {
    /// The stream's length when the walk began to write the node.
    pub offset: u64,
    /// The variant's name in Kiln's `ExprKind` or `StmtKind`; for a unit's
    /// first mark, the name of its key kind; [`DECLARATIONS`] where a
    /// declaration's own rows begin.
    pub node: &'static str,
    /// The subject path of the node's location (the outermost expansion
    /// of a macro); empty with `line` and `column` 0 if it has none.
    pub file: Vec<u8>,
    /// Physical line, 1-based.
    pub line: u64,
    /// Column in bytes, 1-based.
    pub column: u64,
}

/// A digest field of a stream whose value is the key of a unit of the
/// image: a reference to that unit (§6.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefAt {
    pub offset: u64,
    pub len: u64,
    pub key: Digest,
}

/// One unit's stream, as hashed, with its marks and references.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trace {
    pub key: Digest,
    pub digest: Digest,
    pub stream: Vec<u8>,
    /// Ascending by offset (two marks can share one).
    pub marks: Vec<Mark>,
    /// Ascending by offset, not overlapping.
    pub refs: Vec<RefAt>,
}

/// What [`trace`] gives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Traced {
    pub output: Output,
    /// Sorted by `(key, digest)`.
    pub traces: Vec<Trace>,
    /// The second writing panicked: the units not traced before it have
    /// no trace (`spec/verify.md` §6.1, `verify.traces.panicked`).
    pub panicked: bool,
}

/// `output` is `digest(input, options)`, byte for byte; `traces` has one
/// entry per unit whose key is in `keys` (sorted, no duplicates) and whose
/// stream, written again, hashes to its digest. Units of one key with one
/// digest are one unit (their streams are the same bytes): the first
/// written gives the trace.
///
/// A panic in the second writing ends the tracing and nothing else:
/// `output` is whole, the traces made before it are kept (each hashed to
/// its unit's digest), the walk traces nothing more (its state is not
/// trusted after the unwind) and `panicked` is set. The stream module's
/// thread-locals are reset by the unwind (`Armed` and the recorders are
/// dropped), so a walk on the same thread afterwards records nothing.
pub fn trace(input: &Input<'_>, options: &Options<'_>, keys: &[Digest]) -> Traced {
    crate::walk::run_with(input, options, |w, written, output| {
        let mut traces = Vec::new();
        // Deliberate catch (`spec/verify.md` §6.1 `verify.traces.panicked`):
        // record a trace-pass panic in `Traced::panicked` without discarding
        // the digest output.
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            traces_into(w, written, keys, &mut traces);
        }))
        .is_err();
        // Stable: of one key and digest, the first written is kept.
        traces.sort_by_key(|t| (t.key, t.digest));
        traces.dedup_by(|b, a| a.key == b.key && a.digest == b.digest);
        Traced { output, traces, panicked }
    })
}

/// Pushes onto `out` the traces of the units of `written` whose key is in
/// `keys`, in the order they were written.
fn traces_into(w: &mut Walk<'_>, written: &[Written], keys: &[Digest], out: &mut Vec<Trace>) {
    if keys.is_empty() {
        return;
    }
    let unit_keys: HashSet<Digest> = written.iter().map(|u| u.key).collect();
    // The second writing of the parts of one owner is one run of its arms.
    let mut parts: HashMap<DeclId, Vec<(Written, Recording)>> = HashMap::new();
    for u in written {
        if keys.binary_search(&u.key).is_err() {
            continue;
        }
        let again = match (u.entity, u.owner) {
            (None, Some(_)) if u.operands.is_some() => Some(crate::operands::order_again(w)),
            (None, _) => Some(order(w)),
            (Some(e), Some(parent))
                if u.operands.is_some() && u.resolved.is_none() && u.default_argument.is_none() =>
            {
                operand_unit(w, written, u, parent, e)
            }
            (Some(e), Some(_)) => part(w, written, &mut parts, u, e),
            (Some(e), None) if content(w, Some(e)) => Some(content_unit(w, e)),
            (Some(e), None) => Some(entity_unit(w, u, e)),
        };
        let Some(again) = again else { continue };
        #[cfg(test)]
        let again = tests::perturbed(again);
        // §6.1: a trace only if the bytes hash to the unit's digest.
        if tagged_digest(UNIT_TAG, &again.bytes) != u.digest {
            continue;
        }
        // `<order>`'s operand unit has its site-9 entries' marks.
        let first_only = (u.entity.is_none() && u.operands.is_none())
            || (u.owner.is_none() && content(w, u.entity));
        out.push(made(w, u, again, first_only, &unit_keys));
        #[cfg(test)]
        tests::panic_at(tests::IN_THE_LOOP);
    }
}

/// A unit of a content-keyed kind (`spec/digest.md` §3.3).
fn content(w: &Walk<'_>, e: Option<DeclId>) -> bool {
    e.is_some_and(|e| matches!(info(w.ast.decls.get(e).kind()).role, Role::Content(_)))
}

/// The second writing of the unit of entity `e` (`units::write`).
fn entity_unit(w: &mut Walk<'_>, u: &Written, e: DeclId) -> Recording {
    entity_again(w, u, e, false)
}

/// The writing of [`entity_unit`]; with `operands`, as the first writing
/// was, with its operand unit (`operands.rs`) after it and the stream
/// module armed by the caller ([`operand_unit`]): the unit's stream is
/// then an ordinary one, finished before the operand unit's begins, and
/// what is returned is not the unit's.
fn entity_again(w: &mut Walk<'_>, u: &Written, e: DeclId, operands: bool) -> Recording {
    let outside = crate::units::outside_only(w, e);
    let from = if outside { u.read.unwrap_or(e) } else { crate::units::read_decl(w, e) };
    let saved = w.take_unit();
    w.unit.local.holder = Some(e);
    if operands {
        crate::operands::begin(w, e, Some(from));
    }
    let mut s = if operands { Stream::new() } else { Stream::traced() };
    w.decl_stream(&mut s, from, e);
    #[cfg(test)]
    tests::panic_at(tests::IN_A_TRACED_STREAM);
    crate::enclosing::write(w, &mut s, e);
    crate::own_spelling::write_unit_list(w, &mut s);
    let rec = if operands {
        let _ = s.finish(UNIT_TAG);
        let _ = crate::operands::finish(w);
        Recording { bytes: Vec::new(), marks: Vec::new(), digests: Vec::new() }
    } else {
        s.into_recording()
    };
    w.unit = saved;
    rec
}

/// The second writing of the content-keyed entity `e`
/// (`Walk::content_hash`): as the first, which met no memo entry of `e`
/// and found `e` in progress inside its own writing.
fn content_unit(w: &mut Walk<'_>, e: DeclId) -> Recording {
    content_again(w, e, false)
}

/// The writing of [`content_unit`]; with `operands`, as
/// [`entity_again`]'s.
fn content_again(w: &mut Walk<'_>, e: DeclId, operands: bool) -> Recording {
    let read = crate::units::read_decl(w, e);
    let memo = w.content.remove(&e);
    let fresh = w.content_in_progress.insert(e);
    let saved = w.take_unit();
    let types = w.take_types();
    let in_key = std::mem::take(&mut w.in_key);
    let keys = std::mem::take(&mut w.keys_in_progress);
    w.unit.local.content = Some(e);
    if operands {
        crate::operands::begin(w, e, Some(read));
    }
    let mut s = if operands { Stream::new() } else { Stream::traced() };
    w.decl_stream(&mut s, read, e);
    crate::own_spelling::write_unit_list(w, &mut s);
    let rec = if operands {
        let _ = s.finish(UNIT_TAG);
        let _ = crate::operands::finish(w);
        Recording { bytes: Vec::new(), marks: Vec::new(), digests: Vec::new() }
    } else {
        s.into_recording()
    };
    w.keys_in_progress = keys;
    w.in_key = in_key;
    w.restore_types(types);
    w.unit = saved;
    if fresh {
        w.content_in_progress.remove(&e);
    }
    if let Some(h) = memo {
        w.content.insert(e, h);
    }
    rec
}

/// The second writing of the operand unit `u` (`operands.rs`, WP-156)
/// of the entity or content-keyed unit of `e`, whose key is `parent`: the
/// parent's writing again with its operand unit after it, the stream
/// module armed, and the kept stream that hashes to `u`'s digest. `None`
/// if the parent is not among `written` or no kept stream hashes so.
fn operand_unit(
    w: &mut Walk<'_>,
    written: &[Written],
    u: &Written,
    parent: Digest,
    e: DeclId,
) -> Option<Recording> {
    let p = written.iter().find(|p| p.key == parent && p.operands.is_none())?;
    let armed = Armed::new();
    if content(w, Some(e)) {
        content_again(w, e, true);
    } else {
        entity_again(w, p, e, true);
    }
    armed.take().into_iter().find(|r| tagged_digest(UNIT_TAG, &r.bytes) == u.digest)
}

/// The second writing of the order unit (`units::order_unit`).
fn order(w: &mut Walk<'_>) -> Recording {
    let mut s = Stream::traced();
    crate::order::write(w, &mut s);
    s.into_recording()
}

/// The second writing of the part `u` (a resolved or `default_argument`
/// unit) of entity `e`: its owner's arms run again, once per owner, and
/// the stream kept that hashes to the digest the arm gave the part of
/// `u`'s key the second time. `None` if the second run made no such part.
fn part(
    w: &mut Walk<'_>,
    written: &[Written],
    parts: &mut HashMap<DeclId, Vec<(Written, Recording)>>,
    u: &Written,
    e: DeclId,
) -> Option<Recording> {
    if let std::collections::hash_map::Entry::Vacant(slot) = parts.entry(e) {
        let owner = written
            .iter()
            .find(|o| o.entity == Some(e) && o.owner.is_none() && o.kind != KeyKind::Order);
        match owner {
            Some(owner) => slot.insert(parts_again(w, e, owner)),
            // A local function an instantiation made, whose
            // `default_argument` units have no unit above them
            // (`demand::local_default_arguments`, WP-158).
            None if crate::demand::is_local_default_holder(w, e) => {
                let owner = crate::demand::local_owner(w, e);
                slot.insert(local_parts_again(w, e, &owner))
            }
            None => return None,
        };
    }
    let again = parts.get(&e)?;
    again
        .iter()
        .find(|(q, _)| {
            q.key == u.key && q.resolved == u.resolved && q.default_argument == u.default_argument
        })
        .map(|(_, r)| r.clone())
}

/// The `default_argument` units of the local function `e` written again
/// ([`parts_again`] for an owner that is no unit).
fn local_parts_again(w: &mut Walk<'_>, e: DeclId, owner: &Written) -> Vec<(Written, Recording)> {
    again_with(|w, out| crate::demand::local_parts(w, e, owner, out), w)
}

/// The parts of `owner` (the unit of `e`) written again, each with the
/// stream of its second writing; a part whose stream was not kept is
/// left out.
fn parts_again(w: &mut Walk<'_>, e: DeclId, owner: &Written) -> Vec<(Written, Recording)> {
    again_with(|w, out| crate::demand::resolved_units(w, e, owner, out), w)
}

/// The parts `write` writes again, each with the stream of its second
/// writing; a part whose stream was not kept is left out.
fn again_with(
    write: impl FnOnce(&mut Walk<'_>, &mut Vec<Written>),
    w: &mut Walk<'_>,
) -> Vec<(Written, Recording)> {
    let armed = Armed::new();
    let mut again = Vec::new();
    write(w, &mut again);
    #[cfg(test)]
    tests::panic_at(tests::WHILE_ARMED);
    let kept = armed.take();
    let mut by_digest: HashMap<Digest, Recording> = HashMap::new();
    for r in kept {
        by_digest.entry(tagged_digest(UNIT_TAG, &r.bytes)).or_insert(r);
    }
    again
        .into_iter()
        .filter_map(|q| {
            let r = by_digest.get(&q.digest)?.clone();
            Some((q, r))
        })
        .collect()
}

/// The trace of `u` from its second writing `r`, whose bytes hash to its
/// digest. With `first_only`, the hooks' marks are dropped (§6.1: a
/// content-keyed unit and the order unit have the first mark only).
fn made(
    w: &Walk<'_>,
    u: &Written,
    rec: Recording,
    first_only: bool,
    unit_keys: &HashSet<Digest>,
) -> Trace {
    let (file, line, column) = located(w, place(w, u));
    let mut marks = vec![Mark { offset: 0, node: u.kind.name(), file, line, column }];
    if !first_only {
        let mut names = Names::default();
        for &(offset, node) in &rec.marks {
            let (node, loc) = match node {
                Hook::Node(NodeRef::Expr(id)) => {
                    let expr = w.ast.exprs.get(id);
                    (names.expr(expr.kind()), expr.loc())
                }
                Hook::Node(NodeRef::Stmt(id)) => {
                    let stmt = w.ast.stmts.get(id);
                    (names.stmt(stmt.kind()), stmt.loc())
                }
                Hook::Decls(e) => (DECLARATIONS, entity_place(w, e)),
                Hook::Name(loc) => (WRITTEN_NAME, loc),
            };
            let (file, line, column) = located(w, loc);
            marks.push(Mark { offset, node, file, line, column });
        }
    }
    let refs = rec
        .digests
        .iter()
        .filter_map(|&offset| {
            let at = usize::try_from(offset).ok()?;
            let bytes: [u8; 32] = rec.bytes.get(at..at + 32)?.try_into().ok()?;
            let key = Digest(bytes);
            unit_keys.contains(&key).then_some(RefAt { offset, len: 32, key })
        })
        .collect();
    Trace { key: u.key, digest: u.digest, stream: rec.bytes, marks, refs }
}

/// A unit's place (§6.1): the location of the first declaration of its
/// entity's chain, earliest first, that the source wrote (not implicit,
/// not made by an instantiation, not the declaration Kiln made of a
/// function specialisation before it was explicitly specialised); else of
/// the first that has a location; none for the order unit or an entity no
/// declaration of which has one.
fn place(w: &Walk<'_>, u: &Written) -> Loc {
    u.entity.map_or(Loc::INVALID, |e| entity_place(w, e))
}

/// The place of entity `e` (§6.1), as [`place`] takes it.
fn entity_place(w: &Walk<'_>, e: DeclId) -> Loc {
    let ast = w.ast;
    let mut chain = w.chain(e);
    chain.reverse();
    let written = |d: DeclId| {
        !ast.decls.get(d).has(DeclFlags::IMPLICIT)
            && !crate::templ::made_by_instantiation(w, d)
            && !made_before_explicit_specialisation(w, d)
            && w.has_location(d)
    };
    chain
        .iter()
        .copied()
        .find(|&d| written(d))
        .or_else(|| chain.iter().copied().find(|&d| w.has_location(d)))
        .map_or(Loc::INVALID, |d| ast.decls.get(d).loc())
}

/// Whether `d` is the declaration Kiln made of a function template
/// specialisation (by deduction, or a member by its class's
/// instantiation) before the source explicitly specialised it (§6.1, WP-128):
/// the first of a function's chain whose specialisation kind is now
/// `ExplicitSpecialization` and which a written declaration follows. It is
/// located at the pattern; its `SpecInfo` (shared with the written
/// declarations) no longer says it was instantiated, and a non-member's
/// lexical context is no instantiation, so [`crate::templ::made_by_instantiation`]
/// does not see it. Kiln's own reading of "first written": `Ast::contract_first`.
fn made_before_explicit_specialisation(w: &Walk<'_>, d: DeclId) -> bool {
    let ast = w.ast;
    ast.decls.function(d).is_some() && ast.first(d) == d && ast.contract_first(d) != d
}

/// The subject path, line and column of `loc` taken to its outermost
/// expansion, in the physical file (a `#line` directive does not move it:
/// the place is for opening the file); empty, 0, 0 if it has none.
fn located(w: &Walk<'_>, loc: Loc) -> (Vec<u8>, u64, u64) {
    match w.sm.line_col(loc) {
        Some(p) => ((w.subject_path)(p.file), u64::from(p.line), u64::from(p.col)),
        None => (Vec::new(), 0, 0),
    }
}

/// The names of node kinds met in one trace, as Kiln's `Debug` prints the
/// variant ([`kind_name`]).
#[derive(Default)]
struct Names {
    exprs: HashMap<slip_kiln::kiln::ast::expr::ExprKind, &'static str>,
    stmts: HashMap<slip_kiln::kiln::ast::stmt::StmtKind, &'static str>,
}

impl Names {
    fn expr(&mut self, k: slip_kiln::kiln::ast::expr::ExprKind) -> &'static str {
        self.exprs.entry(k).or_insert_with(|| kind_name(&k))
    }

    fn stmt(&mut self, k: slip_kiln::kiln::ast::stmt::StmtKind) -> &'static str {
        self.stmts.entry(k).or_insert_with(|| kind_name(&k))
    }
}

/// The variant's name of a node kind, as its `Debug` prints it, kept for
/// the life of the process: one string per variant met (Kiln's
/// `ExprKind` and `StmtKind` have a few hundred), so the set is bounded.
fn kind_name(k: &dyn std::fmt::Debug) -> &'static str {
    static NAMES: Mutex<BTreeMap<String, &'static str>> = Mutex::new(BTreeMap::new());
    let text = format!("{k:?}");
    let mut names = NAMES.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(&n) = names.get(&text) {
        return n;
    }
    let n: &'static str = Box::leak(text.clone().into_boxed_str());
    names.insert(text, n);
    n
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use slip_model::keys::RuleTable;

    use super::*;
    use crate::stream::{MAX_LIVE, RECORDERS, Recording};
    use crate::{Detail, ReadOnly};

    thread_local! {
        /// Set: the second writing of a unit gets one byte more
        /// ([`perturb`]), as a stream that depends on when it is written
        /// would.
        static PERTURB: Cell<bool> = const { Cell::new(false) };
        /// Where the second writing panics ([`panic_at`]): 0 nowhere.
        static PANIC_AT: Cell<u8> = const { Cell::new(0) };
    }

    /// After a unit's trace is made, in [`traces_into`]'s loop.
    pub(super) const IN_THE_LOOP: u8 = 1;
    /// Inside an entity unit's second writing, its traced stream open and
    /// the unit's state put aside.
    pub(super) const IN_A_TRACED_STREAM: u8 = 2;
    /// While the stream module is armed, after an owner's parts were
    /// written again and before they are taken.
    pub(super) const WHILE_ARMED: u8 = 3;

    /// The panic hook of the second writing: panics at `here` if
    /// [`PANIC_AT`] says so.
    pub(super) fn panic_at(here: u8) {
        assert!(
            PANIC_AT.with(Cell::get) != here,
            "WP-054 test: the second writing panics at {here}"
        );
    }

    /// The mutation hook of [`traces`]: a byte added to the second writing
    /// while [`PERTURB`] is set.
    pub(super) fn perturbed(mut r: Recording) -> Recording {
        if PERTURB.with(Cell::get) {
            r.bytes.push(0);
        }
        r
    }

    /// A unit with statements, expressions, a call, references, a type
    /// digest holding an expression (`decltype`), a default argument, a
    /// resolved unit (a constant's `value`), a class template's
    /// instantiation, a content-keyed unit and the order unit.
    const SRC: &str = "\
namespace lib { int f(long); }
int g(int a, int b = 2) { return a + b; }
template <class T> struct S { T t; int m() { return sizeof(T); } };
constexpr int k = 3;
static_assert(k == 3);
int h(int x) {
  decltype(x + 1) y = lib::f(x);
  if (y > 0) { return g(y); }
  S<int> s;
  return s.m() + k;
}
int v = h(1);
";

    fn with<R>(src: &str, f: impl FnOnce(&Input<'_>) -> R) -> R {
        let files = [("main.cpp", src)];
        let snippet = slip_kiln::snippet::Snippet {
            files: &files,
            main: "main.cpp",
            include: &[""],
            std: "c++20",
        };
        slip_kiln::snippet::parse_with_tables(
            &snippet,
            &slip_kiln::snippet::Extra::default(),
            |p| {
                assert_eq!(p.errors, 0, "{}", p.diagnostics);
                let path = |f| p.path(f).map(|s: &str| s.as_bytes().to_vec()).unwrap_or_default();
                let input = Input { ast: p.ast, sm: p.sm, subject_path: &path };
                f(&input)
            },
        )
        .expect("the snippet parses")
    }

    fn all_keys(o: &Output) -> Vec<Digest> {
        let mut keys: Vec<Digest> =
            o.digests.groups.iter().flat_map(|g| g.units.iter().flatten().map(|u| u.key)).collect();
        keys.sort_unstable();
        keys.dedup();
        keys
    }

    fn options(rules: &RuleTable) -> Options<'_> {
        Options {
            rules,
            read_only: ReadOnly::NO_FILE,
            detail: Detail::All,
            names: true,
            rule_use: false,
        }
    }

    // §6.1 "Traces": tracing changes no digest; every unit of the source
    // has a trace hashing to its digest, whose marks are ascending, inside
    // the stream, the first at 0 named by the key kind; a reference is a
    // digest field that is a unit's key. The ordinary walk makes no
    // recorder (it allocates nothing for tracing), and a trace makes one
    // per traced stream, never two open at once: no sub-stream of a
    // traced stream records.
    #[test]
    fn s6_1_traces_of_every_unit() {
        with(SRC, |input| {
            let rules = RuleTable::empty();
            let before = RECORDERS.with(Cell::get);
            let out = crate::digest(input, &options(&rules));
            assert_eq!(RECORDERS.with(Cell::get), before, "the ordinary walk made a recorder");
            let keys = all_keys(&out);
            MAX_LIVE.with(|m| m.set(0));
            let traced = trace(input, &options(&rules), &keys);
            assert!(traced.output == out, "tracing changed the output");
            assert_eq!(MAX_LIVE.with(Cell::get), 1, "a sub-stream recorded");
            // The module is at rest after a trace, and an ordinary walk on
            // the same thread afterwards makes no recorder.
            assert!(crate::stream::at_rest(), "tracing left the stream module armed");
            let before = RECORDERS.with(Cell::get);
            assert!(crate::digest(input, &options(&rules)) == out);
            assert_eq!(RECORDERS.with(Cell::get), before, "a walk after a trace recorded");
            assert!(!traced.panicked);
            let units: HashSet<(Digest, Digest)> = out
                .digests
                .groups
                .iter()
                .flat_map(|g| g.units.iter().flatten().map(|u| (u.key, u.digest)))
                .collect();
            let have: HashSet<(Digest, Digest)> =
                traced.traces.iter().map(|t| (t.key, t.digest)).collect();
            assert_eq!(have, units, "a unit has no trace");
            let names: HashMap<Digest, &str> =
                out.names.iter().map(|(k, n)| (*k, n.as_str())).collect();
            let mut kinds = HashSet::new();
            for t in &traced.traces {
                assert_eq!(tagged_digest(UNIT_TAG, &t.stream), t.digest);
                assert_eq!(t.marks[0].offset, 0);
                assert!(t.marks.windows(2).all(|m| m[0].offset <= m[1].offset));
                assert!(t.marks.iter().all(|m| m.offset < t.stream.len() as u64));
                assert!(t.refs.windows(2).all(|r| r[0].offset + r[0].len <= r[1].offset));
                for r in &t.refs {
                    assert!(names.contains_key(&r.key), "a reference to no unit");
                    let at = r.offset as usize;
                    assert_eq!(&t.stream[at..at + 32], r.key.as_bytes());
                }
                kinds.extend(t.marks.iter().map(|m| m.node));
            }
            for k in ["function", "CallExpr", "IfStmt", "ReturnStmt", "BinaryOperator"] {
                assert!(kinds.contains(k), "no mark {k}: {kinds:?}");
            }
            // `h`: its call of `lib::f` is a reference, and its marks are
            // at the lines of the source.
            let h = traced
                .traces
                .iter()
                .find(|t| names.get(&t.key) == Some(&"h(int)"))
                .expect("a trace of h");
            assert_eq!((h.marks[0].node, h.marks[0].line), ("function", 6));
            assert_eq!(h.marks[0].file, b"main.cpp");
            let lines: HashSet<u64> = h.marks.iter().map(|m| m.line).collect();
            assert!([7, 8, 10].iter().all(|l| lines.contains(l)), "{lines:?}");
            assert!(h.refs.iter().any(|r| names.get(&r.key) == Some(&"lib::f(long)")));
            // The order unit and a content-keyed unit: the first mark only.
            for t in &traced.traces {
                let n = names.get(&t.key).copied().unwrap_or_default();
                if n == "<order>" || n.ends_with("<static_assert>") {
                    assert_eq!(t.marks.len(), 1, "{n}");
                }
            }
            // A resolved unit (`k`'s value) has a trace.
            assert!(traced.traces.iter().any(|t| names.get(&t.key) == Some(&"k::<value>")));
            // Only the wanted keys.
            let one = [h.key];
            let only = trace(input, &options(&rules), &one);
            assert_eq!(only.traces.len(), 1);
            assert_eq!(only.traces[0], *h);
            assert!(trace(input, &options(&rules), &[]).traces.is_empty());
        });
    }

    // §6.1: a second writing that does not hash to the unit's digest is no
    // trace (here every second writing gets one byte more).
    #[test]
    fn s6_1_unstable_unit_has_no_trace() {
        with(SRC, |input| {
            let rules = RuleTable::empty();
            let out = crate::digest(input, &options(&rules));
            let keys = all_keys(&out);
            PERTURB.with(|p| p.set(true));
            let traced = trace(input, &options(&rules), &keys);
            PERTURB.with(|p| p.set(false));
            assert!(traced.output == out);
            assert!(traced.traces.is_empty(), "{} traces", traced.traces.len());
        });
    }

    // §6.1 "A unit's place": an instantiation's unit, which no declaration
    // the source wrote declares, is placed at its pattern; the kind names
    // are the variants' `Debug` names, the same string each time.
    #[test]
    fn s6_1_place_of_an_instantiation() {
        with(SRC, |input| {
            let rules = RuleTable::empty();
            let out = crate::digest(input, &options(&rules));
            let traced = trace(input, &options(&rules), &all_keys(&out));
            let names: HashMap<Digest, &str> =
                out.names.iter().map(|(k, n)| (*k, n.as_str())).collect();
            let s_int = traced
                .traces
                .iter()
                .find(|t| names.get(&t.key) == Some(&"S<int>"))
                .expect("a trace of S<int>");
            assert_eq!((s_int.marks[0].node, s_int.marks[0].line), ("specialisation", 3));
            assert!(std::ptr::eq(
                kind_name(&slip_kiln::kiln::ast::expr::ExprKind::CallExpr),
                kind_name(&slip_kiln::kiln::ast::expr::ExprKind::CallExpr)
            ));
        });
    }

    // §6.1: a panic in the second writing withdraws places and nothing
    // else: the output is whole, the traces made before it stand (each
    // hashing to its digest), nothing more is traced, `panicked` is set;
    // the stream module is at rest after the unwind, and a walk on the
    // same thread afterwards records nothing. At each of the three places
    // a second writing can be: between units, inside a traced stream, and
    // while armed for a unit's parts.
    #[test]
    fn s6_1_panic_in_the_second_writing() {
        with(SRC, |input| {
            let rules = RuleTable::empty();
            let out = crate::digest(input, &options(&rules));
            let keys = all_keys(&out);
            let whole = trace(input, &options(&rules), &keys);
            for at in [IN_THE_LOOP, IN_A_TRACED_STREAM, WHILE_ARMED] {
                PANIC_AT.with(|p| p.set(at));
                let traced = trace(input, &options(&rules), &keys);
                PANIC_AT.with(|p| p.set(0));
                assert!(traced.panicked, "no panic at {at}");
                assert!(traced.output == out, "a panic at {at} changed the output");
                assert!(traced.traces.len() < whole.traces.len(), "{at}: traced on after it");
                for t in &traced.traces {
                    assert_eq!(tagged_digest(UNIT_TAG, &t.stream), t.digest);
                    assert!(whole.traces.contains(t), "{at}: a trace that differs");
                }
                if at == IN_THE_LOOP {
                    assert_eq!(traced.traces.len(), 1, "the one made before the panic");
                }
                assert!(crate::stream::at_rest(), "{at}: the unwind left the module armed");
                let before = RECORDERS.with(Cell::get);
                assert!(crate::digest(input, &options(&rules)) == out);
                assert_eq!(RECORDERS.with(Cell::get), before, "{at}: a walk after it recorded");
            }
        });
    }
}
