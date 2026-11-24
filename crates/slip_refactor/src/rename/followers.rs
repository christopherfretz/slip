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
//!   of its target and the hider Kiln names (`hidden_shadow`). The digest
//!   writes no unit for it (`docs/wp/WP-119.md` item 6), so the
//!   using-declaration is no party; renaming one of the other two and not
//!   the other is a wrong edit: a hidden declaration becomes visible, or a
//!   visible one hidden. So that mix is reduce's `mixed_follower`, and a
//!   using-declaration is looked at when a subject hides what it names
//!   too (`spec/edits.md` §9.2). A shadow hidden by an implicit member
//!   (declared on use) stays a unit of the digest, but such a shadow and
//!   its hider are an `operator=` or a constructor, which no rename
//!   reaches: it is never looked at here.
//! - **What a using-declaration names** (`docs/wp/WP-127.md` item 1) is
//!   read from its declarator ([`named_set`]), not from its shadows
//!   alone: a member declared before the using-declaration that hides or
//!   overrides a named declaration leaves it with no shadow at all. Such
//!   a declaration counts as a hidden shadow would, its hider the member
//!   that overrides it or, when none does, every member of the class
//!   named like it (all in the using-declaration's `of`, [`of_using`]).
//!   A set that cannot be told makes the follower `unsupported` when the
//!   rename is near it.
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
//!   key, as everywhere: whether a subject has a rule is a function of its
//!   key and the request, never of what else the observer saw (§4.1), so
//!   that two observers never give one id two rules (reduce §5.2 rule 1).
//!   The digest accepts that rule where it gives exactly the key following
//!   the pattern gives (`docs/wp/WP-119.md` item 1), so it is not flagged.

use slip_kiln::kiln::ast::decl::{DeclKind, FnStorage};
use slip_kiln::kiln::ast::types::{Nns, TypeView};
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
    let (named, known) = named_set(ctx, u, &shadows);
    if named.is_empty() && known {
        ctx.claims.unresolved(&ctx.sites, d.name);
        ctx.settled.insert(u);
        return;
    }
    if !named.iter().any(Shadow::relevant) && known {
        // It names no subject, and no subject hides what it names:
        // `decls::claim` keeps its name.
        return;
    }
    if !known && !named.iter().any(Shadow::relevant) && !class_member_subject(ctx, u) {
        // What it names cannot be told, and nothing of the rename is near
        // it: `decls::claim` keeps its name.
        return;
    }
    ctx.settled.insert(u);
    let complete = known && named.iter().all(Shadow::keyed);
    let mut of = of_using(&named);
    if !known {
        unknown_of(ctx, u, &mut of);
    }
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

/// One declaration a using-declarator names: through the shadow Kiln
/// made for it, or (`decl` `None`) one Kiln made no shadow for because a
/// member of the class declared before the using-declaration hides or
/// overrides it ([`named_set`]).
struct Shadow {
    decl: Option<DeclId>,
    /// The key id of its target's origin: the subject's, or the entity's
    /// (`None`: no key).
    id: Option<KeyId>,
    /// The target is a subject.
    subject: bool,
    /// Not introduced: hidden or overridden by a member of the
    /// using-declaration's class ([namespace.udecl]/14). For a shadow,
    /// Kiln's `hidden_shadow`; for a declaration without one, always.
    hidden: bool,
    /// The member that hides it, when it is known: Kiln's `hidden_shadow`,
    /// or the member that overrides it (`overrides`).
    hider: Option<Party>,
    /// A declaration without a shadow that no member overrides (a
    /// non-virtual function hidden by a member that corresponds to it):
    /// every member of the class named like it that may correspond to it,
    /// one of which hides it ([`named_set`]). It and they are renamed
    /// together or not at all.
    hiders: Vec<Party>,
}

impl Shadow {
    /// It names a subject, or a subject hides what it names.
    fn relevant(&self) -> bool {
        self.subject
            || self.hider.is_some_and(|h| h.subject)
            || self.hiders.iter().any(|h| h.subject)
    }

    /// Every party it depends on has a key.
    fn keyed(&self) -> bool {
        self.id.is_some()
            && self.hider.is_none_or(|h| h.id.is_some())
            && self.hiders.iter().all(|h| h.id.is_some())
    }
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
        named.push(Shadow { decl: Some(s), id, subject, hidden, hider, hiders: Vec::new() });
    }
    named
}

