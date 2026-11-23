//! Members of class templates (`spec/edits.md` §9.2, the follower "a
//! member of a class template specialisation"; `docs/wp/WP-073.md` parts
//! B and C).
//!
//! A subject in a class pattern (`families::in_class_pattern`: a class
//! template or a partial specialisation encloses it, no function in
//! between) is renamed as any subject is: its declarations and the
//! references to it and to its instantiations are claimed by `decls` and
//! `refs` (`written::origin` takes a member of an instantiated class to
//! the member of the template). That is a seed, a virtual candidate, and
//! a using-declaration follower or a shadow (`docs/wp/WP-120.md` item 1).
//! What this file adds is the followers:
//!
//! - **Followers.** Every member of an instantiated specialisation that
//!   Kiln links to the subject (`instantiated_from`, through the whole chain
//!   of patterns, asked of every declaration of the member as
//!   `spec/digest.md` §3.4 asks it), whose key has the old name, is a
//!   follower with `of` the subject it was instantiated from: the member
//!   of the template, or the follower in between for a member of a
//!   member template's specialisation. The using-declaration of an
//!   instantiated class is one (Kiln links it to the pattern's); its
//!   shadows, which Kiln does not link, are followers of their targets
//!   (`followers::instantiated_using`). A specialisation's own link (a
//!   specialisation of a member function template) is not followed: its
//!   key is named by its template (`digest.md` §3.4) and needs no record
//!   (§9.2). Members of an explicit or a partial specialisation of the
//!   class are declared there and linked to nothing: they are other
//!   entities, and their names are `keep`s (`decls`, `refs`).
//! - **The follower's rule.** A follower keeps the rule
//!   `Keys::rename_rule` gives for its key (§4.1: a rule is a function of
//!   key and request). Its key follows the pattern member's
//!   (`digest.md` §3.4), and the digest accepts a rule on it exactly when
//!   the rule gives the key following gives (`docs/wp/WP-119.md` item 1):
//!   the rule renames the plain name to the old one and keeps the parent,
//!   as the pattern member's rule renames its name. So it is not flagged
//!   for that (until WP-119 it was `unsupported`).
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
//! - **Names in no table flag the subjects in class patterns** (fix
//!   round; a subject in a pattern is unflagged only when every place the
//!   unit shows naming it is claimed or flags it, `spec/edits.md` §9.2;
//!   every such subject, not only the seeds, since WP-120):
//!   - a friend of a member of a dependent specialisation
//!     (`template <class T> friend void n::S<T>::f();`), which Kiln
//!     records as a `Friend` of a function **type**, with no declaration:
//!     its name, located through the friend's extent row, is claimed
//!     `unresolved`, and every subject in a class pattern is
//!     `unreachable`;
//!   - a member named like the old name in an instantiated
//!     specialisation (not an explicit specialisation of the class) that
//!     no declaration links to a pattern (`template <> template <class U>
//!     void S<int>::g() {}`): Kiln does not record which member it
//!     specialises, so every subject in a class pattern is
//!     `unsupported`.
//!
//!   An explicit instantiation of a member after the member's explicit
//!   specialisation is recorded nowhere at the pin, so nothing here can
//!   see it (`spec/landed/edits/WP-073.md`, fix round F3).
//!
//! **Cost.** Nothing unless a subject is in a class pattern; then one pass over
//! the unit's declarations, comparing each one's identifier with the old
//! name (and locating each function-type friend's name); keys are
//! computed only along chains that reach such a subject.

use std::collections::{HashMap, HashSet};

use slip_kiln::kiln::ast::decl::{DeclKind, FriendKind, FriendTarget, TemplLinkKind};
use slip_kiln::kiln::ast::templates::SpecKind;
use slip_kiln::kiln::ast::{Ast, DeclId};
use slip_kiln::sites::{Located, Reason};
use slip_kiln::written;
use slip_model::edits::{KeyId, Role, subject_flags};
use slip_model::keys::Name;

use super::families::in_class_pattern;
use super::followers;
use super::{CTX_LIMIT, Ctx};

