//! Members of class templates (`spec/edits.md` §9.2, the follower "a
//! member of a class template specialisation"; `docs/wp/WP-073.md` parts
//! B and C).
//!
//! A seed that is a member of a class template (a class template
//! anywhere in its scopes) is renamed as any seed is: its declarations
//! and the references to it and to its instantiations are claimed by
//! `decls` and `refs` (`written::origin` takes a member of an
//! instantiated class to the member of the template). What this file
//! adds is the followers:
//!
//! - **Followers.** Every member of an instantiated specialisation that
//!   Kiln links to the seed (`instantiated_from`, through the whole chain
//!   of patterns, asked of every declaration of the member as
//!   `spec/digest.md` §3.4 asks it), whose key has the old name, is a
//!   follower with `of` the subject it was instantiated from: the member
//!   of the template, or the follower in between for a member of a
//!   member template's specialisation. A specialisation's own link (a
//!   specialisation of a member function template) is not followed: its
//!   key is named by its template (`digest.md` §3.4) and needs no record
//!   (§9.2). Members of an explicit or a partial specialisation of the
//!   class are declared there and linked to nothing: they are other
//!   entities, and their names are `keep`s (`decls`, `refs`).
//! - **`unsupported` on every follower.** A follower keeps the rule
//!   `Keys::rename_rule` gives for its key (§4.1: a rule is a function of
//!   key and request), and its key follows the pattern member's, on which
//!   the digest makes a rule indeterminate
//!   (`key/rule-on-instantiated-member`, `digest.md` §3.4). So it is
//!   flagged, as `followers::followed_patterns` flags a seed whose key
//!   follows a pattern (`spec/landed/edits/WP-071.md` decision 14). The
//!   flag goes when the digest accepts a rule on an instantiated member
//!   that gives the name the followed pattern's rule gives
//!   (`spec/landed/edits/WP-073.md`, "For the digest").
//! - **`unreachable` for a member's explicit instantiation** (part B):
//!   `template void S<long>::f();` marks the follower
//!   `ExplicitInstantiationDefinition` (or `…Declaration`) and Kiln
//!   records its name in neither table (no extent row; the written
//!   reference of role `SpecTarget` is Kiln's stage 3, WP-1932, not in the
//!   pin). An explicit specialisation of a member
//!   (`template <> void S<int>::f() {}`) has an extent row and is claimed
//!   by `decls` (a `replace` with a literal piece); it is `unreachable`
//!   only when the row has no `decl_names` entry (a name a macro spells),
//!   as `subjects::flag_unreachable` decides for a function template's
//!   specialisation.
//!
//! **Cost.** Nothing unless a seed is a member of a class template; then
//! one pass over the unit's declarations, comparing each one's
//! identifier with the old name; keys are computed only along chains
//! that reach a seed.

use std::collections::{HashMap, HashSet};

use slip_kiln::kiln::ast::decl::{FriendKind, TemplLinkKind};
use slip_kiln::kiln::ast::templates::SpecKind;
use slip_kiln::kiln::ast::{Ast, DeclId};
use slip_kiln::written;
use slip_model::edits::{KeyId, Role, subject_flags};
use slip_model::keys::{KeyKind, Name};

use super::{CTX_LIMIT, Ctx, enclosing};

/// Collects the followers of the seeds that are members of class
/// templates (module documentation).
pub(crate) fn collect(ctx: &mut Ctx<'_, '_, '_>) {
    let ast = ctx.unit.ast();
    // What each entity resolved to: the subject it is (a seed of a class
    // template, or a follower), or `None`, not one.
    let mut memo: HashMap<DeclId, Option<KeyId>> = HashMap::new();
    for (id, e) in ctx.subjects.seeds() {
        if in_class_template(ctx, e) {
            memo.insert(e, Some(id));
        }
    }
    if memo.is_empty() {
        return;
    }
    let named: HashSet<DeclId> = ctx.named.old.iter().map(|d| d.decl).collect();
    for d in ast.decls.ids() {
        if !named_old(ast, d, ctx.old) || !follows_a_pattern(ast, d) {
            continue;
        }
        let e = ctx.keys.entity(d);
        resolve(ctx, &mut memo, &named, e, 0);
    }
}

/// A class template encloses `e`: one of its scopes, at any depth, has a
/// key of kind `class_template`.
fn in_class_template(ctx: &mut Ctx<'_, '_, '_>, e: DeclId) -> bool {
    let ast = ctx.unit.ast();
    let mut at = e;
    for _ in 0..CTX_LIMIT {
        let Some(s) = enclosing(ast, at) else { return false };
        let s = ctx.keys.entity(s);
        if ctx.keys.key(s).is_ok_and(|k| k.kind == KeyKind::ClassTemplate) {
            return true;
        }
        at = s;
    }
    false
}

