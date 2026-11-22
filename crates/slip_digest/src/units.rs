//! Which entities are units, and writing one (`spec/digest.md` §2, §3.4,
//! §11; `spec/digest-kinds.md` §5.2).

use slip_kiln::kiln::ast::decl::{DeclFlags, DeclKind, TemplLinkKind, VarStorage};
use slip_kiln::kiln::ast::ids::DeclId;
use slip_kiln::kiln::ast::templates::SpecKind;
use slip_model::Digest;
use slip_model::digests::GroupClass;
use slip_model::keys::{Key, KeyKind, Name, Parent};

use crate::Cause;
use crate::keys::{KeyRes, Role, info};
use crate::stream::{Stream, UNIT_TAG};
use crate::walk::Walk;

/// How many declarations of one entity are read; a longer chain is
/// `Unaccounted("redecls/limit")`.
pub(crate) const REDECL_LIMIT: usize = 1 << 20;

/// How an entity is a unit (`spec/digest.md` §2.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UnitClass {
    /// Written in source: always a unit.
    Declared,
    /// An implicit instantiation, or an implicit member with a body:
    /// always a unit, derived.
    Derived,
    /// An implicit member, a builtin, a template parameter object: a
    /// derived unit only if some unit's stream holds a `Ref` to it.
    OnDemand,
}

/// A unit as written, before grouping.
#[derive(Clone, Debug)]
pub(crate) struct Written {
    /// The entity (its first declaration); `None` for the order unit.
    pub entity: Option<DeclId>,
    pub key: Digest,
    pub kind: KeyKind,
    pub digest: Digest,
    pub cause: Option<Cause>,
    pub class: GroupClass,
    pub file: Vec<u8>,
}

/// Every declaration that is the first of a unit, in arena order
/// (`spec/digest-kinds.md` §5.2); `OnDemand` ones wait for a `Ref`.
pub(crate) fn collect(w: &mut Walk<'_>) {
    let ast = w.ast;
    // Entities already classified: each is classified once, not once per
    // declaration.
    let mut seen: std::collections::HashSet<DeclId> = std::collections::HashSet::new();
    for d in ast.decls.ids() {
        if let Role::Unsupported(what) = info(ast.decls.get(d).kind()).role {
            // No key kind at all: the nearest enclosing unit is made
            // indeterminate instead (`spec/landed/digest/WP-024.md`).
            let scope = enclosing_entity(w, d);
            w.taints.push((scope, Cause::Unsupported(what)));
            continue;
        }
        let e = w.entity(d);
        if !seen.insert(e) {
            continue;
        }
        match unit_class(w, e) {
            Some(class @ (UnitClass::Declared | UnitClass::Derived)) => w.enqueue(e, class),
            Some(UnitClass::OnDemand) | None => {}
        }
    }
}

/// Queues `d`'s entity as a derived unit if it becomes one by being
/// referred to (`spec/digest.md` §2.2).
pub(crate) fn demand(w: &mut Walk<'_>, e: DeclId) {
    if w.queued.contains(&e) {
        return;
    }
    if let Some(class) = unit_class(w, e) {
        w.enqueue(
            e,
            match class {
                UnitClass::Declared => UnitClass::Declared,
                UnitClass::Derived | UnitClass::OnDemand => UnitClass::Derived,
            },
        );
    }
}

