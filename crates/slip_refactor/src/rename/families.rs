//! Conservative until `docs/wp/WP-072.md` replaces it: a seed that is a
//! virtual member function (declared `virtual`, or overriding one) is
//! `unsupported`. Its family (§9.2, candidates and links) is not
//! collected.

use slip_model::edits::subject_flags;

use super::Ctx;

/// Flags every virtual seed.
pub(crate) fn collect(ctx: &mut Ctx<'_, '_, '_>) {
    let ast = ctx.unit.ast();
    for (id, e) in ctx.subjects.seeds() {
        if ast.decls.function(e).is_some_and(|f| f.spec.is_virtual()) {
            ctx.subjects.flag(id, subject_flags::UNSUPPORTED);
        }
    }
}
