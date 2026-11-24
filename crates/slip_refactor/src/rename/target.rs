//! `spec/edits.md` §9.1: the seeds. The match is stated over keys and is
//! read literally (`docs/wp/WP-071.md`, "Matching the target"): the AST
//! only proposes candidates, by naming them like the old name, and gives
//! the enclosing entity at each step of the parent chain, whose key id
//! must be the one the key names as its parent.

use std::collections::HashMap;

use slip_kiln::kiln::ast::DeclId;
use slip_kiln::kiln::ast::decl::DeclKind;
use slip_kiln::written::{self, WrittenRef};
use slip_model::edits::{Role, subject_flags};
use slip_model::keys::{Key, KeyKind, Name, Parent};
use slip_model::transform::TargetKind;

use super::{Ctx, decls, enclosing};

/// What an entity is to the target.
#[derive(Clone, Copy, Debug)]
enum Class {
    /// A seed, with these flags.
    Seed(u64),
    /// Named like the target, with C language linkage: a seed with
    /// `unsupported` when a declaration of it is in the target's scope.
    CLinkage,
    /// Not a seed.
    No,
}

/// One pass over `decl_names` (filling `ctx.named`) and one over
/// `written_refs`; the seeds are added to `ctx.subjects`. Returns the
/// written references whose referent's origin the AST names like the old
/// name, in unit order, and the number of entries looked at.
pub(crate) fn seeds(ctx: &mut Ctx<'_, '_, '_>) -> (Vec<WrittenRef>, u64) {
    let unit = ctx.unit;
    let ast = unit.ast();
    for d in written::decl_names(unit) {
        match decls::name_of(ast, d.decl) {
            Some(n) if n == ctx.old => ctx.named.old.push(d),
            Some(n) if n == ctx.new => ctx.named.new.push(d),
            _ => {}
        }
    }
    let mut memo: HashMap<DeclId, Class> = HashMap::new();
    let names = std::mem::take(&mut ctx.named.old);
    for d in &names {
        consider(ctx, &mut memo, written::origin(unit, d.decl), d.decl);
    }
    ctx.named.old = names;

    let mut hits = Vec::new();
    let mut looked = 0u64;
    for r in written::written_refs(unit) {
        looked += 1;
        if !rename_role(r.role) {
            continue;
        }
        let o = written::origin(unit, r.referent);
        if decls::name_of(ast, o) == Some(ctx.old) {
            consider(ctx, &mut memo, o, r.referent);
            hits.push(r);
        }
    }
    (hits, looked)
}

/// The roles the rename's rows read (§9.3): those Kiln recorded at
/// `913a490bb`, before its written-reference table stage 3 (WP-1932, in
/// the pin since WP-066): stages 1 and 2, and `Concept`, which Kiln
/// already recorded then. The names stage 3 added (a using-declarator, a
/// using-directive's or namespace alias's namespace, an elaborated or
/// friend reference, a specialisation target, a designator, a
/// redeclaration's qualified name) are reached as before: through
/// `decl_names`, the followers, or `unreachable`. This is safe only while
/// §9.1 allows Function and Variable targets; serving those names from
/// the table is WP-138's (`spec/landed/digest-kinds/WP-066.md`, "For
/// WP-138").
fn rename_role(role: written::Role) -> bool {
    use written::Role as R;
    match role {
        R::Expr
        | R::Member
        | R::Type
        | R::Template
        | R::Nns
        | R::Base
        | R::MemInit
        | R::Concept => true,
        R::UsingDecl
        | R::UsingDirective
        | R::NamespaceAlias
        | R::Elaborated
        | R::SpecTarget
        | R::Designator
        | R::Redecl => false,
    }
}

/// Adds the entity of `origin` as a seed if it is one; `decl` is the
/// declaration or referent it was reached by (for the C linkage case,
/// whose scopes are each declaration's).
fn consider(
    ctx: &mut Ctx<'_, '_, '_>,
    memo: &mut HashMap<DeclId, Class>,
    origin: DeclId,
    decl: DeclId,
) {
    let e = ctx.keys.entity(origin);
    let class = if let Some(&c) = memo.get(&e) {
        c
    } else {
        let c = classify(ctx, e);
        memo.insert(e, c);
        c
    };
    let flags = match class {
        Class::Seed(flags) => flags,
        Class::CLinkage if scopes_are_target(ctx, decl) || scopes_are_target(ctx, origin) => {
            subject_flags::UNSUPPORTED
        }
        Class::CLinkage | Class::No => return,
    };
    if let Some(id) = ctx.subjects.add(ctx.keys, e, Role::Seed, ctx.new, &[]) {
        ctx.subjects.flag(id, flags);
    }
}