/// Whether, and how, the entity `e` (a first declaration) is a unit.
pub(crate) fn unit_class(w: &Walk<'_>, e: DeclId) -> Option<UnitClass> {
    let ast = w.ast;
    let n = ast.decls.get(e);
    match info(n.kind()).role {
        Role::Entity(_) | Role::Content(_) => {}
        Role::Nothing
        | Role::Transparent
        | Role::Root
        | Role::Local
        | Role::InParent
        | Role::Unsupported(_) => return None,
    }
    if w.is_local(e) && !w.is_local_extern(e) {
        return None;
    }
    let link = ast.templ_link(e).get();
    if let Some((TemplLinkKind::Pattern, _)) = link {
        return None;
    }
    let kind = n.kind();
    // A using-shadow is declared by its using-declaration, and an indirect
    // field by the anonymous member that holds it, which are written.
    let written_by_another = kind == DeclKind::UsingShadow
        || kind == DeclKind::ConstructorUsingShadow
        || kind == DeclKind::IndirectField;
    if !written_by_another && all_implicit(w, e) {
        if info(kind).function || kind == DeclKind::CXXDeductionGuide {
            let has_body = ast
                .definition(e)
                .and_then(|def| ast.decls.function(def))
                .is_some_and(|f| f.body.is_some());
            return Some(if has_body { UnitClass::Derived } else { UnitClass::OnDemand });
        }
        // Another implicit declaration: a part of a builtin
        // (`__va_list_tag`), a unit only when referred to.
        return Some(UnitClass::OnDemand);
    }
    if in_builtin(w, e) {
        return Some(UnitClass::OnDemand);
    }
    if info(n.kind()).on_demand {
        return Some(UnitClass::OnDemand);
    }
    match link {
        Some((TemplLinkKind::Specialization, _)) => {
            if let Some((_, info)) = ast.spec_info(e) {
                return spec_class(info.kind);
            }
        }
        Some((TemplLinkKind::MemberOfSpecialization, _)) => {
            if let Some((_, info)) = ast.member_spec_info(e) {
                return spec_class(info.kind);
            }
        }
        Some((
            TemplLinkKind::Pattern
            | TemplLinkKind::PartialSpecialization
            | TemplLinkKind::DependentFriend,
            _,
        ))
        | None => {}
    }
    // Every member of an implicit instantiation is derived (digest §2.2),
    // whether or not Kiln gave it a `MemberSpecInfo`.
    if in_implicit_instantiation(w, e) {
        return Some(UnitClass::Derived);
    }
    Some(UnitClass::Declared)
}

/// An enclosing context of `e` is an implicitly instantiated
/// specialisation, or a member of one, with no explicitly specialised
/// context in between (whose members are written in source).
pub(crate) fn in_implicit_instantiation(w: &Walk<'_>, e: DeclId) -> bool {
    let ast = w.ast;
    let mut c = ast.decls.get(e).ctx();
    for _ in 0..64 {
        let Some(x) = c else { return false };
        let kind = if let Some((_, info)) = ast.spec_info(x) {
            Some(info.kind)
        } else {
            ast.member_spec_info(x).map(|(_, info)| info.kind)
        };
        match kind {
            Some(SpecKind::ImplicitInstantiation) => return true,
            Some(
                SpecKind::ExplicitSpecialization
                | SpecKind::ExplicitInstantiationDeclaration
                | SpecKind::ExplicitInstantiationDefinition
                | SpecKind::Undeclared,
            ) => return false,
            None => {}
        }
        c = ast.decls.get(x).ctx();
    }
    false
}

/// The nearest entity around `d` that may be a unit (its first
/// declaration), `None` at the top level.
fn enclosing_entity(w: &Walk<'_>, d: DeclId) -> Option<DeclId> {
    let ast = w.ast;
    let mut c = ast.decls.get(d).ctx();
    for _ in 0..4096 {
        let x = c?;
        match info(ast.decls.get(x).kind()).role {
            Role::Entity(_) | Role::Content(_) => return Some(w.entity(x)),
            Role::Root => return None,
            Role::Nothing
            | Role::Transparent
            | Role::Local
            | Role::InParent
            | Role::Unsupported(_) => c = ast.decls.get(x).ctx(),
        }
    }
    None
}

/// `spec/digest-kinds.md` §9: which specialisations are units.
fn spec_class(kind: SpecKind) -> Option<UnitClass> {
    match kind {
        SpecKind::Undeclared => None,
        SpecKind::ImplicitInstantiation => Some(UnitClass::Derived),
        SpecKind::ExplicitSpecialization
        | SpecKind::ExplicitInstantiationDeclaration
        | SpecKind::ExplicitInstantiationDefinition => Some(UnitClass::Declared),
    }
}

