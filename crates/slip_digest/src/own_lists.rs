//! The walk's two lists of ownership (`spec/landed/verify/WP-146.md`
//! §3.1 and "Stages 0 and 1 as built"; WP-146 stage 1). Inert: nothing
//! here is hashed, counted or keyed into a unit; the lists are formed
//! after every other field of the walk's `Output` is complete, with rule
//! use no longer counted (`Walk::rule_use` taken) and `Walk::bytes` put
//! back.
//!
//! Under a table that is not empty, with rule use asked for
//! (`Options::rule_use`), every entity the walk keyed (its own key
//! memoised in `Walk::keys`) that
//!
//! - is not a namespace, not of a `units::repeatable` kind, not a part
//!   (`resolved`, `default_argument`),
//! - is not of internal linkage (`Ast::linkage`, as `enclosing.rs` reads
//!   it) and not in or under an unnamed namespace
//!   (`Ast::in_anonymous_namespace`); every other answer (none, module,
//!   external) is contributed, aliases included (ruling 4),
//! - is not a deduction guide of a dependency template
//!
//! is in **made** if its key was formed as an owned entity's (under the
//! rules: `OwnedState::keyed`, not `Walk::owned` asked again) and a rule
//! made it: its own key matched a rule, or its parent is a namespace and a
//! level of its chain of namespaces in the owned view matched one
//! (`rule_use::made_by`). A made entry carries two keys: `key`, its own
//! key (mapped: the pre-image's), and `post`, the key it would have were it
//! a dependency's (its key formed with its namespace parents keyed in the
//! dependency view, `Walk::compute_view_key` with `false`, no table: the
//! post-image's spelling), `None` where that key cannot be formed.
//!
//! It is in **fixed** if its key was formed as a dependency entity's (one
//! by the per-inclusion rule alone included) and
//!
//! - **T** (a split: one entity of the pre-image, two of the post-image):
//!   a rule produces its own key (`RuleTable::produces`, on the key before
//!   the table that the memo recorded), or its parent is a namespace and a
//!   level of its chain of namespaces, keyed as the entity's own key keys
//!   it (the dependency view, unmapped), has a key a rule produces;
//! - **M** (a merge: two entities of the pre-image, one of the
//!   post-image): the key it would have were it owned (its key formed with
//!   a namespace parent keyed in the owned view, `Walk::compute_view_key`
//!   with `true`, then the table on it) differs from its own key: a rule
//!   would move it.
//!
//! Either entry's key is the entity's own key (unmapped). Both may hold of
//! one entity: two entries. The join of stage 3 is `made.key == fixed.key`
//! for T (the pre-image key) and `made.post == fixed.key` for M (the
//! post-image key); a made entry with no `post` cannot be joined for M and
//! is itself to be reported (fail closed). A class member, a
//! specialisation, a member of an instantiation and an implicit member
//! have a parent that is no namespace (the chain is not climbed through a
//! class): they are in a list only where a rule names them.
//!
//! **Where.** A made entry is at its first declaration's place
//! (`Walk::representative`, through `Walk::located`); a fixed entry at the
//! earliest declaration (Kiln's chain, earliest first) that is not in an
//! owned file (`Walk::in_owned_file`: by expansion location, name token and
//! include chain), else at the first declaration's (a dependency entity by
//! template, guide or implicit member). The place is the subject path,
//! line and column of the outermost expansion, as `trace.rs` gives it.
//! The name is `names::entity_name`.

use std::collections::{HashMap, HashSet};

use slip_kiln::kiln::ast::DeclId;
use slip_kiln::kiln::ast::linkage::Linkage;
use slip_model::Digest;
use slip_model::keys::{Key, KeyKind};

use crate::keys::KeyRes;
use crate::rule_use::Counting;
use crate::walk::Walk;

/// How far a chain of namespaces is climbed (an AST is far shallower).
const CHAIN_LIMIT: usize = 4096;

/// An owned entity whose key a rule made (`Output::made`).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Made {
    /// Its mapped key's id (the pre-image key): the T join's.
    pub key: Digest,
    /// The rule, its index in `RuleTable::rules`.
    pub rule: u64,
    /// The id of its key were it a dependency's (namespace parents in the
    /// dependency view, no table: the post-image key), the M join's;
    /// `None` where that key cannot be formed (past the depth limit).
    pub post: Option<Digest>,
    /// The subject path, line and column of its first declaration.
    pub file: Vec<u8>,
    pub line: u64,
    pub column: u64,
    /// The entity as a person writes it.
    pub name: String,
}

/// Which test put a dependency entity in `Output::fixed`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FixedAs {
    /// Its own key is one a rule could make.
    T,
    /// Its key were it owned differs from its own key.
    M,
}