/// Collects the followers of the subjects in class patterns, and flags
/// those subjects where a name of theirs is in no table or in no link
/// (module documentation).
pub(crate) fn collect(ctx: &mut Ctx<'_, '_, '_>) {
    let ast = ctx.unit.ast();
    // What each entity resolved to: the subject it is (one in a class
    // pattern, or a follower), or `None`, not one.
    let mut memo: HashMap<DeclId, Option<KeyId>> = HashMap::new();
    let mut in_patterns = Vec::new();
    for (id, e) in ctx.subjects.entities() {
        if in_class_pattern(ctx, e) {
            memo.insert(e, Some(id));
            in_patterns.push(id);
        }
    }
    if in_patterns.is_empty() {
        return;
    }
    let named: HashSet<DeclId> = ctx.named.old.iter().map(|d| d.decl).collect();
    let mut friend = false;
    let mut unlinked = false;
    for d in ast.decls.ids() {
        if ast.decls.get(d).kind() == DeclKind::Friend {
            friend |= friend_of_dependent_member(ctx, d);
            continue;
        }
        if !named_old(ast, d, ctx.old) {
            continue;
        }
        if follows_a_pattern(ast, d) {
            let e = ctx.keys.entity(d);
            let id = resolve(ctx, &mut memo, &named, e, 0);
            if ast.decls.get(d).kind() == DeclKind::Using {
                match id {
                    Some(id) => followers::instantiated_using(ctx, d, id),
                    None => followers::instantiated_unfollowed_using(ctx, d),
                }
            }
        } else {
            unlinked |= unlinked_member(ast, d);
        }
    }
    for id in in_patterns {
        if friend {
            ctx.subjects.flag(id, subject_flags::UNREACHABLE);
        }
        if unlinked {
            ctx.subjects.flag(id, subject_flags::UNSUPPORTED);
        }
    }
}

/// `d`, a `Friend` declaration, befriends a function the front end
/// records only as a type, with no declaration (a member of a dependent
/// specialisation, `template <class T> friend void n::S<T>::f();`), and
/// its declarator-id may be the old name: then the name is claimed
/// `unresolved`, and the caller flags the subjects in class patterns
/// `unreachable`. "May be":
/// `Sites::name_token` locates the name and its bytes are the old name's,
/// or it refuses the name before comparing them (a macro's body, a pasted
/// token, a file that is not owned); `not_plain` is a name spelled
/// otherwise.
fn friend_of_dependent_member(ctx: &mut Ctx<'_, '_, '_>, d: DeclId) -> bool {
    let ast = ctx.unit.ast();
    let FriendTarget::Type { ty, owned_tag: None } = ast.decls.friend_target(d) else {
        return false;
    };
    if !ast.types.is_function(ty) {
        return false;
    }
    let Some(name) = ast.decl_extents.get(d).map(|e| e.name).filter(|n| n.lo.is_valid()) else {
        return false;
    };
    match ctx.sites.name_token(name, ctx.old) {
        Located::At { .. } => {
            ctx.claims.unresolved(&ctx.sites, name);
            true
        }
        Located::Refused { reason, .. } => reason != Reason::NotPlain,
    }
}

/// `d`, named like the old name, no declaration of it linked to a
/// template or a pattern, is a member of an instantiated specialisation
/// of a class template (one that is not an explicit specialisation of
/// the class): the front end declared it there without recording what it
/// specialises (a member template explicitly specialised as a template,
/// `template <> template <class U> void S<int>::g() {}`). A
/// redeclaration without a link of its own (`template <> int S<int>::v =
/// 1;`) is not one: another declaration of its entity has the link.
fn unlinked_member(ast: &Ast, d: DeclId) -> bool {
    // A using-shadow is never linked (Kiln links its using-declaration),
    // and nothing can specialise one: the shadows of an instantiated
    // using-declaration are `followers::instantiated_using`'s.
    if matches!(ast.decls.get(d).kind(), DeclKind::UsingShadow | DeclKind::ConstructorUsingShadow) {
        return false;
    }
    let linked =
        |x: DeclId| ast.templ_link(x).get().is_some() || ast.instantiated_from(x).is_some();
    if linked(d) || ast.redecls_of(d).any(linked) {
        return false;
    }
    let Some(c) = ast.decls.get(d).ctx() else { return false };
    ast.spec_info(c).is_some_and(|(_, s)| s.kind != SpecKind::ExplicitSpecialization)
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
/// was instantiated from is a subject (one in a class pattern, or a
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
    // Its key follows the pattern member's, and its rule agrees with the
    // pattern's (module documentation): no flag for that.
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
