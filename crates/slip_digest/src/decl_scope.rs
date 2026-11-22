//! The payload records of scopes and of the declarations that declare no
//! name, and the cold maps of declarations (`spec/digest-kinds.md` §5.6
//! to §5.8): pending.
//!
//! Each function writes one `record T` of §5.6 for the declaration `d`,
//! read as §2.3 says; `cold` writes the rows of `ColdDeclData` (§5.7).
//! Until the package of this file lands, `static_assert_data` writes the
//! pending placeholder and then the expressions the record owns (the
//! "reach stub": it is no table's encoding and is replaced whole).

use slip_kiln::kiln::ast::DeclId;

use crate::Area;
use crate::stream::Stream;
use crate::walk::Walk;

/// enum `FriendTarget`.
pub(crate) fn friend_target(_w: &mut Walk<'_>, s: &mut Stream, _d: DeclId) {
    s.pending(Area::DeclData);
}

/// `NamespaceData`.
pub(crate) fn namespace_data(_w: &mut Walk<'_>, s: &mut Stream, _d: DeclId) {
    s.pending(Area::DeclData);
}

/// `StaticAssertData`.
pub(crate) fn static_assert_data(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    s.pending(Area::DeclData);
    let (cond, message) =
        w.ast.decls.static_assert(d).map_or((None, None), |a| (Some(a.cond), a.message));
    crate::expr::child_opt(w, s, cond);
    crate::expr::child_opt(w, s, message);
}

/// `FileScopeAsm.data`: the string literal, a `child`.
pub(crate) fn file_scope_asm(_w: &mut Walk<'_>, s: &mut Stream, _d: DeclId) {
    s.pending(Area::DeclData);
}

/// the rows of `ColdDeclData`.
pub(crate) fn cold(_w: &mut Walk<'_>, s: &mut Stream, _d: DeclId) {
    s.pending(Area::DeclData);
}