/// A dependency entity whose key a rule could have made, or would have
/// moved (`Output::fixed`).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fixed {
    /// Its own key's id (unmapped).
    pub key: Digest,
    pub as_: FixedAs,
    /// The subject path, line and column of its first declaration that
    /// is not in an owned file.
    pub file: Vec<u8>,
    pub line: u64,
    pub column: u64,
    /// The entity as a person writes it.
    pub name: String,
    /// Empty, else the subject path of the first read-only file on the
    /// include chain of `file` when that declaration lies in a file that is
    /// not read-only by its own path (WP-150 stage 2, additive).
    pub through: Vec<u8>,
}

/// The two lists of the walk `w`, its rule use `counting` (taken from it):
/// empty unless rule use was counted (`counting` is `Some` only under a
/// table that is not empty). Each sorted.
pub(crate) fn lists(w: &mut Walk<'_>, counting: Option<&Counting>) -> (Vec<Made>, Vec<Fixed>) {
    let Some(c) = counting else { return (Vec::new(), Vec::new()) };
    debug_assert!(w.rule_use.is_none(), "the lists are formed with rule use no longer counted");
    // What the keys formed here would change of the walk's state, put
    // back after: nothing of `Output` reads it, but the caller of
    // `run_with` may read the walk.
    let bytes = w.bytes;
    let mut entities: Vec<(DeclId, Digest, KeyKind)> = w
        .keys
        .iter()
        .filter_map(|(&e, r)| match *r {
            KeyRes::Key { id, kind, .. } => Some((e, id, kind)),
            KeyRes::None(_) => None,
        })
        .collect();
    entities.sort_by_key(|&(e, _, _)| e.index());
    let mut made = Vec::new();
    let mut fixed = Vec::new();
    let mut produced_chain: HashMap<DeclId, bool> = HashMap::new();
    for (e, id, kind) in entities {
        if !candidate(w, e, kind) {
            continue;
        }
        // The ownership and the key before the table that the memo
        // recorded (`Walk::key_in`); every memoised key has them (the
        // switch is set before the walk keys anything). One without them
        // is read as a dependency entity's, keyed again: never made.
        let recorded = w.owned_state.keyed.get(&e).cloned();
        debug_assert!(recorded.is_some(), "a memoised key with no record");
        if let Some((true, _)) = recorded {
            if let Some(rule) = crate::rule_use::made_by(c, e) {
                let at = w.located(w.representative(e));
                let (file, line, column) = place(w, at);
                let name = name(w, e, kind);
                let post = post_key(w, e);
                made.push(Made { key: id, rule: rule as u64, post, file, line, column, name });
            }
            continue;
        }
        let own = match recorded {
            Some((_, k)) => Some(k),
            None => own_key(w, e),
        };
        let as_t = own.is_some_and(|k| w.rules.produces(&k).is_some())
            || w.owned_state
                .ns_parents
                .get(&e)
                .copied()
                .is_some_and(|ns| chain_produced(w, ns, &mut produced_chain));
        let as_m = as_if_owned(w, e).is_some_and(|k| k != id);
        if !as_t && !as_m {
            continue;
        }
        let at = first_read_only(w, e);
        let (file, line, column) = place(w, at);
        let through = if file.is_empty() { Vec::new() } else { through_file(w, at) };
        let name = name(w, e, kind);
        if as_t {
            let (file, name, through) = (file.clone(), name.clone(), through.clone());
            fixed.push(Fixed { key: id, as_: FixedAs::T, file, line, column, name, through });
        }
        if as_m {
            fixed.push(Fixed { key: id, as_: FixedAs::M, file, line, column, name, through });
        }
    }
    w.bytes = bytes;
    made.sort();
    fixed.sort();
    (made, fixed)
}

/// Whether entity `e`, of key kind `kind`, may be in a list (the module's
/// first three bullets).
fn candidate(w: &mut Walk<'_>, e: DeclId, kind: KeyKind) -> bool {
    if kind == KeyKind::Namespace
        || crate::units::repeatable(kind)
        || matches!(kind, KeyKind::Resolved | KeyKind::DefaultArgument)
    {
        return false;
    }
    let rep = w.representative(e);
    if w.ast.linkage(rep) == Linkage::Internal || w.ast.in_anonymous_namespace(rep) {
        return false;
    }
    if kind == KeyKind::DeductionGuide
        && let Some(t) = w.guide_template(e)
        && !w.owned(t)
    {
        return false;
    }
    true
}

