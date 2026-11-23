//! Conservative until `docs/wp/WP-073.md` replaces it: a dependent name
//! (§9.4) spelled like the old name is claimed `unresolved`.
//!
//! A dependent entry has no referent and no found set at this pin (K4,
//! `spec/landed/edits/WP-067.md`), so nothing tells its identifier but
//! the token itself: `Sites::name_token` locates it only when its bytes
//! are the old name's spelling, and a name it does not locate is not
//! claimed. This is the one place the rename asks `sites` about a name
//! the AST did not first name like the old name (the landed file,
//! "Cost").

use slip_kiln::written::DependentName;

use super::Ctx;

/// Claims `name` `unresolved` if it is spelled like the old name.
pub(crate) fn claim(ctx: &mut Ctx<'_, '_, '_>, name: &DependentName) {
    ctx.claims.unresolved(&ctx.sites, name.name);
}
