//! Followers (§9.2), conservative until `docs/wp/WP-072.md` replaces
//! this file, except for the friend declarations of `docs/wp/WP-071.md`
//! "Added after the review of WP-103", which are followers in full:
//!
//! - A using-declaration named like the old name that names a subject
//!   flags that subject `unreachable`, and is not claimed (its name is
//!   no reference Kiln records, `spec/landed/edits/WP-067.md`). A
//!   dependent using-declaration (`using B<T>::f;`) names nothing until
//!   instantiated: its name is claimed `unresolved`.
//! - A subject that names an unnamed class or enumeration (`digest.md`
//!   §3.3: its key's name is `unnamed_by` the old name) is `unsupported`.
//! - A function or variable that a class template's pattern befriends
//!   with a declaration that is its own entity (keyed in the template,
//!   `spec/landed/digest/WP-103.md` finding 2) is a follower of the
//!   subject it declares, with its own rule: same name, same kind and
//!   discriminator, and declared in the subject's scope.

use slip_kiln::kiln::ast::decl::DeclKind;
use slip_kiln::kiln::ast::{Ast, DeclId};
use slip_kiln::written::{self, DeclNameKind};
use slip_model::edits::{Role, subject_flags};
use slip_model::keys::Name;

use super::{CTX_LIMIT, Ctx, enclosing};

/// Collects what the module documentation says.
pub(crate) fn collect(ctx: &mut Ctx<'_, '_, '_>) {
    using_declarations(ctx);
    unnamed_types(ctx);
    friends_in_patterns(ctx);
}

fn using_declarations(ctx: &mut Ctx<'_, '_, '_>) {
    let unit = ctx.unit;
    let ast = unit.ast();
    let names = std::mem::take(&mut ctx.named.old);
    for d in names.iter().filter(|d| d.kind == DeclNameKind::UsingDeclaration) {
        if matches!(
            ast.decls.get(d.decl).kind(),
            DeclKind::UnresolvedUsingValue
                | DeclKind::UnresolvedUsingTypename
                | DeclKind::UnresolvedUsingIfExists
        ) {
            ctx.claims.unresolved(&ctx.sites, d.name);
            ctx.settled.insert(d.decl);
            continue;
        }
        let mut named = Vec::new();
        for shadow in ast.using_shadows(d.decl) {
            if let Some(t) = ast.using_target(shadow)
                && let Some(id) = ctx.subjects.of_decl(ctx.keys, written::origin(unit, t))
            {
                named.push(id);
            }
        }
        if !named.is_empty() {
            for id in named {
                ctx.subjects.flag(id, subject_flags::UNREACHABLE);
            }
            ctx.settled.insert(d.decl);
        }
    }
    ctx.named.old = names;
}

fn unnamed_types(ctx: &mut Ctx<'_, '_, '_>) {
    let ast = ctx.unit.ast();
    for (id, e) in ctx.subjects.entities() {
        let Some(ty) = ast.decls.get(e).ty() else { continue };
        let ty = ast.types.canonical(ty);
        let Some(t) = ast.types.as_record(ty).or_else(|| ast.types.as_enum(ty)) else { continue };
        let named_by_old = ctx.keys.key(t).is_ok_and(|k| {
            matches!(&k.name, Name::UnnamedBy(inner)
                if matches!(inner.as_ref(), Name::Identifier(n) if n.as_slice() == ctx.old))
        });
        if named_by_old {
            ctx.subjects.flag(id, subject_flags::UNSUPPORTED);
        }
    }
}

fn friends_in_patterns(ctx: &mut Ctx<'_, '_, '_>) {
    let unit = ctx.unit;
    let ast = unit.ast();
    let seeds = ctx.subjects.seeds();
    if seeds.is_empty() {
        return;
    }
    // Each seed's key and the entity of its scope.
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
            .filter(|(_, w, s)| *s == scope && w.kind == k.kind && w.disc == k.disc && w.name == k.name)
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