/// Runs `f`, a key computation outside `Walk::key`, as `Walk::key_bytes`
/// does: the unit's and the type digests' state put aside, inside a key.
/// `None` past the depth limit.
fn apart<R>(w: &mut Walk<'_>, f: impl FnOnce(&mut Walk<'_>) -> R) -> Option<R> {
    if !w.enter() {
        return None;
    }
    let saved = w.take_unit();
    let types = w.take_types();
    w.in_key += 1;
    let r = f(w);
    w.in_key -= 1;
    w.restore_types(types);
    w.unit = saved;
    w.leave();
    Some(r)
}

/// Dependency entity `e`'s own key, unmapped (rule 1), as `Walk::key`
/// formed it: only where the memo recorded none.
fn own_key(w: &mut Walk<'_>, e: DeclId) -> Option<Key> {
    let rep = w.representative(e);
    apart(w, |w| w.compute_key(rep).ok().map(|(k, _)| k)).flatten()
}

/// The id of dependency entity `e`'s key were it owned: formed with its
/// namespace parent keyed in the owned view, then mapped.
fn as_if_owned(w: &mut Walk<'_>, e: DeclId) -> Option<Digest> {
    let rep = w.representative(e);
    let key = apart(w, |w| w.compute_view_key(rep, true).ok().map(|(k, _)| k)).flatten()?;
    Some(w.rules.map(key).id())
}

/// The id of owned entity `e`'s key were it a dependency's: formed with
/// its namespace parents keyed in the dependency view, no table (`Made::post`).
fn post_key(w: &mut Walk<'_>, e: DeclId) -> Option<Digest> {
    let rep = w.representative(e);
    let key = apart(w, |w| w.compute_view_key(rep, false).ok().map(|(k, _)| k)).flatten()?;
    Some(key.id())
}

/// Whether namespace `ns`, keyed in the dependency view (unmapped), or a
/// namespace around it, has a key a rule produces; memoised in `memo`.
fn chain_produced(w: &mut Walk<'_>, ns: DeclId, memo: &mut HashMap<DeclId, bool>) -> bool {
    let mut path: Vec<DeclId> = Vec::new();
    let mut seen: HashSet<DeclId> = HashSet::new();
    let mut at = Some(ns);
    let mut found = false;
    while let Some(n) = at {
        if let Some(&r) = memo.get(&n) {
            found = r;
            break;
        }
        if !seen.insert(n) || path.len() > CHAIN_LIMIT {
            break;
        }
        path.push(n);
        let rep = w.representative(n);
        let key = apart(w, |w| w.compute_view_key(rep, false).ok().map(|(k, _)| k)).flatten();
        if key.is_some_and(|k| w.rules.produces(&k).is_some()) {
            found = true;
            break;
        }
        at = w.owned_state.ns_parents.get(&n).copied();
    }
    for n in path {
        memo.insert(n, found);
    }
    found
}

/// The declaration a fixed entry is at: the earliest of `e`'s chain not in
/// an owned file, else the first declaration's place.
fn first_read_only(w: &mut Walk<'_>, e: DeclId) -> Option<DeclId> {
    let mut chain = w.chain(e);
    chain.reverse();
    for d in chain {
        if w.has_location(d) && !w.in_owned_file(d) {
            return Some(d);
        }
    }
    w.located(w.representative(e))
}

/// The subject path, line and column of `d`'s place; empty, 0, 0 if it
/// has none.
fn place(w: &Walk<'_>, d: Option<DeclId>) -> (Vec<u8>, u64, u64) {
    let Some(d) = d else { return (Vec::new(), 0, 0) };
    match w.sm.line_col(w.ast.decls.get(d).loc()) {
        Some(p) => ((w.subject_path)(p.file), u64::from(p.line), u64::from(p.col)),
        None => (Vec::new(), 0, 0),
    }
}

/// When `d`'s expansion location lies in a file that is not read-only by its
/// own path and was entered through a read-only include chain, the subject
/// path of the first read-only file on that chain; empty otherwise.
fn through_file(w: &mut Walk<'_>, d: Option<DeclId>) -> Vec<u8> {
    let Some(d) = d else { return Vec::new() };
    let exp = w.sm.expansion_loc(w.ast.decls.get(d).loc());
    let Some(file) = w.buffer_of(exp) else { return Vec::new() };
    if w.file_read_only(file) {
        return Vec::new();
    }
    let mut at = w.sm.include_loc(exp);
    let mut steps = 0usize;
    while at.is_valid() && steps <= CHAIN_LIMIT {
        if let Some(f) = w.buffer_of(at)
            && w.file_read_only(f)
        {
            return (w.subject_path)(f);
        }
        at = w.sm.include_loc(at);
        steps += 1;
    }
    Vec::new()
}

fn name(w: &Walk<'_>, e: DeclId, kind: KeyKind) -> String {
    crate::names::entity_name(w.ast, w.representative(e), kind)
}
