//! Followers (§9.2, `docs/wp/WP-072.md`), each with `of` the entities it
//! follows:
//!
//! - **A using-declaration** named like the old name whose shadows name a
//!   subject (at namespace, class or block scope). The using-declaration
//!   is a follower whose `of` is every entity it names, subject or not, so
//!   that an overload set partly selected is reduce's `mixed_follower`
//!   (§5.2); each shadow whose target is a subject is a follower of that
//!   subject (its key holds the plain name, `digest.md` §3.4, so it needs
//!   a rule of its own). The using-declaration's name token
//!   (`written::decl_names`, kind `UsingDeclaration`) is claimed `replace`
//!   with a literal piece and the using-declaration as its subject (§9.3:
//!   Kiln records no reference there; the follower's rule checks it). A
//!   using-declaration with no key (one at block scope is no unit) cannot
//!   be a follower: its name is claimed for the subjects it names, or
//!   `refuse(mixed_set)` when it also names an entity that is not one. A
//!   using-declaration that introduces no shadow (it names what an earlier
//!   one of its scope already introduced) is followed through the
//!   declaration it redeclares when Kiln links one, else claimed
//!   `unresolved`: what it names is not recorded. A dependent
//!   using-declaration (`using B<T>::f;`) names nothing until
//!   instantiated: its name is claimed `unresolved`.
//! - **A reference found through a shadow** (`via_using`): the token
//!   denotes the referent and `refs::claim` claims it so; the shadow is a
//!   follower by the row above, since the using-declaration that
//!   introduced it is named like the old name.
//! - **An unnamed class or enumeration** a subject names (`digest.md` §3.3:
//!   its key's name is `unnamed_by` the old name, its parent the
//!   subject's): a follower of the subject whose rule is made from its key
//!   with the inner name replaced (§9.2). A subject whose type is such a
//!   class without a key is `unsupported`.
//! - **A using-declaration in a template's pattern** is a follower as any
//!   is (`docs/wp/WP-120.md` item 1): the using-declaration of an
//!   instantiated class is a follower of the pattern's (`spec_members`),
//!   and its shadows, which Kiln does not link to the pattern's, are
//!   followers of their targets ([`instantiated_using`]).
//! - **A hidden shadow** (`docs/wp/WP-120.md` item 3 and its fix round):
//!   a declaration a member of the using-declaration's class hides or
//!   overrides is not introduced ([namespace.udecl]/14), so it is left
//!   out of the using-declaration's `of` (unless every one is). The
//!   shadow Kiln still makes for it is a follower whatever its target,
//!   of its three parties: the target, the using-declaration and the
//!   hider Kiln names (`hidden_shadow`). While the digest writes it as a
//!   unit, renaming some of the three and not the others is a wrong
//!   edit: the shadow disappears, or a hidden declaration becomes
//!   visible. So any mix is reduce's `mixed_follower`, and a
//!   using-declaration is looked at when a subject hides what it names
//!   too (`spec/edits.md` §9.2).
//! - **An instantiated dependent using-declaration**
//!   ([`instantiated_unfollowed_using`]): follows what it introduces and
//!   the pattern's using-declaration, whose name is `unresolved`.
//! - A function or variable that a class template's pattern befriends
//!   with a declaration that is its own entity (keyed in the template,
//!   `spec/landed/digest/WP-103.md` finding 2) is a follower of the
//!   subject it declares, with its own rule: same name, same
//!   kind, declared in the subject's scope.
//! - A subject whose key follows a pattern in this observer
//!   (`spec/landed/digest/WP-103.md` finding 2: a function a class
//!   template's pattern befriends with a definition, the template
//!   instantiated here) keeps the rule `Keys::rename_rule` gives for its
//!   key, which the digest does not accept here
//!   (`key/rule-on-instantiated-member`): it is `unsupported`. Whether a
//!   subject has a rule is a function of its key and the request, never
//!   of what else the observer saw (§4.1), so that two observers never
//!   give one id two rules (reduce §5.2 rule 1).

