//! Which entities are units, and writing one (`spec/digest.md` §2, §3.4,
//! §11; `spec/digest-kinds.md` §5.2).

use slip_kiln::kiln::ast::decl::{DeclFlags, DeclKind, TemplLinkKind, VarStorage};
use slip_kiln::kiln::ast::ids::DeclId;
use slip_kiln::kiln::ast::templates::{SpecKind, is_template_kind};
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
    /// The declaration the unit's name is read from: the one its stream
    /// is read from for an entity declared only by instantiations, else
    /// the representative (WP-032).
    pub read: Option<DeclId>,
    pub key: Digest,
    pub kind: KeyKind,
    pub digest: Digest,
    pub cause: Option<Cause>,
    pub class: GroupClass,
    pub file: Vec<u8>,
    /// For a resolved unit, its property (`demand.rs`); its `entity` is
    /// the owner's.
    pub resolved: Option<crate::demand::Resolved>,
    /// For a `default_argument` unit (digest-kinds §11.4), and for a
    /// resolved unit of one, the parameter's index (0 for a field); its
    /// `entity` is the function's or field's.
    pub default_argument: Option<u64>,
    /// For a part (a resolved or `default_argument` unit), the key of the
    /// unit it is keyed under.
    pub owner: Option<Digest>,
    /// Whether the unit's key is in a collision (§3.4), whatever cause
    /// `cause` keeps first.
    pub collided: bool,
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
///
/// An entity that would be declared but has no declaration with a
/// location (Kiln's implicit global allocation functions, `__int128_t`,
/// `std::align_val_t`) is derived on demand, as a builtin is
/// (`spec/digest.md` §2.2), whatever its flags: what is not written
/// anywhere has no declared group to be in.
pub(crate) fn unit_class(w: &Walk<'_>, e: DeclId) -> Option<UnitClass> {
    match class_of(w, e) {
        Some(UnitClass::Declared) if !w.chain(e).iter().any(|&d| w.has_location(d)) => {
            Some(UnitClass::OnDemand)
        }
        class => class,
    }
}

/// A using-shadow that the using-declaration does not introduce: a member
/// of its class hides or overrides the declaration it names
/// ([namespace.udecl]/14). Kiln still makes the shadow, marked
/// `hidden_shadow` with the member, when the member is declared after the
/// using-declaration, and makes none when it is declared before; so such a
/// shadow is no unit and no member its class lists (`spec/digest.md` §2.1;
/// WP-119 item 6), and the two orders digest alike. Only a hider written in
/// source counts: an implicit member (a copy or move assignment operator,
/// a constructor) is declared on use, and the mark it sets would make the
/// unit depend on what the translation unit used (§1 rule 5), so a shadow
/// it hides stays a unit, as before. Every declaration of the entity must
/// be so marked.
pub(crate) fn hidden_shadow(w: &Walk<'_>, e: DeclId) -> bool {
    let ast = w.ast;
    if !matches!(ast.decls.get(e).kind(), DeclKind::UsingShadow | DeclKind::ConstructorUsingShadow)
    {
        return false;
    }
    w.chain(e).iter().all(|&d| {
        ast.decls.hidden_shadow(d).is_some_and(|h| !ast.decls.get(h).has(DeclFlags::IMPLICIT))
    })
}

/// [`unit_class`] before the rule for entities without a location.
fn class_of(w: &Walk<'_>, e: DeclId) -> Option<UnitClass> {
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
    // A local entity is no unit, but for what would be a derived unit
    // were its local type not local (digest §4.3).
    if w.is_local(e) && !w.is_local_extern(e) && !crate::local::derived_under_local(w, e) {
        return None;
    }
    if let Some((TemplLinkKind::Pattern, _)) = ast.templ_link(e).get() {
        return None;
    }
    if hidden_shadow(w, e) {
        return None;
    }
    // Declared only in the bodies of templated functions and of their
    // instantiations: no unit (`spec/digest.md` §2.2; WP-032, check 3).
    if w.chain(e).iter().all(|&d| in_templated_body(w, d)) {
        return None;
    }
    // Declared only by instantiations, outside their classes: derived
    // (`spec/digest.md` §2.2, WP-032 rule 2).
    if outside_only(w, e) {
        return Some(UnitClass::Derived);
    }
    // An entity with a written declaration is classified from it, not
    // from a declaration an instantiation added first (WP-032 rule 1).
    let e = w.representative(e);
    let n = ast.decls.get(e);
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
    // The effective kind (§11.10, `demand::effective_kind`): an
    // implicit instantiation and its members are derived, an explicit
    // instantiation (declaration or definition) or specialisation and
    // their members declared, whatever Kiln instantiated first. An
    // explicitly specialised member template is an explicit
    // specialisation there.
    if let Some(k) = crate::demand::effective_kind(w, e) {
        return spec_class(k);
    }
    Some(UnitClass::Declared)
}

/// An enclosing context of `e` is an implicitly instantiated
/// specialisation, or a member of one, with no explicitly specialised
/// context in between (whose members are written in source): the nearest
/// context with an effective kind (`demand::effective_kind`, §11.10) has
/// `ImplicitInstantiation`. A local class has none of its own, so the
/// function that holds it answers.
pub(crate) fn in_implicit_instantiation(w: &Walk<'_>, e: DeclId) -> bool {
    implicit_from(w, w.ast.decls.get(e).ctx())
}