/// `e` is a builtin or a member of one, or a member of an implicit class
/// (`__va_list_tag`'s fields): a unit
/// only when referred to (`spec/digest.md` §2.2).
#[allow(
    clippy::many_single_char_names,
    reason = "the names of digest-kinds: s stream, d declaration, e entity, n node, q type"
)]
fn in_builtin(w: &Walk<'_>, e: DeclId) -> bool {
    let ast = w.ast;
    let mut d = Some(e);
    for _ in 0..64 {
        let Some(x) = d else { return false };
        let n = ast.decls.get(x);
        if ast.decls.builtin_id(x).is_some()
            || (x != e && n.has(DeclFlags::IMPLICIT) && ast.decls.record(x).is_some())
        {
            return true;
        }
        d = n.ctx();
    }
    false
}

/// Every declaration of the entity is implicit.
fn all_implicit(w: &Walk<'_>, e: DeclId) -> bool {
    w.redecls(e).iter().all(|&d| w.ast.decls.get(d).has(DeclFlags::IMPLICIT))
}

/// The declaration a unit's stream is read from (`spec/digest-kinds.md`
/// §2.3): the definition if there is one, else the first declaration. A
/// template is read from the template whose pattern is the definition.
pub(crate) fn read_decl(w: &Walk<'_>, e: DeclId) -> DeclId {
    let ast = w.ast;
    if let Some(pattern) = ast.template_info(e).and_then(|i| i.pattern) {
        return defining_decl(w, pattern).and_then(|def| ast.described_template(def)).unwrap_or(e);
    }
    defining_decl(w, e).unwrap_or(e)
}

/// The defining declaration of entity `e` (§2.3), if it has one. Kiln
/// records `definition` for classes, enumerations and functions only; for
/// a variable it is the first declaration of the chain that is a
/// definition ([basic.def]/2), the search Kiln's own reading of a
/// variable's initialiser makes over the chain (`kiln_oven/src/value.rs`).
pub(crate) fn defining_decl(w: &Walk<'_>, e: DeclId) -> Option<DeclId> {
    let ast = w.ast;
    if ast.decls.var(e).is_none() {
        return ast.definition(e);
    }
    let mut chain = w.redecls(e);
    chain.reverse();
    chain.into_iter().find(|&d| is_var_definition(w, d))
}

/// Whether the variable declaration `d` is a definition ([basic.def]/2),
/// as Kiln's `Sema::is_var_definition` decides it
/// (`kiln_sema/src/decl.rs:7582`): not a non-inline static data member
/// declared in its class, not a block-scope `extern`, not an explicit
/// specialisation of a static data member without an initialiser
/// ([temp.expl.spec]/14, `Ast::is_member_spec_declaration`), and either
/// initialised or not `extern` (a braceless linkage specification makes
/// it `extern`, [dcl.link]/8), a declaration with its own `selectany`
/// being a definition all the same.
pub(crate) fn is_var_definition(w: &Walk<'_>, d: DeclId) -> bool {
    let ast = w.ast;
    let Some(v) = ast.decls.var(d) else { return false };
    let in_class = ast.decls.lexical_ctx(d).is_some_and(|c| ast.decls.record(c).is_some());
    if in_class {
        return v.spec.is_inline();
    }
    if w.is_local_extern(d) || ast.is_member_spec_declaration(d) {
        return false;
    }
    let is_extern = (ast.decls.is_single_line_linkage(d) || v.spec.storage() == VarStorage::Extern)
        && !ast.has_own_selectany(d);
    v.init.is_some() || !is_extern
}