use slip_kiln::kiln::ast::decl::{DeclKind, FriendKind, TemplLinkKind};
use slip_kiln::kiln::ast::types::TypeView;
use slip_kiln::kiln::ast::{Ast, DeclId, QualType};
use slip_kiln::written::{self, DeclName, DeclNameKind};
use slip_model::edits::{KeyId, Reason, Role, subject_flags};
use slip_model::keys::{Key, Name, Rule};

use super::{CTX_LIMIT, Ctx, Written, enclosing};

/// Collects what the module documentation says.
pub(crate) fn collect(ctx: &mut Ctx<'_, '_, '_>) {
    using_declarations(ctx);
    unnamed_types(ctx);
    friends_in_patterns(ctx);
    followed_patterns(ctx);
}

fn followed_patterns(ctx: &mut Ctx<'_, '_, '_>) {
    let ast = ctx.unit.ast();
    for (id, e) in ctx.subjects.entities() {
        let mut patterns: Vec<DeclId> = Vec::new();
        for d in ast.redecls_of(e) {
            // As the digest asks (`instantiated_patterns`): a
            // specialisation's link is to its template, not a pattern.
            if matches!(
                ast.templ_link(d).get(),
                Some((TemplLinkKind::Specialization | TemplLinkKind::PartialSpecialization, _))
            ) {
                continue;
            }
            let Some(p) = ast.instantiated_from(d).map(|p| ctx.keys.entity(p)) else { continue };
            if p != e
                && (ast.decls.get(p).misc().friend() == FriendKind::None
                    || ast.definition(p).is_some())
                && !patterns.contains(&p)
            {
                patterns.push(p);
            }
        }
        if patterns.is_empty() {
            continue;
        }
        // The rule stays: whether a subject has one is a function of its
        // key and the request, never of what else this observer saw.
        ctx.subjects.flag(id, subject_flags::UNSUPPORTED);
    }
}

fn using_declarations(ctx: &mut Ctx<'_, '_, '_>) {
    let names = std::mem::take(&mut ctx.named.old);
    for d in names.iter().filter(|d| d.kind == DeclNameKind::UsingDeclaration) {
        using_declaration(ctx, d);
    }
    ctx.named.old = names;
}

/// One using-declaration named like the old name (module documentation).
fn using_declaration(ctx: &mut Ctx<'_, '_, '_>, d: &DeclName) {
    let unit = ctx.unit;
    let ast = unit.ast();
    let u = d.decl;
    if matches!(
        ast.decls.get(u).kind(),
        DeclKind::UnresolvedUsingValue
            | DeclKind::UnresolvedUsingTypename
            | DeclKind::UnresolvedUsingIfExists
    ) {
        ctx.claims.unresolved(&ctx.sites, d.name);
        ctx.settled.insert(u);
        return;
    }
    // The shadows: its own, or those of the declaration it redeclares.
    let mut shadows: Vec<DeclId> = ast.using_shadows(u).collect();
    if shadows.is_empty() {
        shadows = ast.redecls_of(u).flat_map(|x| ast.using_shadows(x)).collect();
    }
    if shadows.is_empty() {
        ctx.claims.unresolved(&ctx.sites, d.name);
        ctx.settled.insert(u);
        return;
    }
    let named = shadows_named(ctx, &shadows);
    if !named.iter().any(|n| n.subject || n.hider.is_some_and(|h| h.subject)) {
        // It names no subject, and no subject hides what it names:
        // `decls::claim` keeps its name.
        return;
    }
    ctx.settled.insert(u);
    let complete = named.iter().all(|n| n.id.is_some());
    let of = introduced(&named);
    if let Some(f) = ctx.subjects.add(ctx.keys, u, Role::Follower, ctx.new, &of) {
        if !complete {
            ctx.subjects.flag(f, subject_flags::UNSUPPORTED);
        }
        ctx.claims.replace(&ctx.sites, d.name, &[f], Written::Literal);
        shadow_followers(ctx, &named, Some(f));
    } else {
        // No key: no follower. The name is claimed for what it names.
        let subjects: Vec<KeyId> =
            named.iter().filter(|n| n.subject).filter_map(|n| n.id).collect();
        if named.iter().all(|n| n.subject) {
            ctx.claims.replace(&ctx.sites, d.name, &subjects, Written::Literal);
        } else {
            ctx.claims.refuse(&ctx.sites, d.name, &subjects, Reason::MixedSet);
        }
        shadow_followers(ctx, &named, None);
    }
}

