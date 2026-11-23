//! Conservative until `docs/wp/WP-073.md` replaces it: a seed that is a
//! member of a class template (a class template anywhere in its key's
//! parent chain) is `unsupported`. Its followers in the specialisations
//! (§9.2) are not collected.

use slip_model::edits::subject_flags;
use slip_model::keys::KeyKind;

use super::{CTX_LIMIT, Ctx, enclosing};

/// Flags every seed enclosed by a class template.
pub(crate) fn collect(ctx: &mut Ctx<'_, '_, '_>) {
    let ast = ctx.unit.ast();
    for (id, e) in ctx.subjects.seeds() {
        let mut at = e;
        for _ in 0..CTX_LIMIT {
            let Some(s) = enclosing(ast, at) else { break };
            let s = ctx.keys.entity(s);
            if ctx.keys.key(s).is_ok_and(|k| k.kind == KeyKind::ClassTemplate) {
                ctx.subjects.flag(id, subject_flags::UNSUPPORTED);
                break;
            }
            at = s;
        }
    }
}
