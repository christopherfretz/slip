//! Virtual families (`spec/edits.md` §9.2, `docs/wp/WP-072.md`): every
//! virtual member function the AST names like the old name is a subject
//! of role `candidate` (a `seed` stays one), in every observer, whatever
//! class it is in, with a link to each function it overrides directly.
//! Reduce closes the seeds under the links (§5.2), so a rename of one
//! virtual function renames its whole family and no other; an unselected
//! candidate's claims become `keep` there.
//!
//! **Where overriding is read.** Kiln records, on a virtual member
//! function's declaration, the functions it overrides directly
//! ([class.virtual]/2; `kiln_ast/src/decl.rs:1686`, `DeclArena::cold.
//! overrides`, written by `Sema::finish_virtual_decl`,
//! `kiln_sema/src/class/virtual/mod.rs:255`, from `Sema::virtual_decl`
//! at `:130`, for every declared member function: one written in a class,
//! and one declared again when a class template is instantiated); a
//! redeclaration shares its first declaration's list. Nothing here
//! compares names or signatures to find an overrider.
//!
//! **Which functions are looked at.** Two sources, so that a family is
//! whole even where `written::decl_names` has no entry (a member of an
//! instantiated class, a name a macro pastes):
//!
//! - every declaration `decl_names` gives that the AST names like the old
//!   name: its origin's entity is a candidate when it is a virtual member
//!   function;
//! - every function of the overriding table named like the old name, and
//!   each function it overrides.
//!
//! A function is taken to its origin (`written::origin`: a member of an
//! instantiated class is the member of the template) and to the digest's
//! entity, as a reference to it is (§9.3 "The pieces").
//!
//! **Conservative rows that stay**, each a flag on the candidate (a flag
//! is what one observer sees, §4.1; its rule is `Keys::rename_rule`'s
//! whatever the flag):
//!
//! - a candidate declared in a template's pattern (a class template, a
//!   partial specialisation, or a local class of a function template:
//!   one of them in its parent chain) is `unsupported`: its followers in
//!   the specialisations are `docs/wp/WP-073.md`'s;
//! - a function of the family that has no key (`Keys::key_id` refuses)
//!   cannot be a subject, so the link to it cannot be recorded: the
//!   function at the other end is `unsupported`, so that the family is
//!   refused rather than split;
//! - a function the family reaches that the AST does not name like the
//!   old name (a destructor, an operator or a conversion function cannot
//!   arise: overriding keeps the name, and a destructor is not named by an
//!   identifier) makes the function that reaches it `unsupported`.
//!
//! A candidate declared in a file that is not owned is `read_only`: that
//! is `decls::claim`'s rule for any subject, and through its link it
//! refuses a family that reaches into a library (§9.5). A covariant
//! return or `final` changes nothing here.

use std::collections::BTreeSet;

use slip_kiln::kiln::ast::decl::DeclKind;
use slip_kiln::kiln::ast::names::DeclNameKind;
use slip_kiln::kiln::ast::{Ast, DeclId, ListId};
use slip_kiln::written;
use slip_model::edits::{KeyId, Role, subject_flags};
use slip_model::keys::KeyKind;

use super::{CTX_LIMIT, Ctx, decls, enclosing};

/// Collects the candidates and their links.
pub(crate) fn collect(ctx: &mut Ctx<'_, '_, '_>) {
    let unit = ctx.unit;
    let ast = unit.ast();
    // The declarations to look at, each once, in id order (the overriding
    // table is a hash map: its order is not observable, §9.7).
    let mut looked: BTreeSet<DeclId> = BTreeSet::new();
    for d in &ctx.named.old {
        if is_named_method(ast, ctx.old, d.decl) {
            looked.insert(d.decl);
        }
    }
    for (m, _) in ast.decls.cold.overrides.iter() {
        if is_named_method(ast, ctx.old, m) {
            looked.insert(m);
        }
    }
    for d in looked {
        let overridden: Vec<DeclId> = overridden(ast, d).collect();
        if overridden.is_empty() && !is_virtual(ast, d) {
            continue;
        }
        let Some(a) = candidate(ctx, d) else {
            // No key: nothing can be recorded for it, and each function
            // it overrides is refused in its place.
            for m in overridden {
                if let Some(b) = candidate(ctx, m) {
                    ctx.subjects.flag(b, subject_flags::UNSUPPORTED);
                }
            }
            continue;
        };
        for m in overridden {
            if !is_named_method(ast, ctx.old, m) {
                ctx.subjects.flag(a, subject_flags::UNSUPPORTED);
                continue;
            }
            match candidate(ctx, m) {
                Some(b) => ctx.subjects.link(a, b),
                None => ctx.subjects.flag(a, subject_flags::UNSUPPORTED),
            }
        }
    }
}

/// Adds the entity of `d`'s origin as a candidate (a seed stays a seed)
/// and flags it `unsupported` when it is declared in a template's
/// pattern. `None` when it has no key.
fn candidate(ctx: &mut Ctx<'_, '_, '_>, d: DeclId) -> Option<KeyId> {
    let o = written::origin(ctx.unit, d);
    let id = ctx.subjects.add(ctx.keys, o, Role::Candidate, ctx.new, &[])?;
    if in_pattern(ctx, o) {
        ctx.subjects.flag(id, subject_flags::UNSUPPORTED);
    }
    Some(id)
}

/// A member function (not a constructor, destructor or conversion
/// function) whose name is the identifier `old`.
fn is_named_method(ast: &Ast, old: &[u8], d: DeclId) -> bool {
    let n = ast.decls.get(d);
    n.kind() == DeclKind::CXXMethod
        && n.name().kind() == DeclNameKind::Identifier
        && decls::name_of(ast, d) == Some(old)
}

/// Whether the function `d` is virtual (declared so, or overriding one),
/// as its first declaration records it.
fn is_virtual(ast: &Ast, d: DeclId) -> bool {
    ast.decls.function(ast.first(d)).is_some_and(|f| f.spec.is_virtual())
}

/// The functions `d` overrides directly (their first declarations), as
/// Kiln records them on its first declaration.
fn overridden(ast: &Ast, d: DeclId) -> impl Iterator<Item = DeclId> + '_ {
    let list = ast.decls.cold.overrides.get(ast.first(d)).copied().unwrap_or(ListId::EMPTY);
    ast.lists.iter::<DeclId>(list)
}

/// Whether an entity that encloses `d` is a template's pattern: its key
/// is a class template, a partial specialisation or a function template
/// (the last for a local class of a function template's pattern).
pub(crate) fn in_pattern(ctx: &mut Ctx<'_, '_, '_>, d: DeclId) -> bool {
    let ast = ctx.unit.ast();
    let mut at = d;
    for _ in 0..CTX_LIMIT {
        let Some(s) = enclosing(ast, at) else { return false };
        let s = ctx.keys.entity(s);
        if ctx.keys.key(s).is_ok_and(|k| {
            matches!(
                k.kind,
                KeyKind::ClassTemplate | KeyKind::PartialSpecialisation | KeyKind::FunctionTemplate
            )
        }) {
            return true;
        }
        at = s;
    }
    false
}