/// The first of `c` and its contexts with an effective kind has
/// `ImplicitInstantiation`.
fn implicit_from(w: &Walk<'_>, mut c: Option<DeclId>) -> bool {
    let ast = w.ast;
    for _ in 0..64 {
        let Some(x) = c else { return false };
        if let Some(k) = crate::demand::effective_kind(w, x) {
            return k == SpecKind::ImplicitInstantiation;
        }
        if specialised_member_template(w, x) {
            return false;
        }
        c = ast.decls.get(x).ctx();
    }
    false
}

/// `x`, or the template whose pattern `x` is, is a member template of an
/// implicitly instantiated class that `template <> template <...>`
/// explicitly specialises ([temp.expl.spec]/16), so that it and its
/// members are written in source (`spec/digest.md` §2.2). Kiln keeps no
/// `SpecInfo` for it: it drops the `instantiated_from` of the template's
/// first declaration (`Sema::specialize_member_template`), which every
/// member template an instantiation declares has.
pub(crate) fn specialised_member_template(w: &Walk<'_>, x: DeclId) -> bool {
    let ast = w.ast;
    let t = if is_template_kind(ast.decls.get(x).kind()) {
        x
    } else {
        match ast.described_template(x) {
            Some(t) => t,
            None => return false,
        }
    };
    ast.templates.instantiated_from_map(ast.first(t)).is_none() && in_implicit_instantiation(w, t)
}

/// The kind of `x`'s own `SpecInfo`, else of its `MemberSpecInfo`.
fn own_spec_kind(w: &Walk<'_>, x: DeclId) -> Option<SpecKind> {
    let ast = w.ast;
    ast.spec_info(x).map(|(_, i)| i.kind).or_else(|| ast.member_spec_info(x).map(|(_, i)| i.kind))
}

/// Declaration `d` is *instantiated outside its class* (`spec/digest.md`
/// §2.2, WP-032): its lexical context is an implicitly instantiated
/// class specialisation or a member of one (`implicit_from`
/// applied to the lexical context), and its semantic context is not.
pub(crate) fn instantiated_outside_class(w: &Walk<'_>, d: DeclId) -> bool {
    let ast = w.ast;
    let Some(lex) = ast.decls.lexical_ctx(d) else { return false };
    if Some(lex) == ast.decls.get(d).ctx() {
        return false;
    }
    let lexically = implicit_from(w, Some(lex));
    lexically && !in_implicit_instantiation(w, d)
}

/// Declaration `d` is one an instantiation made outside its class for a
/// member of a class specialisation: its semantic context is a class, its
/// lexical context another, and it was made by instantiation
/// (`templ::instantiated_declaration`). Kiln makes one when a static data
/// member of an instantiation is used (`instantiate_static_member_def`,
/// `kiln_sema/src/templ/class.rs:1943`), in the namespace of the
/// pattern's out-of-line definition.
pub(crate) fn instantiated_out_of_line(w: &Walk<'_>, d: DeclId) -> bool {
    let ast = w.ast;
    let Some(sem) = ast.decls.get(d).ctx() else { return false };
    if ast.decls.record(sem).is_none() {
        return false;
    }
    // A friend's lexical context is a class: only a namespace-scope
    // declaration is out of line.
    let lex = ast.decls.lexical_ctx(d);
    lex.is_some_and(|l| l != sem && ast.decls.record(l).is_none())
        && crate::templ::instantiated_declaration(w, d)
}

/// Declaration `d` names an entity outside the function whose body holds
/// it (a block-scope `extern` declaration, or a friend of a local
/// class), and that function is templated or is in an implicit
/// instantiation (`spec/digest.md` §2.2; WP-032, check 3). Kiln chains the
/// pattern's declaration to no other ([temp.spec.general]/2,
/// `Sema::note_local_extern`), and an instantiation's declaration
/// repeats the pattern's: neither is a declaration a unit reads.
pub(crate) fn in_templated_body(w: &Walk<'_>, d: DeclId) -> bool {
    let ast = w.ast;
    let lex = ast.decls.lexical_ctx(d);
    if !w.is_local_extern(d) && lex == ast.decls.get(d).ctx() {
        return false;
    }
    let mut ctx = lex;
    let mut in_body = false;
    for _ in 0..4096 {
        let Some(x) = ctx else { return false };
        let kind = ast.decls.get(x).kind();
        if kind == DeclKind::TranslationUnit || kind == DeclKind::Namespace {
            return false;
        }
        in_body = in_body || info(kind).local_scope;
        if in_body {
            match own_spec_kind(w, x) {
                Some(SpecKind::ImplicitInstantiation) => return true,
                Some(
                    SpecKind::ExplicitSpecialization
                    | SpecKind::ExplicitInstantiationDeclaration
                    | SpecKind::ExplicitInstantiationDefinition
                    | SpecKind::Undeclared,
                ) => return false,
                None => {}
            }
            if ast.described_template(x).is_some()
                || kind == DeclKind::ClassTemplatePartialSpecialization
            {
                return true;
            }
        }
        ctx = ast.decls.lexical_ctx(x);
    }
    false
}

/// Every declaration of entity `e` is instantiated outside its class
/// (WP-032 rule 2): the entity exists only through instantiations.
pub(crate) fn outside_only(w: &Walk<'_>, e: DeclId) -> bool {
    w.chain(e).iter().all(|&d| instantiated_outside_class(w, d))
}