/// Writes the unit of entity `e`; `None` only for a role without a key
/// kind, which `collect` never queues. A unit whose key cannot be formed is
/// written under a fallback key, indeterminate with the key's cause
/// (`spec/landed/digest/WP-024.md`): nothing §2.1 makes a unit vanishes.
pub(crate) fn write(w: &mut Walk<'_>, e: DeclId, class: UnitClass) -> Option<Written> {
    let content = matches!(info(w.ast.decls.get(e).kind()).role, Role::Content(_));
    let hashed = if content {
        w.content_hash(e)
    } else {
        let read = read_decl(w, e);
        let saved = w.take_unit();
        let mut s = Stream::new();
        w.decl_stream(&mut s, read, e);
        w.unit = saved;
        let h = s.finish(UNIT_TAG);
        w.hashed(h)
    };
    let (key, kind, key_cause) = match w.key(e) {
        KeyRes::Key { id, kind, cause } => (id, kind, cause),
        KeyRes::None(cause) => {
            let cause = cause.unwrap_or(Cause::Unaccounted("key/none"));
            let (id, kind, parent_cause) = fallback_key(w, e, hashed.digest)?;
            (id, kind, Some(parent_cause.map_or(cause, |p| merge(Some(cause), p))))
        }
    };
    let mut cause = hashed.cause;
    if let Some(c) = key_cause {
        cause = Some(merge(cause, c));
    }
    let (class, file) = if kind == KeyKind::Namespace {
        (GroupClass::Tu, Vec::new())
    } else {
        match class {
            UnitClass::Declared => (GroupClass::Declared, w.file_of(e)),
            UnitClass::Derived | UnitClass::OnDemand => (GroupClass::Derived, origin_file(w, e)),
        }
    };
    Some(Written { entity: Some(e), key, kind, digest: hashed.digest, cause, class, file })
}

/// The fallback key of a unit whose key cannot be formed: its own key
/// kind, parent the nearest enclosing entity with a key (else `root`),
/// name `content`, discriminator the unit's digest.
fn fallback_key(
    w: &mut Walk<'_>,
    e: DeclId,
    digest: Digest,
) -> Option<(Digest, KeyKind, Option<Cause>)> {
    let ast = w.ast;
    let kind = match info(ast.decls.get(e).kind()).role {
        Role::Entity(k) | Role::Content(k) => k,
        Role::Nothing
        | Role::Transparent
        | Role::Root
        | Role::Local
        | Role::InParent
        | Role::Unsupported(_) => return None,
    };
    let mut parent = Parent::Root;
    let mut parent_cause = None;
    let mut c = ast.decls.get(e).ctx();
    for _ in 0..4096 {
        let Some(x) = c else { break };
        match info(ast.decls.get(x).kind()).role {
            Role::Root => break,
            Role::Entity(_) | Role::Content(_) => {
                if let KeyRes::Key { id, cause, .. } = w.key(x) {
                    parent = Parent::Unit(id);
                    parent_cause = cause;
                    break;
                }
            }
            Role::Nothing
            | Role::Transparent
            | Role::Local
            | Role::InParent
            | Role::Unsupported(_) => {}
        }
        c = ast.decls.get(x).ctx();
    }
    let key = w.rules.map(Key { kind, parent, name: Name::Content, disc: Some(digest) });
    w.bytes += key.to_bytes().len() as u64;
    Some((key.id(), kind, parent_cause))
}

/// Makes the unit around each declaration of a kind without a key kind
/// indeterminate (`collect`); the order unit at the top level.
pub(crate) fn apply_taints(w: &Walk<'_>, written: &mut [Written]) {
    for &(scope, cause) in &w.taints {
        let target = scope
            .and_then(|s| written.iter().position(|u| u.entity == Some(s)))
            .or_else(|| written.iter().position(|u| u.kind == KeyKind::Order));
        if let Some(i) = target {
            written[i].cause = Some(merge(written[i].cause, cause));
        }
    }
}

/// The cause kept when `c` meets `prev` (`Stream::indeterminate`).
pub(crate) fn merge(prev: Option<Cause>, c: Cause) -> Cause {
    match prev {
        None => c,
        Some(Cause::Pending(_)) if !matches!(c, Cause::Pending(_)) => c,
        Some(p) => p,
    }
}

/// The order unit (`spec/digest.md` §2.1): pending.
pub(crate) fn order_unit(w: &mut Walk<'_>) -> Written {
    let key = Key { kind: KeyKind::Order, parent: Parent::Root, name: Name::Order, disc: None };
    let mut s = Stream::new();
    crate::order::write(w, &mut s);
    let h = s.finish(UNIT_TAG);
    let h = w.hashed(h);
    let key = w.rules.map(key);
    let bytes = key.to_bytes();
    w.bytes += bytes.len() as u64;
    Written {
        entity: None,
        key: key.id(),
        kind: KeyKind::Order,
        digest: h.digest,
        cause: h.cause,
        class: GroupClass::Tu,
        file: Vec::new(),
    }
}