/// What the using-declarator of `u` names (`docs/wp/WP-127.md` item 1):
/// the declarations of its shadows (`shadows`) and, at class scope, those
/// Kiln made no shadow for. The second boolean is `false` when the set
/// cannot be told; the first is then the shadows alone.
///
/// Kiln makes no shadow for a declaration that a member of the class
/// declared **before** the using-declaration hides or overrides
/// ([namespace.udecl]/14; `kiln_sema` `introduce_shadow`), and a hidden
/// one for a member declared after it. So the shadows depend on the order
/// of the members, and the set is read instead from what the declarator
/// names, which does not: the members of the nominated class named like
/// it, from the class's lookup table (`Ast::lookup_local`: what the class
/// declares; a using-shadow of the class is its target, one the class
/// hides is no member), or, when it declares none, the one set its bases
/// give ([class.member.lookup]); bases that give different sets are not
/// resolved here (the set cannot be told). A shadow whose target is not in
/// the set also makes it unknown: the reading and the front end disagree.
///
/// Each declaration of the set without a shadow is hidden. Its hider is
/// the member of the class that overrides it (Kiln's `overrides`): the two
/// are of one virtual family, renamed together or not at all. One that no
/// member overrides is hidden by a member that corresponds to it, which
/// Kiln does not name: every member of the class named like it that may
/// correspond to it ([`may_correspond`]) stands in (`Shadow::hiders`), a
/// superset where correspondence cannot be told exactly. A declaration
/// without a shadow that no member could hide (an earlier
/// using-declarator of the class introduced it) is introduced.
fn named_set(ctx: &mut Ctx<'_, '_, '_>, u: DeclId, shadows: &[DeclId]) -> (Vec<Shadow>, bool) {
    let mut named = shadows_named(ctx, shadows);
    let ast = ctx.unit.ast();
    let Some(class) = ast.decls.get(u).ctx().filter(|&c| ast.decls.record(c).is_some()) else {
        return (named, true);
    };
    let Some(Nns::Type { ty, .. }) = ast.using_qualifier(u).map(|q| ast.types.nns_view(q)) else {
        return (named, true);
    };
    let Some(nominated) = ast.types.as_record(ast.types.canonical(ty)) else {
        return (named, true);
    };
    let name = ast.decls.get(u).name();
    let Some(found) = member_lookup(ast, nominated, name, 0) else { return (named, false) };
    let targets: Vec<DeclId> =
        shadows.iter().filter_map(|&s| ast.using_target(s)).map(|t| ast.first(t)).collect();
    if targets.iter().any(|t| !found.contains(t)) {
        return (named, false);
    }
    let members = own_members(ast, class, name);
    for x in found.into_iter().filter(|x| !targets.contains(x)) {
        // The members that can hide `x`: those that correspond to it,
        // where that can be told exactly, else every one.
        let members: Vec<DeclId> =
            members.iter().copied().filter(|&m| may_correspond(ast, m, x)).collect();
        let Party { id, subject } = party(ctx, x);
        let overrider = members.iter().copied().find(|&m| {
            let list = ast.decls.cold.overrides.get(ast.first(m)).copied();
            let list = list.unwrap_or(slip_kiln::kiln::ast::ListId::EMPTY);
            ast.lists.iter::<DeclId>(list).any(|o| ast.first(o) == x)
        });
        let entry = match overrider {
            Some(m) => Shadow {
                decl: None,
                id,
                subject,
                hidden: true,
                hider: Some(party(ctx, m)),
                hiders: Vec::new(),
            },
            None if members.is_empty() => {
                Shadow { decl: None, id, subject, hidden: false, hider: None, hiders: Vec::new() }
            }
            None => {
                let hiders = members.iter().map(|&m| party(ctx, m)).collect();
                Shadow { decl: None, id, subject, hidden: true, hider: None, hiders }
            }
        };
        named.push(entry);
    }
    (named, true)
}

/// The members `class` declares named `name`, shadows and
/// using-declarations left out: those that can hide what a
/// using-declarator of the class names.
fn own_members(
    ast: &Ast,
    class: DeclId,
    name: slip_kiln::kiln::ast::names::DeclName,
) -> Vec<DeclId> {
    ast.lookup_local(class, name)
        .iter()
        .filter(|&m| {
            !matches!(
                ast.decls.get(m).kind(),
                DeclKind::UsingShadow
                    | DeclKind::ConstructorUsingShadow
                    | DeclKind::Using
                    | DeclKind::UnresolvedUsingValue
                    | DeclKind::UnresolvedUsingTypename
                    | DeclKind::UnresolvedUsingIfExists
            )
        })
        .collect()
}

