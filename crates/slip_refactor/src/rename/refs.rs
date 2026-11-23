//! The written references (§9.3, the rows "an id-expression" to "a
//! non-dependent name in a template pattern"): every entry of
//! `written::written_refs` whose referent's origin the AST names like the
//! old name. A reference to a subject is a `replace` with a spelled
//! piece, `K` the key id of the referent's origin (`written::origin`,
//! `Keys::entity`, `Keys::key_id`: the subject's id); a reference to
//! anything else is a `keep` (§4.2). `via_using` is `followers`'
//! business: here the name denotes the referent.

use slip_kiln::written::WrittenRef;

use super::{Ctx, Written};

/// Claims `hits`, the references `target::seeds` kept.
pub(crate) fn claim(ctx: &mut Ctx<'_, '_, '_>, hits: &[WrittenRef]) {
    for r in hits {
        let o = super::origin_of(ctx, r.referent);
        match ctx.subjects.of_decl(ctx.keys, o) {
            Some(id) => ctx.claims.replace(&ctx.sites, r.name, &[id], Written::Spelled(id)),
            None => ctx.claims.keep(&ctx.sites, r.name),
        }
    }
}
