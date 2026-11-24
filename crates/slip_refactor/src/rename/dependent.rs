//! Dependent names (§9.4). `docs/wp/WP-073.md` parts A and D wait for
//! Kiln, and neither follow-up is in the pin (`spec/landed/edits/WP-073.md`,
//! "Kiln at the pin"):
//!
//! - **Part A, row 1** (the unqualified callee of a dependent call, and the
//!   declarations found at the definition) needs K-a, the join from a
//!   dependent entry to its unresolved-lookup expression
//!   (`docs/requests/KR1-amendment-1.md` §2). Kiln keeps that link in a
//!   private map that it empties as overload sets are resolved
//!   (`kiln_ast/src/written_refs.rs:179`, `:271`), and its specification
//!   promises no location equality between an entry and its expression.
//! - **Part D, row 2** (`T::f`, `x.f`, `x->f`, per instantiated
//!   specialisation) needs K-e, Kiln's stage 4 (WP-1933): "Instantiation
//!   records nothing" (Kiln `spec/ast.md` §18.5).
//!
//! **The refusing form** (`docs/wp/WP-127.md` item 2): what an
//! instantiation the observer made binds a dependent name to is read from
//! the expressions Kiln built for it, without stage 4. Kiln instantiates a
//! pattern's expression at the pattern's place: the `MemberExpr` of `t.f()`
//! in `call<n::S<int>>`, the `DeclRefExpr` of `T::f()` and of `&T::f`, has
//! as its location the name token of the dependent entry in the pattern
//! (the location the probe of `spec/landed/edits/WP-127.md` shows; Kiln
//! `spec/oven.md` says so of an instantiation's constructs). So one pass
//! over the expression arena ([`bound`]) gives, for each dependent name of
//! the unit, every declaration an expression at its name token refers to.
//! A dependent name one of those binds to a subject (taken to its origin
//! and entity, as a written reference is, §9.3) is claimed `refuse` with
//! reason `mixed_instances` and those subjects: the pattern's one name is
//! that of every instantiation, those of other observers and of no
//! observer included, and this rename does not edit it. Every other
//! dependent name spelled like the old name is `unresolved`, as before.
//!
//! The reading over-approximates where it could err: an expression Kiln
//! built and dropped (an overload candidate) refuses more, never less. A
//! name that an instantiation binds and whose expression does not carry
//! the pattern's location would be missed: the run is then accepted and
//! verification reports the instantiation (loud, as before this package).
//!
//! The tests of both rows are written with the answers they must give
//! then, marked `# pending` (`tests/rename/s9_4-*`), except the row 2 case
//! instantiated with a subject and with another, which the refusing form
//! already gives (`s9_4-dependent-member-mixed-instances`); the row 2 case
//! that is not instantiated is `unresolved` now and then
//! (`s9_4-dependent-member-not-instantiated`).
//!
//! A dependent entry has no referent and no found set (K4,
//! `spec/landed/edits/WP-067.md`), so nothing tells its identifier but
//! the token itself: `Sites::name_token` locates it only when its bytes
//! are the old name's spelling, and a name it does not locate is not
//! claimed. This is the one place the rename asks `sites` about a name
//! the AST did not first name like the old name
//! (`spec/landed/edits/WP-071.md`, "Cost").
//!
//! **Cost.** Nothing when the unit has no dependent name; else one pass
//! over the expression arena, a hash lookup per `DeclRefExpr` and
//! `MemberExpr`, and the name of each one that is at a dependent name's
//! place compared with the old name.

use std::collections::{BTreeSet, HashMap, HashSet};

use slip_kiln::kiln::ast::expr::ExprKind;
use slip_kiln::kiln::ast::{DeclId, ExprId};
use slip_kiln::written::DependentName;
use slip_model::edits::{KeyId, Reason};

use super::{Ctx, decls};

/// For each dependent name's place (its first token's location), the
/// declarations named like the old name that an expression there refers
/// to, each once, in arena order.
pub(crate) fn bound(ctx: &Ctx<'_, '_, '_>, names: &[DependentName]) -> HashMap<u32, Vec<DeclId>> {
    let mut out: HashMap<u32, Vec<DeclId>> = HashMap::new();
    if names.is_empty() {
        return out;
    }
    let places: HashSet<u32> = names.iter().map(|n| n.name.lo.to_raw()).collect();
    let ast = ctx.unit.ast();
    for i in 1..=ast.exprs.len() {
        let Ok(i) = u32::try_from(i) else { break };
        let n = ast.exprs.get(ExprId::new(i));
        let d = match n.kind() {
            ExprKind::DeclRefExpr => DeclId::try_new(n.a()),
            ExprKind::MemberExpr => DeclId::try_new(n.b()),
            _ => None,
        };
        let Some(d) = d else { continue };
        let at = n.loc().to_raw();
        if !places.contains(&at) {
            continue;
        }
        let d = ast.strip_shadow(d);
        if decls::name_of(ast, d) != Some(ctx.old) {
            continue;
        }
        let v = out.entry(at).or_default();
        if !v.contains(&d) {
            v.push(d);
        }
    }
    out
}

/// Claims `name`: `refuse(mixed_instances)` with the subjects an
/// instantiation binds it to (`bound`), else `unresolved` if it is spelled
/// like the old name.
pub(crate) fn claim(
    ctx: &mut Ctx<'_, '_, '_>,
    name: &DependentName,
    bound: &HashMap<u32, Vec<DeclId>>,
) {
    let mut subjects: BTreeSet<KeyId> = BTreeSet::new();
    for &d in bound.get(&name.name.lo.to_raw()).into_iter().flatten() {
        let o = super::origin_of(ctx, d);
        if let Some(id) = ctx.subjects.of_decl(ctx.keys, o) {
            subjects.insert(id);
        }
    }
    if subjects.is_empty() {
        ctx.claims.unresolved(&ctx.sites, name.name);
    } else {
        let subjects: Vec<KeyId> = subjects.into_iter().collect();
        ctx.claims.refuse(&ctx.sites, name.name, &subjects, Reason::MixedInstances);
    }
}