/// `from`, the `TemplateTables` `instantiated_from` entry of the written
/// declaration `d`, is a friend's pattern an instantiation set on it
/// (`kiln_sema/src/templ/member.rs:316-329`): a declaration lexically in
/// a class but not semantically, while `d` itself is not instantiated
/// outside its class. Written absent (WP-032 rule 1).
pub(crate) fn instantiated_friend_link(w: &Walk<'_>, d: DeclId, from: DeclId) -> bool {
    let ast = w.ast;
    let Some(lex) = ast.decls.lexical_ctx(from) else { return false };
    let friend_pattern = ast.decls.record(lex).is_some() && ast.decls.get(from).ctx() != Some(lex);
    friend_pattern && !instantiated_outside_class(w, d)
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

/// A declaration whose lexical context lies in a class template's
/// pattern (or a partial specialisation) and whose semantic context is
/// another: a friend the pattern declares. Kiln chains one to a later
/// namespace-scope declaration only on its path for the global allocation
/// functions (`kiln_sema/src/new_delete.rs` `act_on_global_new_delete_decl`
/// finds it by `lookup_local`; ordinary redeclaration lookup does not),
/// so such a chain reads, and takes its file from, the other declarations
/// (`spec/landed/model/WP-050.md`).
pub(crate) fn pattern_friend(w: &Walk<'_>, d: DeclId) -> bool {
    let ast = w.ast;
    let lex = ast.decls.lexical_ctx(d);
    if lex.is_none() || lex == ast.decls.get(d).ctx() {
        return false;
    }
    let mut ctx = lex;
    for _ in 0..4096 {
        let Some(x) = ctx else { return false };
        let kind = ast.decls.get(x).kind();
        if kind == DeclKind::TranslationUnit || kind == DeclKind::Namespace {
            return false;
        }
        if ast.described_template(x).is_some()
            || kind == DeclKind::ClassTemplatePartialSpecialization
        {
            return true;
        }
        ctx = ast.decls.lexical_ctx(x);
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
/// An entity of the definition class (§11.9) is read from its first
/// written declaration (`Walk::representative`): its definition is the
/// resolved unit `definition`'s, and Kiln may make it on use.
pub(crate) fn read_decl(w: &Walk<'_>, e: DeclId) -> DeclId {
    let ast = w.ast;
    if crate::demand::definition_class(w, e) {
        return w.representative(e);
    }
    if let Some(pattern) = ast.template_info(e).and_then(|i| i.pattern) {
        return defining_decl(w, pattern)
            .and_then(|def| ast.described_template(def))
            .unwrap_or_else(|| w.representative(e));
    }
    defining_decl(w, e).unwrap_or_else(|| w.representative(e))
}

/// The declaration an entity declared only by instantiations outside
/// their class is read from (digest §2.2, WP-032 rule 3): the one whose
/// declaring class specialisation (its lexical context) has the least
/// mapped key, by the key's bytes; among declarations of one class (a
/// friend declared twice), the one whose own stream digest is least.
/// Its unit's stream, origin file and name are read from it, once; a
/// definition another of them holds is not part of the unit.
pub(crate) fn canonical_decl(w: &mut Walk<'_>, e: DeclId) -> DeclId {
    let ast = w.ast;
    let mut best: Option<(Vec<u8>, Vec<DeclId>)> = None;
    for d in w.chain(e) {
        #[cfg(test)]
        tests::count_candidate(false);
        let class = ast.decls.lexical_ctx(d).and_then(|c| w.key_bytes(c)).unwrap_or_default();
        match &mut best {
            Some((b, tied)) if *b == class => tied.push(d),
            Some((b, _)) if *b < class => {}
            Some(_) | None => best = Some((class, vec![d])),
        }
    }
    let Some((_, tied)) = best else { return e };
    if tied.len() == 1 {
        return tied[0];
    }
    let mut least: Option<(Digest, DeclId)> = None;
    for d in tied {
        #[cfg(test)]
        tests::count_candidate(true);
        let saved = w.take_unit();
        w.unit.local.holder = Some(e);
        let mut s = Stream::new();
        w.decl_stream(&mut s, d, e);
        w.unit = saved;
        let h = s.finish(UNIT_TAG);
        if least.is_none_or(|(l, _)| h.digest < l) {
            least = Some((h.digest, d));
        }
    }
    least.map_or(e, |(_, d)| d)
}

/// The defining declaration of entity `e` (§2.3), if it has one. Kiln
/// records `definition` for classes, enumerations and functions only; for
/// a variable it is the first declaration of the chain that is a
/// definition ([basic.def]/2), the search Kiln's own reading of a
/// variable's initialiser makes over the chain (`kiln_oven/src/value.rs`).
///
/// A definition an instantiation added outside its class is none: of an
/// entity that has a written declaration, nor of one declared only by
/// instantiations, whose unit is read from one declaration and whose
/// definition is its class specialisation's (WP-032 rules 1 and 3).
pub(crate) fn defining_decl(w: &Walk<'_>, e: DeclId) -> Option<DeclId> {
    let ast = w.ast;
    if ast.decls.var(e).is_none() {
        // Through the representative: Kiln's definition of a template is
        // the template asked about (`Ast::definition`).
        return ast
            .definition(w.representative(e))
            .filter(|&def| !instantiated_outside_class(w, def));
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
    let outside = !content && outside_only(w, e);
    let read = if outside { canonical_decl(w, e) } else { w.representative(e) };
    let hashed = if content {
        w.content_hash(e)
    } else {
        // `local::stream_source`, which numbering an owner reads from.
        let from = if outside { read } else { read_decl(w, e) };
        let saved = w.take_unit();
        w.unit.local.holder = Some(e);
        let mut s = Stream::new();
        w.decl_stream(&mut s, from, e);
        crate::enclosing::write(w, &mut s, e);
        // The list comes after the unit stream, whose end is `enclosing`
        // (digest-kinds §1.7, §4.4 "Spellings" item 6).
        crate::own_spelling::write_unit_list(w, &mut s);
        w.unit = saved;
        #[cfg(test)]
        tests::count_read(e);
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
            UnitClass::Declared => {
                if let Some(file) = declared_file(w, e) {
                    (GroupClass::Declared, file)
                } else {
                    // `unit_class` makes an entity without a location
                    // on demand, so this is a defect of the walk: the
                    // unit goes to the derived group with the empty
                    // path, indeterminate, instead of making the shard
                    // invalid (`spec/landed/model/WP-050.md`).
                    cause = Some(merge(cause, Cause::Unaccounted("unit/file")));
                    (GroupClass::Derived, Vec::new())
                }
            }
            UnitClass::Derived | UnitClass::OnDemand => (GroupClass::Derived, origin_file(w, read)),
        }
    };
    let entity = Some(e);
    Some(Written {
        entity,
        read: Some(read),
        key,
        kind,
        digest: hashed.digest,
        cause,
        class,
        file,
        resolved: None,
        default_argument: None,
        owner: None,
        collided: false,
    })
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
    let key = w.map_key(e, None, Key { kind, parent, name: Name::Content, disc: Some(digest) });
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
        owner: None,
        collided: false,
        entity: None,
        read: None,
        key: key.id(),
        kind: KeyKind::Order,
        digest: h.digest,
        cause: h.cause,
        class: GroupClass::Tu,
        file: Vec::new(),
        resolved: None,
        default_argument: None,
    }
}

/// The file whose declared group holds the declared unit of `e`: that of
/// its first written declaration, except for an instantiated entity (an
/// effective kind of an instantiation, §11.10) that is a member of a
/// specialisation or a function: Kiln moves such a function to the
/// pattern's definition when it instantiates the definition
/// (`relocate_to_pattern_definition`, `kiln_ast/src/templates.rs:1989`,
/// called at `kiln_sema/src/templ/class.rs:1597` and `:1700`), so its
/// file is the file of the declaration it was instantiated from
/// (`origin_file`), whether or not the definition was instantiated
/// (digest-kinds §11.1). `None` when no declaration the walk may read
/// has a location (`Walk::located`).
pub(crate) fn declared_file(w: &Walk<'_>, e: DeclId) -> Option<Vec<u8>> {
    let ast = w.ast;
    let instantiated = matches!(
        crate::demand::effective_kind(w, e),
        Some(
            SpecKind::ImplicitInstantiation
                | SpecKind::ExplicitInstantiationDeclaration
                | SpecKind::ExplicitInstantiationDefinition
        )
    );
    if instantiated && (ast.member_spec_info(e).is_some() || ast.decls.function(e).is_some()) {
        return Some(origin_file(w, e));
    }
    w.located(w.representative(e)).map(|d| w.file_of(d))
}

/// The file whose derived group holds a derived unit: that of the first
/// declaration of what it derives from (`spec/digest.md` §11). Builtins
/// and template parameter objects have the empty path.
#[allow(
    clippy::many_single_char_names,
    reason = "the names of digest-kinds: s stream, d declaration, e entity, n node, q type"
)]
pub(crate) fn origin_file(w: &Walk<'_>, e: DeclId) -> Vec<u8> {
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
            Some(_) | None => return w.unit_file(d),
        }
    }
    w.unit_file(d)
}

