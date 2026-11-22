//! The payload records and cold maps of declarations
//! (`spec/digest-kinds.md` §5.4 to §5.8): pending (WP-024).
//!
//! Each function writes one `record T` of §5.4 to §5.6 for the
//! declaration `d`, read as §2.3 says; `cold` writes the rows of
//! `ColdDeclData` (§5.7).

use slip_kiln::kiln::ast::DeclId;

use crate::Area;
use crate::stream::Stream;
use crate::walk::Walk;

/// `TypedefData`.
pub(crate) fn typedef_data(_w: &mut Walk<'_>, s: &mut Stream, _d: DeclId) {
    s.pending(Area::DeclData);
}

/// `EnumData`.
pub(crate) fn enum_data(_w: &mut Walk<'_>, s: &mut Stream, _d: DeclId) {
    s.pending(Area::DeclData);
}

/// `EnumeratorData`.
pub(crate) fn enumerator_data(_w: &mut Walk<'_>, s: &mut Stream, _d: DeclId) {
    s.pending(Area::DeclData);
}

/// `RecordData`.
pub(crate) fn record_data(_w: &mut Walk<'_>, s: &mut Stream, _d: DeclId) {
    s.pending(Area::DeclData);
}

/// `FieldData`.
pub(crate) fn field_data(_w: &mut Walk<'_>, s: &mut Stream, _d: DeclId) {
    s.pending(Area::DeclData);
}

/// enum `FriendTarget`.
pub(crate) fn friend_target(_w: &mut Walk<'_>, s: &mut Stream, _d: DeclId) {
    s.pending(Area::DeclData);
}

/// `FunctionData`.
pub(crate) fn function_data(_w: &mut Walk<'_>, s: &mut Stream, _d: DeclId) {
    s.pending(Area::DeclData);
}

/// `VarData`.
pub(crate) fn var_data(_w: &mut Walk<'_>, s: &mut Stream, _d: DeclId) {
    s.pending(Area::DeclData);
}

/// `ParamData`.
pub(crate) fn param_data(_w: &mut Walk<'_>, s: &mut Stream, _d: DeclId) {
    s.pending(Area::DeclData);
}

/// `NamespaceData`.
pub(crate) fn namespace_data(_w: &mut Walk<'_>, s: &mut Stream, _d: DeclId) {
    s.pending(Area::DeclData);
}

/// `StaticAssertData`.
pub(crate) fn static_assert_data(_w: &mut Walk<'_>, s: &mut Stream, _d: DeclId) {
    s.pending(Area::DeclData);
}

/// `LetData`.
pub(crate) fn let_data(_w: &mut Walk<'_>, s: &mut Stream, _d: DeclId) {
    s.pending(Area::DeclData);
}

/// `FileScopeAsm.data`: the string literal, a `child`.
pub(crate) fn file_scope_asm(_w: &mut Walk<'_>, s: &mut Stream, _d: DeclId) {
    s.pending(Area::DeclData);
}

/// the rows of `ColdDeclData`.
pub(crate) fn cold(_w: &mut Walk<'_>, s: &mut Stream, _d: DeclId) {
    s.pending(Area::DeclData);
}