/// One shadow of a using-declaration: what it names.
struct Shadow {
    decl: DeclId,
    /// The key id of its target's origin: the subject's, or the entity's
    /// (`None`: no key).
    id: Option<KeyId>,
    /// The target is a subject.
    subject: bool,
    /// Kiln records it hidden by a member of the using-declaration's class
    /// (`hidden_shadow`, [namespace.udecl]/14).
    hidden: bool,
    /// The member that hides it, when it is hidden.
    hider: Option<Party>,
}

/// An entity a hidden shadow depends on: its key id (the subject's, or
/// the entity's; `None`: no key) and whether it is a subject.
#[derive(Clone, Copy)]
struct Party {
    id: Option<KeyId>,
    subject: bool,
}

/// The party `d` is, taken to its origin as a reference to it is.
fn party(ctx: &mut Ctx<'_, '_, '_>, d: DeclId) -> Party {
    let o = written::origin(ctx.unit, d);
    let subject = ctx.subjects.of_decl(ctx.keys, o);
    let id = subject.or_else(|| ctx.keys.key_id(ctx.keys.entity(o)).ok());
    Party { id, subject: subject.is_some() }
}

fn shadows_named(ctx: &mut Ctx<'_, '_, '_>, shadows: &[DeclId]) -> Vec<Shadow> {
    let unit = ctx.unit;
    let ast = unit.ast();
    let mut named = Vec::with_capacity(shadows.len());
    for &s in shadows {
        let hider = ast.decls.hidden_shadow(s).map(|h| party(ctx, h));
        let hidden = hider.is_some();
        let target = ast.using_target(s).map(|t| party(ctx, t));
        let Party { id, subject } = target.unwrap_or(Party { id: None, subject: false });
        named.push(Shadow { decl: s, id, subject, hidden, hider });
    }
    named
}

/// §9.2, the using-declaration's `of`: the entities it introduces. A
/// declaration hidden or overridden by a member of the using-declaration's
/// class is not introduced ([namespace.udecl]/14), so the name is right
/// unchanged whatever happens to it: it is left out, unless every one is
/// hidden (then the name must change exactly when they all do, or the
/// using-declarator would find nothing).
fn introduced(named: &[Shadow]) -> Vec<KeyId> {
    let any_introduced = named.iter().any(|n| !n.hidden);
    let mut of: Vec<KeyId> =
        named.iter().filter(|n| !n.hidden || !any_introduced).filter_map(|n| n.id).collect();
    of.sort_unstable();
    of.dedup();
    of
}

/// Each shadow whose target is a subject is a follower of it. A hidden
/// shadow is a follower whatever its target, of its three parties: its
/// target, its using-declaration `using` and the member that hides it
/// (`spec/edits.md` §9.2). While the digest writes a hidden shadow as a
/// unit, the shadow is right only when all three are renamed or none is:
/// the using-declarator must still name the target, and the hider must
/// still hide it. Any mix is reduce's `mixed_follower`. A party with no
/// key cannot be in `of`: the shadow is `unsupported`.
fn shadow_followers(ctx: &mut Ctx<'_, '_, '_>, named: &[Shadow], using: Option<KeyId>) {
    for n in named {
        if let Some(h) = n.hider {
            let parties = [n.id, using, h.id];
            let of: Vec<KeyId> = parties.iter().flatten().copied().collect();
            let f = ctx.subjects.add(ctx.keys, n.decl, Role::Follower, ctx.new, &of);
            match f {
                Some(f) if of.len() < parties.len() => {
                    ctx.subjects.flag(f, subject_flags::UNSUPPORTED);
                }
                Some(_) => {}
                // The shadow has no key: nothing can be recorded for it,
                // and the using-declaration is refused in its place.
                None => {
                    if let Some(u) = using {
                        ctx.subjects.flag(u, subject_flags::UNSUPPORTED);
                    }
                }
            }
            continue;
        }
        let (true, Some(t)) = (n.subject, n.id) else { continue };
        ctx.subjects.add(ctx.keys, n.decl, Role::Follower, ctx.new, &[t]);
    }
}