/// Marks every unit whose key two different entities share
/// (`spec/digest.md` §3.4); returns how many keys collide. Namespaces are
/// merged, and declarations a scope may repeat are one entity when their
/// digests are equal too. A run made only of parts (resolved or
/// `default_argument` units, keyed under their owner's key) is left to
/// [`mark_part_collisions`], which runs once every owner is marked.
pub(crate) fn mark_collisions(written: &mut [Written]) -> u64 {
    let mut collisions = 0;
    for run in runs(written) {
        if run.len() > 1
            && collides(written, &run)
            && !run.iter().all(|&k| is_part(written[k].kind))
        {
            collisions += 1;
            for &k in &run {
                collide(&mut written[k]);
            }
        }
    }
    collisions
}

/// Marks `u` as in a collision: `collided`, and `Collision` merged into
/// its cause.
pub(crate) fn collide(u: &mut Written) {
    u.collided = true;
    u.cause = Some(merge(u.cause, Cause::Collision));
}

/// The parts of colliding owners (`spec/digest.md` §3.4, §12), after
/// [`mark_collisions`] and the mapped collisions: every part of an owner
/// marked `Collision` is marked `Collision` (a part follows its owner's
/// key and key cause), whether or not another part shares its key. A run
/// of parts sharing a key is counted unless every pair in it is explained
/// by its owners' collision: the two parts are under one owner key, that
/// key is marked `Collision`, and their owners are different entities.
/// Returns how many part keys are counted.
pub(crate) fn mark_part_collisions(written: &mut [Written]) -> u64 {
    follow_owners(written);
    let marked: std::collections::HashSet<Digest> =
        written.iter().filter(|u| u.collided).map(|u| u.key).collect();
    let mut collisions = 0;
    for run in runs(written) {
        if run.len() < 2
            || !run.iter().all(|&k| is_part(written[k].kind))
            || !collides(written, &run)
        {
            continue;
        }
        let explained = run.iter().enumerate().all(|(i, &a)| {
            run[i + 1..].iter().all(|&b| {
                let (a, b) = (&written[a], &written[b]);
                a.owner.is_some()
                    && a.owner == b.owner
                    && a.owner.is_some_and(|o| marked.contains(&o))
                    && a.entity != b.entity
            })
        });
        if !explained {
            collisions += 1;
        }
        for &k in &run {
            collide(&mut written[k]);
        }
    }
    follow_owners(written);
    collisions
}