/// What decides whether a member hides a declaration a using-declarator
/// names ([namespace.udecl]/14: the same parameter-type-list,
/// cv-qualification and ref-qualifier), told exactly: a non-static member
/// function that is no template, its type not dependent, with the canonical
/// parameter types, variadic-ness and qualifiers of its function type.
/// `None` when it cannot be told so.
fn hiding_signature(ast: &Ast, d: DeclId) -> Option<(Vec<QualType>, bool, u8, u8)> {
    if ast.decls.get(d).kind() != DeclKind::CXXMethod {
        return None;
    }
    if ast.decls.function(d).is_none_or(|f| f.spec.storage() == FnStorage::Static) {
        return None;
    }
    let q = ast.decls.get(d).ty()?;
    if ast.types.is_dependent(q) {
        return None;
    }
    let p = ast.types.proto(q)?;
    let params = ast.types.params(p).iter().map(|&t| ast.types.canonical(t)).collect();
    let info = p.info();
    Some((params, info.variadic(), info.this_quals().bits(), info.ref_qual() as u8))
}

/// `m` may hide `x` (`docs/wp/WP-127.md` review, F2): `false` only when
/// both signatures are told exactly ([`hiding_signature`]) and differ.
/// Where they are not, `m` counts as a hider: a superset, never fewer.
fn may_correspond(ast: &Ast, m: DeclId, x: DeclId) -> bool {
    match (hiding_signature(ast, m), hiding_signature(ast, x)) {
        (Some(a), Some(b)) => a == b,
        _ => true,
    }
}

/// When what `u` names cannot be told (`named_set`'s `false`), the
/// members of its class named like it, which may hide any of it: they
/// join the using-declaration's `of`, so that the follower is selected,
/// and refused `unsupported`, when the rename renames them, and is
/// `mixed_follower` when it renames part of them or part of the shadows
/// (`docs/wp/WP-127.md` review, F1). Without them a hider that is a
/// subject left the follower unselected and its flag inert.
fn unknown_of(ctx: &mut Ctx<'_, '_, '_>, u: DeclId, of: &mut Vec<KeyId>) {
    let ast = ctx.unit.ast();
    let Some(class) = ast.decls.get(u).ctx().filter(|&c| ast.decls.record(c).is_some()) else {
        return;
    };
    for m in own_members(ast, class, ast.decls.get(u).name()) {
        of.extend(party(ctx, m).id);
    }
    of.sort_unstable();
    of.dedup();
}

/// [class.member.lookup] of `name` in class `c`, as far as this reading
/// goes (`named_set`): the first declarations of what `c` declares named
/// so (a using-shadow taken to its target, one `c` hides left out), or,
/// when it declares none, the one set every base that gives one gives.
/// `None` when bases give different sets, or the chain is too deep.
fn member_lookup(
    ast: &Ast,
    c: DeclId,
    name: slip_kiln::kiln::ast::names::DeclName,
    depth: u32,
) -> Option<Vec<DeclId>> {
    if depth >= 64 {
        return None;
    }
    let mut own = Vec::new();
    for d in ast.lookup_local(c, name).iter() {
        let t = match ast.decls.get(d).kind() {
            DeclKind::UsingShadow | DeclKind::ConstructorUsingShadow => {
                if ast.decls.hidden_shadow(d).is_some() {
                    continue;
                }
                ast.using_target(d)?
            }
            DeclKind::Using
            | DeclKind::UnresolvedUsingValue
            | DeclKind::UnresolvedUsingTypename
            | DeclKind::UnresolvedUsingIfExists => continue,
            _ => d,
        };
        let t = ast.first(t);
        if !own.contains(&t) {
            own.push(t);
        }
    }
    if !own.is_empty() {
        return Some(own);
    }
    let def = ast.definition(c).unwrap_or(c);
    let mut got: Option<Vec<DeclId>> = None;
    for b in ast.decls.bases(def, &ast.lists) {
        let base = ast.types.as_record(ast.types.canonical(b.ty))?;
        let mut set = member_lookup(ast, base, name, depth + 1)?;
        if set.is_empty() {
            continue;
        }
        set.sort_unstable();
        match &got {
            None => got = Some(set),
            Some(g) if *g == set => {}
            Some(_) => return None,
        }
    }
    Some(got.unwrap_or_default())
}