/// The AST names `d` by the identifier `old`.
fn named_old(ast: &Ast, d: DeclId, old: &[u8]) -> bool {
    ast.decls.get(d).name().as_ident().is_some_and(|s| ast.symbols().as_str(s).as_bytes() == old)
}

/// `d` is linked to a pattern it was instantiated from, and is not a
/// specialisation (whose link is to its template: `digest.md` §3.4).
fn follows_a_pattern(ast: &Ast, d: DeclId) -> bool {
    !matches!(
        ast.templ_link(d).get(),
        Some((TemplLinkKind::Specialization | TemplLinkKind::PartialSpecialization, _))
    ) && ast.instantiated_from(d).is_some()
}

/// The patterns entity `e` was instantiated from, as entities, each once:
/// asked of every declaration of `e`, as the digest asks
/// (`Walk::instantiated_patterns`): a specialisation's link is not one,
/// and a friend pattern that does not define the function is not
/// followed.
fn patterns(ctx: &Ctx<'_, '_, '_>, e: DeclId) -> Vec<DeclId> {
    let ast = ctx.unit.ast();
    let mut out = Vec::new();
    for d in ast.redecls_of(e) {
        if !follows_a_pattern(ast, d) {
            continue;
        }
        let Some(p) = ast.instantiated_from(d).map(|p| ctx.keys.entity(p)) else { continue };
        if p != e
            && (ast.decls.get(p).misc().friend() == FriendKind::None || ast.definition(p).is_some())
            && !out.contains(&p)
        {
            out.push(p);
        }
    }
    out
}

/// The subject entity `e` is, adding it as a follower when a pattern it
/// was instantiated from is a subject (a seed of a class template, or a
/// follower of one) and its key has the old name. `depth` bounds the
/// chain (each step leaves an instantiation for what it was made from).
fn resolve(
    ctx: &mut Ctx<'_, '_, '_>,
    memo: &mut HashMap<DeclId, Option<KeyId>>,
    named: &HashSet<DeclId>,
    e: DeclId,
    depth: u32,
) -> Option<KeyId> {
    if let Some(&r) = memo.get(&e) {
        return r;
    }
    // Decided `None` first, so that a malformed cycle ends.
    memo.insert(e, None);
    if depth >= CTX_LIMIT {
        return None;
    }
    let mut of = Vec::new();
    for p in patterns(ctx, e) {
        if let Some(id) = resolve(ctx, memo, named, p, depth + 1)
            && !of.contains(&id)
        {
            of.push(id);
        }
    }
    if of.is_empty() || !key_named_old(ctx, e) {
        return None;
    }
    let id = ctx.subjects.add(ctx.keys, e, Role::Follower, ctx.new, &of)?;
    // Its key follows the pattern member's: a rule on it is
    // `key/rule-on-instantiated-member` (module documentation).
    ctx.subjects.flag(id, subject_flags::UNSUPPORTED);
    if hidden(ctx, named, e) {
        ctx.subjects.flag(id, subject_flags::UNREACHABLE);
    }
    memo.insert(e, Some(id));
    Some(id)
}

/// `Keys::key` of `e` is named `identifier(old)` (§9.2: "when its key has
/// the old name").
fn key_named_old(ctx: &mut Ctx<'_, '_, '_>, e: DeclId) -> bool {
    ctx.keys.key(e).is_ok_and(|k| matches!(&k.name, Name::Identifier(n) if n.as_slice() == ctx.old))
}

/// A declaration of follower `e` names it where neither of Kiln's tables
/// gives the name: an explicit instantiation of the member (no extent
/// row; `SpecTarget` is stage 3), or an explicit specialisation of it
/// whose row has no `decl_names` entry, or that no declaration names.
fn hidden(ctx: &Ctx<'_, '_, '_>, named: &HashSet<DeclId>, e: DeclId) -> bool {
    let unit = ctx.unit;
    let ast = unit.ast();
    let mut specialised = false;
    let mut any = false;
    for d in ast.redecls_of(e) {
        match ast.member_spec_info(d).map(|(_, m)| m.kind) {
            Some(
                SpecKind::ExplicitInstantiationDeclaration
                | SpecKind::ExplicitInstantiationDefinition,
            ) => return true,
            Some(SpecKind::ExplicitSpecialization) => specialised = true,
            _ => {}
        }
        let has = named.contains(&d);
        any |= has;
        if !has && written::row(unit, d).is_some() {
            return true;
        }
    }
    specialised && !any
}