/// §9.1 for entity `e`: its key is named `identifier(old)`; its parent
/// chain is the target's scope ([`chain_matches`]); its kind is allowed
/// (a seed), or another (a seed with `unsupported`). A key with parent
/// `c_linkage` is decided by the scopes of each declaration; `builtin`
/// and a key `Keys::key` refuses are not seeds. `::main` is
/// `unsupported` (§9.5).
fn classify(ctx: &mut Ctx<'_, '_, '_>, e: DeclId) -> Class {
    let Ok(key) = ctx.keys.key(e) else { return Class::No };
    if !matches!(&key.name, Name::Identifier(n) if n.as_slice() == ctx.old) {
        return Class::No;
    }
    // A using-declaration in the target's scope is no seed (of another
    // kind): it names the name, and is the follower §9.2 makes of what it
    // introduces (`docs/wp/WP-120.md` item 3).
    if key.kind == KeyKind::UsingDeclaration {
        return Class::No;
    }
    match key.parent {
        Parent::CLinkage => return Class::CLinkage,
        Parent::Builtin => return Class::No,
        Parent::Root | Parent::Unit(_) => {}
    }
    if !chain_matches(ctx, e, &key) {
        return Class::No;
    }
    let allowed = match ctx.request.kind {
        TargetKind::Function => matches!(key.kind, KeyKind::Function | KeyKind::FunctionTemplate),
        TargetKind::Variable => matches!(key.kind, KeyKind::Variable | KeyKind::VariableTemplate),
        TargetKind::Any => matches!(
            key.kind,
            KeyKind::Function
                | KeyKind::FunctionTemplate
                | KeyKind::Variable
                | KeyKind::VariableTemplate
        ),
    };
    let mut flags = if allowed { 0 } else { subject_flags::UNSUPPORTED };
    if key.parent == Parent::Root && key.kind == KeyKind::Function && ctx.old == b"main" {
        flags |= subject_flags::UNSUPPORTED;
    }
    Class::Seed(flags)
}

/// The target's scope, innermost component first, against `key`'s parent
/// chain: at each step the parent is `unit(id)`, the AST's enclosing
/// entity has key id `id`, and its key is a namespace, class or class
/// template named like the component; after the outermost component the
/// parent is `root`.
fn chain_matches(ctx: &mut Ctx<'_, '_, '_>, e: DeclId, key: &Key) -> bool {
    let ast = ctx.unit.ast();
    let scope = &ctx.request.target[..ctx.request.target.len() - 1];
    let mut parent = key.parent;
    let mut at = e;
    for component in scope.iter().rev() {
        let Parent::Unit(want) = parent else { return false };
        let Some(s) = enclosing(ast, at) else { return false };
        let s = ctx.keys.entity(s);
        if ctx.keys.key_id(s) != Ok(want) {
            return false;
        }
        let Ok(k) = ctx.keys.key(s) else { return false };
        if !is_component(&k, component) {
            return false;
        }
        parent = k.parent;
        at = s;
    }
    parent == Parent::Root
}

/// The last sentence of §9.1, for a key with parent `c_linkage`: the
/// scopes that enclose declaration `d` (their keys, from the innermost)
/// are the target's, and the outermost is in the global namespace.
fn scopes_are_target(ctx: &mut Ctx<'_, '_, '_>, d: DeclId) -> bool {
    let ast = ctx.unit.ast();
    let scope = &ctx.request.target[..ctx.request.target.len() - 1];
    let mut at = d;
    for component in scope.iter().rev() {
        let Some(s) = enclosing(ast, at) else { return false };
        let s = ctx.keys.entity(s);
        let Ok(k) = ctx.keys.key(s) else { return false };
        if !is_component(&k, component) {
            return false;
        }
        at = s;
    }
    enclosing(ast, at).is_some_and(|s| ast.decls.get(s).kind() == DeclKind::TranslationUnit)
}

/// A scope component of the target: a namespace, class or class template
/// named `component`.
fn is_component(k: &Key, component: &[u8]) -> bool {
    matches!(&k.name, Name::Identifier(n) if n.as_slice() == component)
        && matches!(k.kind, KeyKind::Namespace | KeyKind::Class | KeyKind::ClassTemplate)
}
