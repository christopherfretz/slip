//! Dependent names (§9.4): every one spelled like the old name is claimed
//! `unresolved` at this pin. `docs/wp/WP-073.md` parts A and D wait for
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
//! The tests of both rows are written with the answers they must give
//! then, marked `# pending` (`tests/rename/s9_4-*`); the row 2 case that
//! is not instantiated is `unresolved` now and then
//! (`s9_4-dependent-member-not-instantiated`).
//!
//! A dependent entry has no referent and no found set (K4,
//! `spec/landed/edits/WP-067.md`), so nothing tells its identifier but
//! the token itself: `Sites::name_token` locates it only when its bytes
//! are the old name's spelling, and a name it does not locate is not
//! claimed. This is the one place the rename asks `sites` about a name
//! the AST did not first name like the old name
//! (`spec/landed/edits/WP-071.md`, "Cost").

use slip_kiln::written::DependentName;

use super::Ctx;

/// Claims `name` `unresolved` if it is spelled like the old name.
pub(crate) fn claim(ctx: &mut Ctx<'_, '_, '_>, name: &DependentName) {
    ctx.claims.unresolved(&ctx.sites, name.name);
}