/// The file whose derived group holds a derived unit: that of the first
/// declaration of what it derives from (`spec/digest.md` §11). Builtins
/// and template parameter objects have the empty path.
#[allow(
    clippy::many_single_char_names,
    reason = "the names of digest-kinds: s stream, d declaration, e entity, n node, q type"
)]
fn origin_file(w: &Walk<'_>, e: DeclId) -> Vec<u8> {
    let ast = w.ast;
    let mut d = e;
    for _ in 0..64 {
        let n = ast.decls.get(d);
        if ast.decls.builtin_id(d).is_some() {
            return Vec::new();
        }
        if info(n.kind()).on_demand {
            return Vec::new();
        }
        let next = if let Some((_, info)) = ast.spec_info(d) {
            Some(info.template)
        } else if let Some((_, info)) = ast.member_spec_info(d) {
            Some(info.instantiated_from)
        } else if all_implicit(w, d) {
            n.ctx()
        } else {
            None
        };
        match next {
            Some(x) if x != d => d = w.entity(x),
            Some(_) | None => return w.file_of(d),
        }
    }
    w.file_of(d)
}

/// Marks every unit whose key two different entities share
/// (`spec/digest.md` §3.4); returns how many keys collide. Namespaces are
/// merged, and declarations a scope may repeat are one entity when their
/// digests are equal too.
pub(crate) fn mark_collisions(written: &mut [Written]) -> u64 {
    let mut order: Vec<usize> = (0..written.len()).collect();
    order.sort_by(|&a, &b| written[a].key.cmp(&written[b].key).then(a.cmp(&b)));
    let mut collisions = 0;
    let mut i = 0;
    while i < order.len() {
        let mut j = i + 1;
        while j < order.len() && written[order[j]].key == written[order[i]].key {
            j += 1;
        }
        let run = &order[i..j];
        if run.len() > 1 && collides(written, run) {
            collisions += 1;
            for &k in run {
                written[k].cause = Some(merge(written[k].cause, Cause::Collision));
            }
        }
        i = j;
    }
    collisions
}

fn collides(written: &[Written], run: &[usize]) -> bool {
    let first = &written[run[0]];
    if run.iter().all(|&k| written[k].kind == KeyKind::Namespace) {
        return false;
    }
    let repeatable = run.iter().all(|&k| repeatable(written[k].kind));
    !(repeatable && run.iter().all(|&k| written[k].digest == first.digest))
}

/// The declarations a scope may repeat and Kiln does not join
/// (`spec/digest.md` §3.4).
fn repeatable(kind: KeyKind) -> bool {
    matches!(
        kind,
        KeyKind::UsingDeclaration
            | KeyKind::UsingShadow
            | KeyKind::NamespaceAlias
            | KeyKind::UsingEnum
            | KeyKind::UsingDirective
            | KeyKind::StaticAssert
            | KeyKind::Friend
            | KeyKind::FileScopeAsm
            | KeyKind::ConstevalBlock
            | KeyKind::TemplateParamObject
    )
}

impl Walk<'_> {
    /// The declarations of entity `e`, in Kiln's chain order (latest
    /// first); one element for a declaration that is not redeclarable.
    pub(crate) fn redecls(&self, e: DeclId) -> Vec<DeclId> {
        let ast = self.ast;
        if ast.decls.redecl(e).is_none() {
            return vec![e];
        }
        let mut out: Vec<DeclId> = Vec::new();
        for d in ast.redecls_of(e) {
            // Past the limit the list ends with one more element, which
            // `decl_node` makes `Unaccounted("redecls/limit")`.
            if out.len() > REDECL_LIMIT {
                break;
            }
            out.push(d);
        }
        if out.is_empty() {
            out.push(e);
        }
        out
    }

    /// The subject path of the file of `d`'s location, or of the
    /// outermost macro expansion that produced it (`spec/digest.md` §11).
    pub(crate) fn file_of(&self, d: DeclId) -> Vec<u8> {
        let loc = self.ast.decls.get(d).loc();
        let loc = self.sm.expansion_loc(loc);
        match self.sm.offset_of(loc) {
            Some((file, _)) => (self.subject_path)(file),
            None => Vec::new(),
        }
    }
}