/// The shadows of `u`, a using-declaration of an instantiated class that
/// is follower `id` of its pattern's (`spec_members`): Kiln links the
/// using-declaration to the pattern's and not its shadows, whose keys
/// therefore hold their own plain names (`spec/digest.md` §3.4). Each
/// shadow whose target is a subject is a follower as in the pattern.
pub(crate) fn instantiated_using(ctx: &mut Ctx<'_, '_, '_>, u: DeclId, id: KeyId) {
    let shadows: Vec<DeclId> = ctx.unit.ast().using_shadows(u).collect();
    let named = shadows_named(ctx, &shadows);
    shadow_followers(ctx, &named, Some(id));
}

/// `u`, a using-declaration of an instantiated class whose pattern's is no
/// subject (a dependent one, `using Base<T>::f;`, names nothing until
/// instantiated, and its name is claimed `unresolved`): what the
/// instantiation names is read here, as for any using-declaration. When it
/// names a subject (or a subject hides what it names), it is a follower
/// with `of` what it introduces **and the pattern's using-declaration**:
/// its name is the pattern's, which this rename does not edit, so with a
/// subject selected it is reduce's `mixed_follower` (`docs/wp/WP-120.md`
/// fix round, question 3: renaming `Base<U>::f` would leave `using
/// Base<T>::f;` naming another `f`, which `Base<U>::f` hid). A pattern
/// with no key: `unsupported`. Its key follows the pattern's, so it is
/// `unsupported` besides, as every member of an instantiated
/// specialisation is (`spec_members`); its shadows are followers as above.
pub(crate) fn instantiated_unfollowed_using(ctx: &mut Ctx<'_, '_, '_>, u: DeclId) {
    let ast = ctx.unit.ast();
    let shadows: Vec<DeclId> = ast.using_shadows(u).collect();
    let named = shadows_named(ctx, &shadows);
    if !named.iter().any(|n| n.subject || n.hider.is_some_and(|h| h.subject)) {
        return;
    }
    let mut of = introduced(&named);
    let pattern = ast.instantiated_from(u).and_then(|p| ctx.keys.key_id(ctx.keys.entity(p)).ok());
    of.extend(pattern);
    of.sort_unstable();
    of.dedup();
    let f = ctx.subjects.add(ctx.keys, u, Role::Follower, ctx.new, &of);
    if let Some(f) = f {
        ctx.subjects.flag(f, subject_flags::UNSUPPORTED);
    }
    shadow_followers(ctx, &named, f);
}

fn unnamed_types(ctx: &mut Ctx<'_, '_, '_>) {
    let ast = ctx.unit.ast();
    for (id, e) in ctx.subjects.entities() {
        let Some(t) = ast.decls.get(e).ty().and_then(|q| declared_tag(ast, q)) else { continue };
        if !ast.decls.get(t).name().is_empty() {
            continue;
        }
        let Ok(k) = ctx.keys.key(t) else {
            // An unnamed class whose key the digest refuses: whether the
            // subject names it cannot be told, nor a rule made.
            ctx.subjects.flag(id, subject_flags::UNSUPPORTED);
            continue;
        };
        let parent = ctx.keys.key(e).map(|s| s.parent);
        if parent != Ok(k.parent) {
            continue;
        }
        let (old, new) = (ctx.old, ctx.new);
        if unnamed_rule(&k, old, new).is_none() {
            continue;
        }
        let mut rule = |keys: &mut slip_digest::query::Keys<'_>, x: DeclId| {
            keys.key(x).ok().and_then(|k| unnamed_rule(&k, old, new))
        };
        ctx.subjects.add_ruled(ctx.keys, t, Role::Follower, &[id], &mut rule);
    }
}