/// A member of `u`'s class named like the using-declarator is a subject:
/// what decides whether a using-declaration whose named set cannot be
/// told is refused.
fn class_member_subject(ctx: &mut Ctx<'_, '_, '_>, u: DeclId) -> bool {
    let ast = ctx.unit.ast();
    let Some(class) = ast.decls.get(u).ctx() else { return false };
    let members: Vec<DeclId> = ast.lookup_local(class, ast.decls.get(u).name()).iter().collect();
    members.into_iter().any(|m| party(ctx, m).subject)
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

/// The using-declaration's `of` (`docs/wp/WP-127.md` item 1): what it
/// introduces, and, for each declaration it names that is hidden without
/// a shadow and overridden by no member (`Shadow::hiders`), that
/// declaration and its possible hiders. With a shadow, a hidden
/// declaration's pair is the shadow's follower (`shadow_followers`);
/// without one nothing else can carry it. The pair must be renamed
/// together: the hider renamed alone unhides the declaration, which the
/// kept name then introduces; the declaration renamed alone with the
/// name is introduced unhidden. Both are a change of the program, and the
/// using-declaration follows them so that the mix is `mixed_follower`.
/// The target and the hider are in one scope each, and every overload of
/// the name in the target's scope is a seed (§9.1), so the declarations
/// introduced beside it are renamed with it.
fn of_using(named: &[Shadow]) -> Vec<KeyId> {
    let mut of = introduced(named);
    for n in named.iter().filter(|n| n.decl.is_none() && !n.hiders.is_empty()) {
        of.extend(n.id);
        of.extend(n.hiders.iter().filter_map(|h| h.id));
    }
    of.sort_unstable();
    of.dedup();
    of
}

/// Each shadow whose target is a subject is a follower of it. A hidden
/// shadow is a follower whatever its target, of its target and the member
/// that hides it (`spec/edits.md` §9.2): the digest writes no unit for it
/// (`spec/digest.md` §2.1, `docs/wp/WP-119.md` item 6), so what the
/// using-declaration does no longer matters, but the hider must still hide
/// the target, or a hidden declaration becomes visible (or a visible one
/// hidden). So the two are renamed together or neither; any mix is
/// reduce's `mixed_follower`. (A shadow hidden by an implicit member stays
/// a unit of the digest, but it and its hider are an `operator=` or a
/// constructor, which no rename reaches.) A party with no key cannot be in
/// `of`: the shadow is `unsupported`.
fn shadow_followers(ctx: &mut Ctx<'_, '_, '_>, named: &[Shadow], using: Option<KeyId>) {
    for n in named {
        // A declaration without a shadow has nothing to record: its pair is
        // in the using-declaration's `of` (`of_using`), or of one family.
        let Some(decl) = n.decl else { continue };
        if let Some(h) = n.hider {
            let parties = [n.id, h.id];
            let of: Vec<KeyId> = parties.iter().flatten().copied().collect();
            let f = ctx.subjects.add(ctx.keys, decl, Role::Follower, ctx.new, &of);
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
        ctx.subjects.add(ctx.keys, decl, Role::Follower, ctx.new, &[t]);
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
/// with no key: `unsupported`. Its key follows the pattern's; its rule is
/// accepted where it agrees with the pattern's (`docs/wp/WP-119.md` item
/// 1), as every member of an instantiated specialisation's is
/// (`spec_members`), so that is no flag; its shadows are followers as
/// above.
pub(crate) fn instantiated_unfollowed_using(ctx: &mut Ctx<'_, '_, '_>, u: DeclId) {
    let ast = ctx.unit.ast();
    let shadows: Vec<DeclId> = ast.using_shadows(u).collect();
    let (named, known) = named_set(ctx, u, &shadows);
    let relevant = named.iter().any(Shadow::relevant);
    if !relevant && (known || !class_member_subject(ctx, u)) {
        return;
    }
    let complete = known && named.iter().all(Shadow::keyed);
    let mut of = of_using(&named);
    if !known {
        unknown_of(ctx, u, &mut of);
    }
    let pattern = ast.instantiated_from(u).and_then(|p| ctx.keys.key_id(ctx.keys.entity(p)).ok());
    let keyed = pattern.is_some();
    of.extend(pattern);
    of.sort_unstable();
    of.dedup();
    let f = ctx.subjects.add(ctx.keys, u, Role::Follower, ctx.new, &of);
    // A party with no key cannot be in `of`: refused, as for any
    // using-declaration follower (`using_declaration`).
    if let Some(f) = f
        && !(complete && keyed)
    {
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