/// Marks `Collision` every part whose owner key is marked, to a fixed
/// point (a part may be keyed under a part).
fn follow_owners(written: &mut [Written]) {
    loop {
        let marked: std::collections::HashSet<Digest> =
            written.iter().filter(|u| u.collided).map(|u| u.key).collect();
        let mut changed = false;
        for u in written.iter_mut() {
            if !u.collided && u.owner.is_some_and(|o| marked.contains(&o)) {
                collide(u);
                changed = true;
            }
        }
        if !changed {
            return;
        }
    }
}

/// The indices of `written` grouped by key, each run in index order.
fn runs(written: &[Written]) -> Vec<Vec<usize>> {
    let mut order: Vec<usize> = (0..written.len()).collect();
    order.sort_by(|&a, &b| written[a].key.cmp(&written[b].key).then(a.cmp(&b)));
    order.chunk_by(|&a, &b| written[a].key == written[b].key).map(<[usize]>::to_vec).collect()
}

/// A unit keyed under another unit's key (digest-kinds §11.1, §11.4).
fn is_part(kind: KeyKind) -> bool {
    matches!(kind, KeyKind::Resolved | KeyKind::DefaultArgument)
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
pub(crate) fn repeatable(kind: KeyKind) -> bool {
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
    /// The declarations of entity `e` a unit reads, in Kiln's chain order
    /// (latest first); one element for a declaration that is not
    /// redeclarable. Declarations instantiated outside their class are
    /// left out unless every declaration is one (WP-032 rule 1).
    pub(crate) fn redecls(&self, e: DeclId) -> Vec<DeclId> {
        let chain = self.chain(e);
        let read = |&d: &DeclId| !in_templated_body(self, d);
        let chain: Vec<DeclId> = if chain.iter().any(read) && chain.len() <= REDECL_LIMIT {
            chain.into_iter().filter(read).collect()
        } else {
            chain
        };
        if chain.len() > REDECL_LIMIT || chain.iter().all(|&d| instantiated_outside_class(self, d))
        {
            return chain;
        }
        // The out-of-line definition an instantiation makes for a static
        // data member when it is used is the resolved unit `definition`'s
        // (§11.9): the own unit does not read it.
        let definition = self.unit.definition;
        let chain: Vec<DeclId> = chain
            .into_iter()
            .filter(|&d| !instantiated_outside_class(self, d))
            .filter(|&d| definition || !instantiated_out_of_line(self, d))
            .collect();
        if chain.iter().all(|&d| pattern_friend(self, d)) {
            return chain;
        }
        chain.into_iter().filter(|&d| !pattern_friend(self, d)).collect()
    }

    /// The declaration an entity with a written declaration is classified
    /// from: its earliest declaration that is not instantiated outside its
    /// class, not in a templated body, and not a [`pattern_friend`] (that
    /// last only if another one is left); `e` itself for an entity that
    /// has none.
    pub(crate) fn representative(&self, e: DeclId) -> DeclId {
        let mut chain = self.chain(e);
        chain.reverse();
        self.first_read(&chain, |_| true).unwrap_or(e)
    }

    /// The earliest declaration of `chain` (earliest first) that `keep`
    /// holds of and that a unit is classified from (see
    /// [`Walk::representative`]).
    fn first_read(&self, chain: &[DeclId], keep: impl Fn(DeclId) -> bool) -> Option<DeclId> {
        let read = |d: DeclId| !instantiated_outside_class(self, d) && !in_templated_body(self, d);
        let pick = |friends: bool| {
            chain
                .iter()
                .copied()
                .find(|&d| read(d) && (friends || !pattern_friend(self, d)) && keep(d))
        };
        if chain.iter().any(|&d| read(d) && !pattern_friend(self, d)) {
            pick(false)
        } else {
            pick(true)
        }
    }

    /// Every declaration of entity `e`, in Kiln's chain order. A template
    /// is not redeclarable itself: Kiln keeps its declarations as those
    /// of its pattern (`Ast::first`, `template_redecl`), so its chain is
    /// the templates its pattern's declarations describe.
    pub(crate) fn chain(&self, e: DeclId) -> Vec<DeclId> {
        let ast = self.ast;
        let pattern = ast.template_info(e).and_then(|i| i.pattern);
        let (start, template) = match pattern {
            Some(p) if ast.decls.redecl(e).is_none() && ast.decls.redecl(p).is_some() => (p, true),
            Some(_) | None => (e, false),
        };
        if ast.decls.redecl(start).is_none() {
            return vec![e];
        }
        let mut out: Vec<DeclId> = Vec::new();
        for d in ast.redecls_of(start) {
            let d = if template {
                match ast.described_template(d) {
                    Some(t) => t,
                    None => continue,
                }
            } else {
                d
            };
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

    /// Whether declaration `d` has a location in some buffer (Kiln makes
    /// some implicit declarations with none).
    pub(crate) fn has_location(&self, d: DeclId) -> bool {
        let loc = self.ast.decls.get(d).loc();
        self.sm.offset_of(self.sm.expansion_loc(loc)).is_some()
    }

    /// The file of a unit whose origin is declaration `d`
    /// (`spec/digest.md` §11): that of `d` if it has a location, else of
    /// the first declaration of its chain, in chain order (earliest
    /// first), that has one; the empty path if none has.
    pub(crate) fn unit_file(&self, d: DeclId) -> Vec<u8> {
        self.located(d).map_or_else(Vec::new, |x| self.file_of(x))
    }

    /// The declaration [`Walk::unit_file`] reads the file of: `d` if it
    /// has a location, else the earliest declaration of its chain that
    /// has one among those [`Walk::representative`] may pick (so a friend
    /// of a class template's pattern, or a declaration an instantiation
    /// made outside its class, is not taken while another is left); `None`
    /// if none has.
    pub(crate) fn located(&self, d: DeclId) -> Option<DeclId> {
        if self.has_location(d) {
            return Some(d);
        }
        let mut chain = self.chain(d);
        chain.reverse();
        self.first_read(&chain, |x| self.has_location(x))
    }

    /// The subject path of the file of `d`'s location, or of the
    /// outermost macro expansion that produced it (`spec/digest.md` §11).
    fn file_of(&self, d: DeclId) -> Vec<u8> {
        let loc = self.ast.decls.get(d).loc();
        let loc = self.sm.expansion_loc(loc);
        match self.sm.offset_of(loc) {
            Some((file, _)) => (self.subject_path)(file),
            None => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use slip_kiln::kiln::ast::DeclId;
    use slip_kiln::kiln::ast::decl::DeclKind;
    use slip_kiln::kiln::ast::templates::SpecKind;
    use slip_model::Digest;
    use slip_model::digests::GroupClass;
    use slip_model::keys::RuleTable;

    use super::{UnitClass, instantiated_outside_class, outside_only, unit_class};
    use crate::decl_func::tests::{named, with_walk};
    use crate::walk::Walk;

    thread_local! {
        /// How many times `write` has read each entity's stream.
        static READS: std::cell::RefCell<std::collections::BTreeMap<DeclId, u64>> =
            const { std::cell::RefCell::new(std::collections::BTreeMap::new()) };
    }

    /// Counts one read of `e`'s stream by `write`.
    pub(super) fn count_read(e: DeclId) {
        READS.with(|r| *r.borrow_mut().entry(e).or_insert(0) += 1);
    }

    thread_local! {
        /// The candidates `canonical_decl` has keyed, and those whose
        /// stream it has read to break a tie.
        static CANDIDATES: std::cell::Cell<(u64, u64)> = const { std::cell::Cell::new((0, 0)) };
    }

    /// Counts one candidate `canonical_decl` keys, or (`read`) reads.
    pub(super) fn count_candidate(read: bool) {
        CANDIDATES.with(|c| {
            let (k, r) = c.get();
            c.set(if read { (k, r + 1) } else { (k + 1, r) });
        });
    }

    // WP-032 rule 3: an entity declared only by the instantiations of `n`
    // class templates has its stream read once, from one candidate, after
    // `n` candidates are keyed and none is read; the work grows linearly
    // (2,000 declaring specialisations cost about ten times 200, not a
    // hundred times).
    #[test]
    fn canonical_candidate_read_once() {
        let src = |n: usize| -> String {
            use std::fmt::Write as _;
            let mut s = String::new();
            for i in 0..n {
                let _ = writeln!(
                    s,
                    "template <class T> struct A{i} {{ friend void j(long); }};\nA{i}<int> a{i};"
                );
            }
            s
        };
        let run = |n: usize| -> std::time::Duration {
            let text = src(n);
            let j = with_walk(&text, |w| w.entity(named(w, "j", DeclKind::Function)[0]));
            READS.with(|r| r.borrow_mut().clear());
            CANDIDATES.with(|c| c.set((0, 0)));
            let (all, took) = units_timed(&text);
            let derived = all
                .into_iter()
                .filter(|(class, name, _, _)| *class == GroupClass::Derived && name == "j(long)")
                .count();
            assert_eq!(derived, 1, "one unit for j");
            assert_eq!(READS.with(|r| r.borrow().get(&j).copied()), Some(1), "read once");
            let (keyed, read) = CANDIDATES.with(std::cell::Cell::get);
            assert_eq!(read, 0, "no tie, no candidate's stream read");
            assert_eq!(keyed, n as u64, "each candidate keyed once");
            took
        };
        let small = run(200);
        let large = run(2000);
        assert!(large < small * 30, "200: {small:?}, 2000: {large:?}");
    }

    /// The kind of `d`'s lexical context's own `SpecInfo`, if any.
    fn lexical_spec(w: &Walk<'_>, d: DeclId) -> Option<SpecKind> {
        let lex = w.ast.decls.lexical_ctx(d)?;
        w.ast.spec_info(lex).map(|(_, i)| i.kind)
    }

    // WP-032: the predicate over hand-picked declarations. Only the
    // declarations an implicit instantiation makes of a friend, in the
    // class or in a member class of it, are instantiated outside their
    // class; the written one, the pattern's, an explicit specialisation's
    // and the `Friend` declarations themselves are not.
    #[test]
    fn instantiated_outside_class_predicate() {
        let src = "void g(int);\n\
                   template <class T> struct A {\n\
                     friend void g(int) {}\n\
                     struct In { friend void q(In) {} };\n\
                   };\n\
                   template <> struct A<char> { friend void g(int); };\n\
                   A<int> a;\n\
                   A<int>::In in;\n";
        with_walk(src, |w| {
            let gs = named(w, "g", DeclKind::Function);
            assert_eq!(gs.len(), 4, "written, pattern, A<char>'s, A<int>'s");
            let outside: Vec<DeclId> =
                gs.iter().copied().filter(|&d| instantiated_outside_class(w, d)).collect();
            assert_eq!(outside.len(), 1);
            assert_eq!(lexical_spec(w, outside[0]), Some(SpecKind::ImplicitInstantiation));
            for &d in &gs {
                if d != outside[0] {
                    assert_ne!(lexical_spec(w, d), Some(SpecKind::ImplicitInstantiation));
                }
            }
            // A member class of the instantiation: its friend too.
            let qs = named(w, "q", DeclKind::Function);
            assert_eq!(qs.len(), 2, "pattern, A<int>::In's");
            let q_outside: Vec<bool> =
                qs.iter().map(|&d| instantiated_outside_class(w, d)).collect();
            assert_eq!(q_outside, [false, true]);
            // `Friend` declarations are in their class semantically too.
            for d in super::super::decl_func::tests::of_kind(w, DeclKind::Friend) {
                assert!(!instantiated_outside_class(w, d));
            }
        });
    }

    // WP-032: a friend that redeclares a member of another implicit
    // instantiation (`B<int>::m`, declared in `A<int>`) is in an
    // instantiation semantically too, so not instantiated outside its
    // class: it stays a declaration of `B<int>::m`'s unit.
    #[test]
    fn friend_member_of_instantiation_is_not_outside() {
        let src = "template <class T> struct B { void m(); };\n\
                   template <class T> struct A { friend void B<T>::m(); };\n\
                   A<int> a;\n";
        with_walk(src, |w| {
            let ms = named(w, "m", DeclKind::CXXMethod);
            let friend: Vec<DeclId> = ms
                .iter()
                .copied()
                .filter(|&d| lexical_spec(w, d) == Some(SpecKind::ImplicitInstantiation))
                .filter(|&d| w.ast.decls.lexical_ctx(d) != w.ast.decls.get(d).ctx())
                .collect();
            assert_eq!(friend.len(), 1, "A<int>'s declaration of B<int>::m");
            let d = friend[0];
            assert!(!instantiated_outside_class(w, d));
            assert!(w.redecls(w.entity(d)).contains(&d));
        });
    }

    // WP-032 rule 2: a friend function template declared only by an
    // instantiation (not a function, so `definition_class` reaches its last
    // branch) is of the definition class, as its specialisations are
    // instantiated on use.
    #[test]
    fn friend_template_declared_by_instantiation_is_definition_class() {
        let src = "template <class T> struct A { template <class U> friend void g(U) {} };\n\
                   A<int> a;\n";
        with_walk(src, |w| {
            let gs = named(w, "g", DeclKind::FunctionTemplate);
            let inst = *gs.last().expect("A<int>'s g");
            let g = w.entity(inst);
            assert!(outside_only(w, g));
            assert!(crate::demand::definition_class(w, g));
        });
    }

    // WP-032: a friend of a local class in an instantiated member
    // function. The local class has no `SpecInfo` or `MemberSpecInfo` of
    // its own, so whether it is in an implicit instantiation is asked of
    // its contexts (`in_implicit_instantiation`).
    #[test]
    fn friend_of_local_class_in_instantiation_is_outside() {
        let src = "template <class T> struct A {\n\
                     int m() { void r(int); struct L { friend void r(int); }; return 0; }\n\
                   };\n\
                   int u = A<int>().m();\n";
        with_walk(src, |w| {
            let rs = named(w, "r", DeclKind::Function);
            let local: Vec<DeclId> = rs
                .iter()
                .copied()
                .filter(|&d| {
                    let lex = w.ast.decls.lexical_ctx(d);
                    lex != w.ast.decls.get(d).ctx()
                        && lex.is_some_and(|c| super::own_spec_kind(w, c).is_none())
                        && lex.is_some_and(|c| super::in_implicit_instantiation(w, c))
                })
                .collect();
            assert_eq!(local.len(), 1, "the instantiated local class's friend: {rs:?}");
            assert!(instantiated_outside_class(w, local[0]));
        });
    }

    // WP-032 rule 3: the canonical candidate is the declaration whose
    // declaring class specialisation has the least mapped key, by bytes.
    #[test]
    fn canonical_candidate_has_least_key() {
        let src = "template <class T> struct A { friend void j(long); };\n\
                   template <class T> struct C { friend void j(long); };\n\
                   A<int> a;\nC<int> c;\nA<long> b;\n";
        with_walk(src, |w| {
            let js = named(w, "j", DeclKind::Function);
            let j = w.entity(*js.last().expect("A<long>'s j"));
            let chain = w.chain(j);
            assert_eq!(chain.len(), 3);
            let key = |w: &mut Walk<'_>, d: DeclId| {
                let c = w.ast.decls.lexical_ctx(d).expect("a class");
                w.key_bytes(c).expect("a key")
            };
            let least = chain.iter().copied().min_by_key(|&d| key(w, d)).expect("a candidate");
            assert_eq!(super::canonical_decl(w, j), least);
        });
    }

    // WP-045 decision 4: Kiln instantiates a static data member's
    // out-of-line definition on use as a new redeclaration at namespace
    // scope. The own unit reads the first written declaration and leaves
    // that one out of its chain; the `definition` unit keeps it.
    #[test]
    fn instantiated_out_of_line_definition_is_left_out() {
        let src = "template <class T> struct A { static int s; };\n\
                   template <class T> int A<T>::s = 1;\n\
                   A<int> a;\n\
                   int y() { return A<int>::s; }\n";
        with_walk(src, |w| {
            let ss = named(w, "s", DeclKind::Var);
            let out: Vec<DeclId> =
                ss.iter().copied().filter(|&d| super::instantiated_out_of_line(w, d)).collect();
            assert_eq!(out.len(), 1, "A<int>'s instantiated definition of s: {ss:?}");
            let d = out[0];
            let e = w.entity(d);
            assert_ne!(e, d);
            assert!(crate::demand::definition_class(w, e));
            assert!(!w.redecls(e).contains(&d));
            assert_eq!(super::read_decl(w, e), w.representative(e));
            assert_ne!(super::read_decl(w, e), d);
            w.unit.definition = true;
            assert!(w.redecls(e).contains(&d));
            w.unit.definition = false;
            assert_eq!(crate::demand::defining_declaration(w, e), Some(d));
        });
    }

    // WP-032 rules 1 and 2: `k` is declared first by an instantiation and
    // then in source: a declared unit, classified from the written
    // declaration. `j` exists only through the instantiations: derived, of
    // the definition class.
    #[test]
    fn entity_declared_by_instantiation() {
        let src = "template <class T> struct A { friend void k(int); friend void j(long); };\n\
                   A<int> a;\n\
                   A<long> b;\n\
                   void k(int);\n";
        with_walk(src, |w| {
            let ks = named(w, "k", DeclKind::Function);
            let written = *ks.last().expect("the written k");
            let k = w.entity(written);
            assert_ne!(k, written, "Kiln's first declaration of k is the instantiation's");
            assert!(!outside_only(w, k));
            assert_eq!(w.representative(k), written);
            assert_eq!(w.redecls(k), vec![written]);
            assert_eq!(unit_class(w, k), Some(UnitClass::Declared));
            assert!(!crate::demand::definition_class(w, k));
            let js = named(w, "j", DeclKind::Function);
            let j = w.entity(js[1]);
            assert!(outside_only(w, j));
            assert_eq!(w.redecls(j).len(), 2, "both instantiations' declarations");
            assert_eq!(unit_class(w, j), Some(UnitClass::Derived));
            assert!(crate::demand::definition_class(w, j));
        });
    }

    /// The units of `src`: (class, name, key, digest).
    fn units(src: &str) -> Vec<(GroupClass, String, Digest, Digest)> {
        units_timed(src).0
    }

    /// `units(src)`, and how long the digest took (not the parse).
    fn units_timed(src: &str) -> (Vec<(GroupClass, String, Digest, Digest)>, std::time::Duration) {
        let files = [("main.cpp", src)];
        let snippet = slip_kiln::snippet::Snippet {
            files: &files,
            main: "main.cpp",
            include: &[""],
            std: "c++26",
        };
        let rules = RuleTable::empty();
        slip_kiln::snippet::parse(&snippet, |p| {
            assert_eq!(p.errors, 0, "{}", p.diagnostics);
            let path = |f| p.path(f).map(|s: &str| s.as_bytes().to_vec()).unwrap_or_default();
            let input = crate::Input { ast: p.ast, sm: p.sm, subject_path: &path };
            let options = crate::Options {
                rules: &rules,
                read_only: crate::ReadOnly::NO_FILE,
                detail: crate::Detail::All,
                names: true,
                rule_use: false,
            };
            let t = std::time::Instant::now();
            let o = crate::digest(&input, &options);
            let took = t.elapsed();
            let names: std::collections::BTreeMap<Digest, String> =
                o.names.iter().cloned().collect();
            let mut out = Vec::new();
            for g in &o.digests.groups {
                for u in g.units.iter().flatten() {
                    let name = names.get(&u.key).cloned().unwrap_or_default();
                    out.push((g.class, name, u.key, u.digest));
                }
            }
            (out, took)
        })
        .expect("the snippet parses")
    }

    // WP-032 rule 3, with `differ/_/member-like-friends-two-specialisations`:
    // the member-like friends of `M<int>`, `M<long>` and `N<int>` are three
    // derived units with three keys; changing `N`'s friend moves only the
    // unit `N<int>` declares.
    #[test]
    fn member_like_friends_move_alone() {
        let side = |b: bool| {
            let n = if b { "m(U *)" } else { "m(U)" };
            format!(
                "template <class T> concept C = true;\n\
                 template <class T> struct M {{ template <class U> friend void m(U) requires C<T> {{}} }};\n\
                 template <class T> struct N {{ template <class U> friend void {n} requires C<T> {{}} }};\n\
                 M<int> a;\nM<long> b;\nN<int> c;\n"
            )
        };
        let friends = |src: &str| -> BTreeSet<(Digest, Digest)> {
            units(src)
                .into_iter()
                .filter(|(class, name, _, _)| {
                    *class == GroupClass::Derived
                        && name.contains("::friend template <class> void m(")
                })
                .map(|(_, _, key, digest)| (key, digest))
                .collect()
        };
        let a = friends(&side(false));
        let b = friends(&side(true));
        assert_eq!(a.len(), 3, "three keys on side A");
        assert_eq!(b.len(), 3, "three keys on side B");
        assert_eq!(a.intersection(&b).count(), 2, "M<int>'s and M<long>'s do not move");
    }

    // WP-032 rule 3: `j`, declared by two instantiations, is one derived
    // unit whose digest does not depend on which came first.
    #[test]
    fn friend_of_two_instantiations_is_one_unit() {
        let src = |first: &str, second: &str| {
            format!("template <class T> struct A {{ friend void j(long); }};\n{first}\n{second}\n")
        };
        let j = |src: &str| -> Vec<(GroupClass, Digest, Digest)> {
            units(src)
                .into_iter()
                .filter(|(class, name, _, _)| *class == GroupClass::Derived && name == "j(long)")
                .map(|(class, _, key, digest)| (class, key, digest))
                .collect()
        };
        let a = j(&src("A<int> a;", "A<long> b;"));
        let b = j(&src("A<long> b;", "A<int> a;"));
        assert_eq!(a.len(), 1, "one derived unit");
        assert_eq!(a, b);
    }
}