/// The class or enumeration a declaration of type `q` declares an object
/// of, through pointers, references, arrays and member pointers: the tag
/// an unnamed class's first declarator names (`digest.md` §3.3). Which
/// declaration names it is the key's to say; this only proposes it.
fn declared_tag(ast: &Ast, q: QualType) -> Option<DeclId> {
    let types = &ast.types;
    let mut q = types.canonical(q);
    for _ in 0..CTX_LIMIT {
        match types.view(q.ty()) {
            TypeView::Record(d) | TypeView::Enum(d) => return Some(ast.first(d)),
            TypeView::Pointer(x)
            | TypeView::LValueRef(x)
            | TypeView::RValueRef(x)
            | TypeView::IncompleteArray(x)
            | TypeView::MemberPointer { pointee: x, .. } => q = types.canonical(x),
            TypeView::ConstantArray(a) => q = types.canonical(a.elem),
            TypeView::ExtQual { base, .. } => q = types.canonical(base),
            _ => return None,
        }
    }
    None
}

/// §9.2: the rule of an unnamed class or enumeration whose key `k` is
/// named `unnamed_by(identifier(old))`: the key with the inner name
/// `new`, called by the name it has now. `None` for any other key.
fn unnamed_rule(k: &Key, old: &[u8], new: &[u8]) -> Option<Rule> {
    let Name::UnnamedBy(inner) = &k.name else { return None };
    if !matches!(inner.as_ref(), Name::Identifier(n) if n.as_slice() == old) {
        return None;
    }
    Some(Rule {
        parent: k.parent,
        kind: k.kind,
        name: Name::UnnamedBy(Box::new(Name::Identifier(new.to_vec()))),
        disc: k.disc,
        to_parent: k.parent,
        to_name: k.name.clone(),
    })
}

fn friends_in_patterns(ctx: &mut Ctx<'_, '_, '_>) {
    let unit = ctx.unit;
    let ast = unit.ast();
    let seeds = ctx.subjects.seeds();
    if seeds.is_empty() {
        return;
    }
    // Each seed's key and the entity of its scope. A friend in a pattern
    // has a key of its own (its parent is the template, its discriminator
    // differs), so it is matched on scope, kind and name, and follows
    // every seed it matches: all functions of that name in that scope are
    // seeds (§9.1), so whichever it redeclares is among them.
    let mut wanted = Vec::new();
    for (id, e) in seeds {
        if let (Ok(k), Some(s)) = (ctx.keys.key(e), enclosing(ast, e)) {
            wanted.push((id, k, ctx.keys.entity(s)));
        }
    }
    let names = std::mem::take(&mut ctx.named.old);
    for d in &names {
        if ctx.settled.contains(&d.decl) || !is_friend_in_pattern(ast, d.decl) {
            continue;
        }
        let o = written::origin(unit, d.decl);
        if ctx.subjects.of_decl(ctx.keys, o).is_some() {
            continue;
        }
        let Some(scope) = enclosing(ast, d.decl).map(|s| ctx.keys.entity(s)) else { continue };
        let Ok(k) = ctx.keys.key(ctx.keys.entity(o)) else { continue };
        let of: Vec<_> = wanted
            .iter()
            .filter(|(_, w, s)| *s == scope && w.kind == k.kind && w.name == k.name)
            .map(|(id, ..)| *id)
            .collect();
        if !of.is_empty() {
            ctx.subjects.add(ctx.keys, o, Role::Follower, ctx.new, &of);
        }
    }
    ctx.named.old = names;
}

/// `d` is declared as a friend (its lexical context is a class that is
/// not its semantic one) in a class that is, or is inside, the pattern
/// of a class template or a partial specialisation.
fn is_friend_in_pattern(ast: &Ast, d: DeclId) -> bool {
    let Some(lexical) = ast.decls.lexical_ctx(d) else { return false };
    if Some(lexical) == ast.decls.get(d).ctx() || ast.decls.record(lexical).is_none() {
        return false;
    }
    let mut at = Some(lexical);
    for _ in 0..CTX_LIMIT {
        let Some(x) = at else { return false };
        if ast.described_template(x).is_some()
            || ast.decls.get(x).kind() == DeclKind::ClassTemplatePartialSpecialization
        {
            return true;
        }
        at = ast.decls.get(x).ctx();
    }
    false
}
