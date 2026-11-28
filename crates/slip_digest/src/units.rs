//! Which entities are units, and writing one (`spec/digest.md` §2, §3.4,
//! §11; `spec/digest-kinds.md` §5.2).

use slip_kiln::kiln::ast::Ast;
use slip_kiln::kiln::ast::decl::{DeclFlags, DeclKind, SubstFailedKind, TemplLinkKind, VarStorage};
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
    /// For a namespace's unit in a view that is not its own view
    /// ([`view_units`], WP-140, WP-144), that view (`true`: mapped/owned);
    /// `None` for every other unit.
    pub view: Option<bool>,
    /// For a view unit, whether the namespace stays (WP-144 §3(b): its
    /// mapped view is named `(the moved part of …)` rather than `(the
    /// owned part of …)`).
    pub stays: bool,
    /// For a view unit, the name its key denotes (`legacy` for the owned
    /// view of a dependency's `acme` under `relocate legacy => :: acme`),
    /// read from the key; `None` for every other unit, and when the key's
    /// name is not one a namespace has. `names::names` reports a view unit
    /// as `<denotes> (the owned part of <namespace>)` (WP-140 review F4).
    pub denotes: Option<String>,
    /// Whether the unit holds the definition of an entity the translation
    /// unit emits unreferenced (`emitted.rs`, WP-155): `Unit::EMITTED`,
    /// part of no digest.
    pub emitted: bool,
}

/// Every declaration that is the first of a unit, in arena order
/// (`spec/digest-kinds.md` §5.2); `OnDemand` ones wait for a `Ref`.
pub(crate) fn collect(w: &mut Walk<'_>) {
    let ast = w.ast;
    // Entities already classified: each is classified once, not once per
    // declaration.
    let mut seen: std::collections::HashSet<DeclId> = std::collections::HashSet::new();
    for d in ast.decls.ids() {
        match failed_substitution(ast, d) {
            FailedSubst::Yes => continue,
            FailedSubst::DepthLimit => {
                let scope = match info(ast.decls.get(d).kind()).role {
                    Role::Entity(_) | Role::Content(_) => Some(w.entity(d)),
                    Role::Nothing
                    | Role::Transparent
                    | Role::Root
                    | Role::Local
                    | Role::InParent
                    | Role::Unsupported(_) => enclosing_entity(w, d),
                };
                w.taints.push((scope, Cause::Unaccounted("decl/subst-failed-depth")));
            }
            FailedSubst::No => {}
        }
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

/// Whether declaration `d` itself carries a `subst_failed` mark saying
/// that substitution failed and no entity was formed (`spec/digest.md`
/// §2.1, §8; Kiln WP-3281).
pub(crate) fn is_no_entity_failed_substitution(ast: &Ast, d: DeclId) -> bool {
    match ast.decls.subst_failed(d) {
        Some(
            SubstFailedKind::FunctionSignature
            | SubstFailedKind::ExplicitSpecifier
            | SubstFailedKind::VariablePartialAmbiguity
            | SubstFailedKind::LocalOfFailedSignature,
        ) => true,
        Some(SubstFailedKind::ClassPartialAmbiguity | SubstFailedKind::OtherTrapped) | None => {
            false
        }
    }
}

/// Maximum parent chain steps walked by [`failed_substitution`], matching
/// Kiln's `Sema::decl_owner` bound.
pub(crate) const SUBST_FAILED_LIMIT: usize = 256;

/// Result of checking whether a declaration is, or is owned by, a no-entity
/// failed substitution (`spec/digest.md` §2.1, §8; Kiln `spec/ast.md` §4.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FailedSubst {
    /// Not a failed substitution (or the AST has none).
    No,
    /// The declaration itself, or an enclosing `closure_owner` / `ctx` within
    /// the step bound, is a no-entity failed substitution.
    Yes,
    /// The parent chain exceeded the step bound before reaching root or a
    /// failed substitution: fail closed (`Unaccounted("decl/subst-failed-depth")`).
    DepthLimit,
}

/// [`failed_substitution`] with an explicit step bound.
pub(crate) fn failed_substitution_with_limit(ast: &Ast, d: DeclId, limit: usize) -> FailedSubst {
    if ast.decls.cold.subst_failed.is_empty() {
        return FailedSubst::No;
    }
    let mut cur = Some(d);
    for _ in 0..limit {
        let Some(x) = cur else { return FailedSubst::No };
        if is_no_entity_failed_substitution(ast, x) {
            return FailedSubst::Yes;
        }
        cur = ast.closure_owner(x).or_else(|| ast.decls.get(x).ctx());
    }
    if cur.is_none() { FailedSubst::No } else { FailedSubst::DepthLimit }
}

/// Whether declaration `d` is a failed substitution that is no entity, or
/// is owned by one through its `closure_owner` / `ctx` chain (a parameter,
/// local, or closure created while a failed signature was being formed;
/// `spec/digest.md` §2.1, §8; Kiln `spec/ast.md` §4.1). Fails closed with
/// [`FailedSubst::DepthLimit`] past [`SUBST_FAILED_LIMIT`] steps.
pub(crate) fn failed_substitution(ast: &Ast, d: DeclId) -> FailedSubst {
    failed_substitution_with_limit(ast, d, subst_failed_limit())
}

#[cfg(not(test))]
const fn subst_failed_limit() -> usize {
    SUBST_FAILED_LIMIT
}

#[cfg(test)]
fn subst_failed_limit() -> usize {
    tests::subst_failed_limit()
}

/// Whether declaration `d` is known to be inside a no-entity failed
/// substitution (returning `false` on [`FailedSubst::DepthLimit`] so callers
/// never silently drop an entity past the step bound; `collect`, `decl_node`
/// and `write_ref` flag `DepthLimit` indeterminate instead).
pub(crate) fn in_failed_substitution(ast: &Ast, d: DeclId) -> bool {
    matches!(failed_substitution(ast, d), FailedSubst::Yes)
}

/// The indeterminate cause of an invalid declaration `d`, or `None` when
/// `d` is valid or is a class template specialisation whose partial
/// specialisation choice was ambiguous under a trap (`ClassPartialAmbiguity`,
/// written as a specialisation that was never instantiated: no definition,
/// members or bases; `spec/digest.md` §8, §10; WP-136).
pub(crate) fn invalid_cause(ast: &Ast, d: DeclId) -> Option<Cause> {
    if !ast.decls.get(d).has(DeclFlags::INVALID) {
        return None;
    }
    match ast.decls.subst_failed(d) {
        None => Some(Cause::Error),
        Some(SubstFailedKind::ClassPartialAmbiguity) => {
            if ast.definition(d).is_some()
                || ast.decls.members(d).next().is_some()
                || ast.decls.bases(d, &ast.lists).next().is_some()
            {
                Some(Cause::Unaccounted("decl/class-partial-ambiguity"))
            } else {
                None
            }
        }
        Some(SubstFailedKind::OtherTrapped) => Some(Cause::Unaccounted("decl/trapped")),
        Some(
            SubstFailedKind::FunctionSignature
            | SubstFailedKind::ExplicitSpecifier
            | SubstFailedKind::VariablePartialAmbiguity
            | SubstFailedKind::LocalOfFailedSignature,
        ) => Some(Cause::Unaccounted("ref/failed-substitution")),
    }
}

/// `digest.invalid.unmarked` (`spec/digest.md` §10, §12; WP-136): how many
/// declarations in the AST carry `DeclFlags::INVALID` without Kiln's
/// `subst_failed` mark. In an observer with no error diagnostic this must
/// be 0 (Kiln `spec/ast.md` §4.1 "The invariant").
pub(crate) fn unmarked_invalid(ast: &Ast) -> u64 {
    let mut n = 0u64;
    for d in ast.decls.ids() {
        if ast.decls.get(d).has(DeclFlags::INVALID) && ast.decls.subst_failed(d).is_none() {
            n += 1;
        }
    }
    n
}

/// [`unit_class`] before the rule for entities without a location.
fn class_of(w: &Walk<'_>, e: DeclId) -> Option<UnitClass> {
    let ast = w.ast;
    if w.chain(e).iter().all(|&d| in_failed_substitution(ast, d)) {
        return None;
    }
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
    // A specialisation under a local type (a generic lambda's call
    // operator for given arguments): what it would be were the type not
    // local, a derived unit whenever Kiln made it, whatever demanded it
    // (`spec/digest.md` §2.2, §4.3; WP-158). Before `in_builtin`, which
    // reads a closure type, `IMPLICIT`, as a builtin's implicit class and
    // made such a specialisation a unit only if a `Ref` named it: a
    // conversion to a function pointer or an unevaluated operand writes
    // none.
    if w.is_local(e)
        && let Some((_, i)) = ast.spec_info(e)
    {
        return spec_class(i.kind);
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
        // A candidate's strings are not the unit's (digest §9 "The names
        // digest"): the chosen one is written again by the unit.
        let names = w.names_off();
        w.unit.local.holder = Some(e);
        let mut s = Stream::new();
        w.dropped += 1;
        w.decl_stream(&mut s, d, e);
        w.dropped -= 1;
        w.names_end(names);
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
    let (hashed, emits) = if content {
        let h = w.content_hash(e);
        (h, w.content_emits.get(&e).cloned().unwrap_or_default())
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
        let emits = std::mem::take(&mut w.unit.emits);
        w.unit = saved;
        #[cfg(test)]
        tests::count_read(e);
        let h = s.finish(UNIT_TAG);
        (w.hashed(h), emits)
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
    // Rule 1 of the mark "emitted unreferenced" (`emitted.rs`, WP-155).
    let emitted = crate::emitted::written(w, e, emits);
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
        view: None,
        stays: false,
        denotes: None,
        emitted,
    })
}

/// The fallback key of a unit whose key cannot be formed: its own key
/// kind, parent the nearest enclosing entity with a key (else `root`),
/// name `content`, discriminator the unit's digest.
pub(crate) fn fallback_key(
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

/// The units of namespaces in a view that is not their own ownership
/// (`spec/digest.md` §3.4 "A namespace's unit in each view",
/// `spec/verify.md` §2.1 rule 2; WP-140), after every other unit is
/// written and tainted.
///
/// A namespace `N` is owned iff none of its openings lies in a read-only
/// file (`Walk::owned`); its own unit is keyed in that view. It has a unit
/// in the other view too when that view exists (`Walk::ns_holds`, the one
/// predicate, decided from the declarations of the image: a member of
/// that ownership, an entity whose semantic parent is `N`, declared by the
/// source or made by the compiler; a written content-keyed declaration;
/// or a nested namespace, named, inline or unnamed, whose view of that
/// ownership exists; `spec/verify.md` §2.1, WP-141's ruling after WP-140
/// review F3). An opening alone holds nothing. The unit is written when
/// its key differs from `N`'s own
/// key (equal keys would be one unit; the dedup of `groups::build` is not
/// relied on). Its digest is `N`'s own unit's, made of every opening (the
/// inline bit and the attributes of the whole chain): the digest is not
/// per view, or a fact one view's openings hold would be lost in the
/// other's unit (`spec/landed/digest/WP-140.md`, why option A was
/// rejected). Its cause is the own unit's, merged with the view key's. A
/// view whose key cannot be formed makes the own unit indeterminate with
/// its cause.
///
/// A reference to `N` (an alias's target, a using-directive, a qualifier,
/// a reflection) still writes `N`'s own key (`spec/verify.md` "Open
/// questions"). Under the empty table nothing is written.
pub(crate) fn view_units(w: &mut Walk<'_>, written: &mut Vec<Written>) {
    if w.rules.is_empty() {
        return;
    }
    let mut views = Vec::new();
    for (i, u) in written.iter().enumerate() {
        if u.kind != KeyKind::Namespace
            || u.view.is_some()
            || u.resolved.is_some()
            || u.default_argument.is_some()
        {
            continue;
        }
        let Some(e) = u.entity else { continue };
        views.push((i, e));
    }
    let first_view = written.len();
    for (i, e) in views {
        let KeyRes::Key { id: own, .. } = w.key(e) else { continue };
        let view = !w.ns_own_view(e);
        if !w.ns_holds(e, view) {
            continue;
        }
        let stays = w.ns_stays(e);
        match w.view_key(e, view) {
            KeyRes::Key { id, cause, .. } => {
                if id == own {
                    continue;
                }
                let mut u = written[i].clone();
                u.key = id;
                u.view = Some(view);
                u.stays = stays;
                // A guard (WP-140 review F5): a namespace key's cause is made
                // of its name's (an identifier or the unnamed namespace:
                // none) and its parents', which are the same namespaces'
                // in either view, so the view key's cause is the own key's,
                // which the own unit already holds (`write`). No source
                // makes the two differ.
                if let Some(c) = cause {
                    u.cause = Some(merge(u.cause, c));
                }
                if view {
                    crate::rule_use::view_unit(w, e);
                }
                written.push(u);
            }
            // A guard (WP-140 review F5): an error of a namespace key
            // (a refused table, a bad name) is the same in either view and
            // would have left the own key without one (`continue` above);
            // an abort (depth, cycle) cannot happen here, at depth 0 with
            // the enclosing namespaces' keys in this view asked, and
            // memoised, before their members' (`written` is in arena
            // order). No source reaches this arm.
            KeyRes::None(cause) => {
                let c = cause.unwrap_or(Cause::Unaccounted("key/view"));
                written[i].cause = Some(merge(written[i].cause, c));
            }
        }
    }
    if first_view == written.len() {
        return;
    }
    // The name each view unit's key denotes, from its key and the names its
    // parents' keys denote (a parent is a namespace's own or view unit).
    let mut by_key = std::collections::HashMap::new();
    for (j, u) in written.iter().enumerate() {
        if u.kind == KeyKind::Namespace && u.resolved.is_none() && u.default_argument.is_none() {
            by_key.entry(u.key).or_insert(j);
        }
    }
    let mut memo = std::collections::HashMap::new();
    for i in first_view..written.len() {
        let id = written[i].key;
        written[i].denotes = denoted(w, written, &by_key, &mut memo, id, 0);
    }
}

/// The qualified name the namespace key `id` denotes (`Written::denotes`):
/// its key's identifier (or the unnamed namespace) under the name its
/// parent's key denotes. `None` when a part has another form, or a parent
/// is no namespace unit here.
fn denoted(
    w: &mut Walk<'_>,
    written: &[Written],
    by_key: &std::collections::HashMap<Digest, usize>,
    memo: &mut std::collections::HashMap<Digest, Option<String>>,
    id: Digest,
    depth: u32,
) -> Option<String> {
    if let Some(n) = memo.get(&id) {
        return n.clone();
    }
    if depth > 64 {
        return None;
    }
    let u = &written[*by_key.get(&id)?];
    let key = crate::records::key_again(w, u.entity?, u.view)?;
    let leaf = match &key.name {
        Name::Identifier(s) => String::from_utf8_lossy(s).into_owned(),
        Name::Anonymous => "(anonymous namespace)".to_owned(),
        _ => return None,
    };
    let name = match key.parent {
        Parent::Root => Some(leaf),
        Parent::Unit(p) => {
            denoted(w, written, by_key, memo, p, depth + 1).map(|p| format!("{p}::{leaf}"))
        }
        Parent::CLinkage | Parent::Builtin => None,
    };
    memo.insert(id, name.clone());
    name
}

/// Makes the unit around each declaration of a kind without a key kind
/// indeterminate (`collect`); the order unit at the top level.
pub(crate) fn apply_taints(w: &Walk<'_>, written: &mut [Written]) {
    for &(scope, cause) in &w.taints {
        let target = scope
            // Not a part keyed under a local function, which has no unit of
            // its own (`demand::local_default_arguments`): the taint goes
            // where it went before those parts existed.
            .and_then(|s| {
                written
                    .iter()
                    .position(|u| u.entity == Some(s) && !(u.owner.is_some() && w.is_local(s)))
            })
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
        view: None,
        stays: false,
        denotes: None,
        emitted: false,
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
        static SUBST_FAILED_LIMIT_OVERRIDE: std::cell::Cell<usize> =
            const { std::cell::Cell::new(super::SUBST_FAILED_LIMIT) };
    }

    pub(super) fn subst_failed_limit() -> usize {
        SUBST_FAILED_LIMIT_OVERRIDE.with(std::cell::Cell::get)
    }

    fn with_subst_failed_limit<R>(limit: usize, f: impl FnOnce() -> R) -> R {
        struct Reset(usize);
        impl Drop for Reset {
            fn drop(&mut self) {
                SUBST_FAILED_LIMIT_OVERRIDE.with(|c| c.set(self.0));
            }
        }
        let prev = SUBST_FAILED_LIMIT_OVERRIDE.with(|c| c.replace(limit));
        let _reset = Reset(prev);
        f()
    }

    /// `hidden_shadow`: only a hider written in source counts. Since Kiln
    /// WP-3145/WP-3146 (`690711450`, in the pin since WP-163) an implicit
    /// member marks the shadow it hides at the class's closing brace, used
    /// or not, so `differ/_/wp119-shadow-hidden-by-implicit-member` sees
    /// the mark on both sides and no longer tells the rule from the
    /// mutation "an implicit hider counts"; this test does (WP-163 fix
    /// round 1, F5). `E`'s written `operator=(const D&)` hides the shadow;
    /// `D`'s implicit copy assignment marks it, and it stays a unit.
    #[test]
    fn only_a_hider_written_in_source_counts() {
        let src = "struct D;\n\
                   struct Base { Base& operator=(const D&); };\n\
                   struct D : Base { using Base::operator=; };\n\
                   struct E : Base { using Base::operator=; E& operator=(const D&); };\n";
        with_walk(src, |w| {
            let ast = w.ast;
            let shadows: Vec<DeclId> = ast
                .decls
                .ids()
                .filter(|&d| ast.decls.get(d).kind() == DeclKind::UsingShadow)
                .collect();
            let parent = |d: DeclId| {
                let p = ast.semantic_parent(d).expect("a class");
                let n = named(w, "D", DeclKind::CXXRecord);
                if n.iter().any(|&x| ast.first(x) == ast.first(p)) { "D" } else { "E" }
            };
            let mut seen: Vec<(&str, bool, bool)> = shadows
                .iter()
                .map(|&d| {
                    (parent(d), ast.decls.hidden_shadow(d).is_some(), super::hidden_shadow(w, d))
                })
                // The shadows of `Base`'s implicit copy and move
                // assignment operators are hidden by none.
                .filter(|&(_, marked, _)| marked)
                .collect();
            seen.sort_unstable();
            assert_eq!(seen, [("D", true, false), ("E", true, true)]);
        });
    }

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
                existing: &[],
                existing_local: &[],
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

    // WP-158 fix round (review F5): the candidates' streams that
    // `canonical_decl` digests to break a tie (a friend declared twice by
    // one specialisation) are dropped: inside the run's window they
    // record no hold. Today a candidate's declaration stream writes no
    // body, default or initialiser (the definition is the resolved unit's,
    // and a friend with a default argument has one declaration only), so
    // this guards the predicate's use there rather than kills a mutant.
    #[test]
    fn canonical_candidates_hold_nothing() {
        let src = "struct D;\n\
                   template <class T> struct S { friend int fr(D *, T *); \
                   friend int fr(D *, T *) { return sizeof(T); } };\n\
                   struct D : S<int> {};\n\
                   int w() { return fr((D *)nullptr, (int *)nullptr); }\n";
        with_walk(src, |w| {
            let inst = named(w, "fr", DeclKind::Function).into_iter().find(|&d| {
                !w.ast.decls.get(d).has(slip_kiln::kiln::ast::decl::DeclFlags::DEPENDENT_CTX)
            });
            let e = w.entity(inst.expect("an instantiated fr"));
            CANDIDATES.with(|c| c.set((0, 0)));
            w.hold_begin();
            let _ = super::canonical_decl(w, e);
            let recorded = w.hold_pending.as_ref().map_or(0, Vec::len);
            w.hold_end(true);
            let (_, read) = CANDIDATES.with(std::cell::Cell::get);
            assert!(read >= 2, "a tie: each candidate's stream read ({read})");
            assert_eq!(recorded, 0);
        });
    }

    /// Digests `src` (C++20, `main.cpp`) and returns `(output, errors)`.
    fn digest_src(src: &str) -> (crate::Output, u32) {
        let files = [("main.cpp", src)];
        let snippet = slip_kiln::snippet::Snippet {
            files: &files,
            main: "main.cpp",
            include: &[""],
            std: "c++20",
        };
        let rules = RuleTable::empty();
        slip_kiln::snippet::parse(&snippet, |p| {
            let paths: Vec<Vec<u8>> =
                p.sm.file_ids()
                    .map(|f| p.path(f).map(|s: &str| s.as_bytes().to_vec()).unwrap_or_default())
                    .collect();
            let (ast, sm) = (p.ast, p.sm);
            let out = std::thread::scope(|s| {
                std::thread::Builder::new()
                    .stack_size(crate::STACK_SIZE)
                    .spawn_scoped(s, || {
                        let path = |f: slip_kiln::kiln::source::FileId| {
                            paths.get(f.index()).cloned().unwrap_or_default()
                        };
                        let input = crate::Input { ast, sm, subject_path: &path };
                        let options = crate::Options {
                            rules: &rules,
                            read_only: crate::ReadOnly::NO_FILE,
                            existing: &[],
                            existing_local: &[],
                            detail: crate::Detail::All,
                            names: true,
                            rule_use: false,
                        };
                        crate::digest(&input, &options)
                    })
                    .expect("spawn")
                    .join()
                    .expect("join")
            });
            (out, p.errors)
        })
        .expect("parse")
    }

    fn counter(out: &crate::Output, name: &str) -> u64 {
        out.counters.iter().find(|c| c.name == name).map_or(0, |c| c.value)
    }

    // WP-136 items 1–2 (`spec/digest.md` §2.1, §8, §10): kinds 1–5 produce
    // zero indeterminate units and zero unmarked invalid declarations; kind 4
    // (class ambiguous partial specialisation) matches an unattempted
    // specialisation; an unmarked invalid declaration (a real error) stays
    // `Cause::Error` and increments `digest.invalid.unmarked`.
    #[test]
    fn wp136_kinds_1_to_3() {
        use slip_kiln::kiln::ast::decl::SubstFailedKind;

        let cases = [
            (
                "trailing-return",
                "template <class T> auto g(T t) -> decltype(t.size());\n\
                 int g(...);\n\
                 int u() { return g(1); }\n",
                SubstFailedKind::FunctionSignature,
            ),
            (
                "trapped-narrowing",
                "template <class T> auto h(T t) -> decltype(int{t});\n\
                 long h(...);\n\
                 long u() { return h(1.5); }\n",
                SubstFailedKind::FunctionSignature,
            ),
            (
                "kind2-explicit-specifier",
                "struct S {\n\
                   template <class T> explicit(T::value) S(T);\n\
                   S(...);\n\
                 };\n\
                 S s = 1;\n",
                SubstFailedKind::ExplicitSpecifier,
            ),
            (
                "kind3-var-partial-ambiguity",
                "template <class T, class U> constexpr int v = 0;\n\
                 template <class T> constexpr int v<T, int> = 1;\n\
                 template <class U> constexpr int v<int, U> = 2;\n\
                 template <class T> auto f(T) -> decltype(v<T, int>);\n\
                 long f(...);\n\
                 long x = f(1);\n",
                SubstFailedKind::VariablePartialAmbiguity,
            ),
        ];
        for (label, src, want_kind) in cases {
            with_walk(src, |w| {
                let marks = w.ast.decls.subst_failed_decls();
                assert!(
                    marks.iter().any(|&(_, k)| k == want_kind),
                    "{label}: expected {want_kind:?} in {marks:?}"
                );
            });
            let (out, errors) = digest_src(src);
            assert_eq!(errors, 0, "{label}");
            let named_causes: Vec<_> = out
                .causes
                .iter()
                .map(|&(k, c)| {
                    let n =
                        out.names.iter().find(|&&(nk, _)| nk == k).map_or("", |(_, s)| s.as_str());
                    (n, c)
                })
                .collect();
            assert!(named_causes.is_empty(), "{label}: {named_causes:?}");
            assert_eq!(counter(&out, "digest.invalid.unmarked"), 0, "{label}");
            assert_eq!(counter(&out, "digest.instantiated.unheld"), 0, "{label}");
            assert_eq!(counter(&out, "digest.emitted.unplaced"), 0, "{label}");
        }
    }

    #[test]
    fn wp136_lambda_and_kind5() {
        use slip_kiln::kiln::ast::decl::SubstFailedKind;

        // Lambda inside a failed signature: the pattern template itself has
        // `UndeclaredLocal` (a lambda in a template signature, §4.3), but the
        // failed specialisation `f<int>` and its valid closure are no units
        // and leave `digest.instantiated.unheld` and `digest.emitted.unplaced`
        // at 0.
        let lambda_sig = "template <class T> auto f(T t) -> decltype([]{ return 1; }(), t.size());\n\
                          int f(...);\n\
                          int u() { return f(1); }\n";
        let (lout, lerr) = digest_src(lambda_sig);
        assert_eq!(lerr, 0);
        assert!(
            !lout.names.iter().any(|(_, n)| n.contains("f<int>")),
            "failed spec f<int> must not be a unit: {:?}",
            lout.names
        );
        assert_eq!(counter(&lout, "digest.invalid.unmarked"), 0);
        assert_eq!(counter(&lout, "digest.instantiated.unheld"), 0);
        assert_eq!(counter(&lout, "digest.emitted.unplaced"), 0);

        // Kind 5 (local of a failed signature): a GNU statement-expression in
        // a generic lambda's trailing return type. The pattern's own key has
        // `key/local-ref` (a local variable in a signature), while the failed
        // specialisation `operator()<int>` and its kind-5 local `x` are no
        // units and leave zero `Cause::Error`, zero `ref/failed-substitution`,
        // and zero `digest.invalid.unmarked`.
        let kind5_src = "template <class L> constexpr bool callable = requires(L l) { l(1); };\n\
                         void g() {\n\
                           auto l = [](auto t) -> decltype(({ int x = t.foo(); x; })) { return 0; };\n\
                           static_assert(!callable<decltype(l)>);\n\
                         }\n";
        with_walk(kind5_src, |w| {
            let marks = w.ast.decls.subst_failed_decls();
            assert!(
                marks.iter().any(|&(_, k)| k == SubstFailedKind::LocalOfFailedSignature),
                "kind5: {marks:?}"
            );
        });
        let (k5out, k5err) = digest_src(kind5_src);
        assert_eq!(k5err, 0);
        assert!(
            !k5out.names.iter().any(|(_, n)| n.contains("operator()<int>")),
            "failed spec must not be a unit: {:?}",
            k5out.names
        );
        assert!(
            !k5out.causes.iter().any(|&(_, c)| matches!(
                c,
                crate::Cause::Error | crate::Cause::Unaccounted("ref/failed-substitution")
            )),
            "unexpected cause: {:?}",
            k5out.causes
        );
        assert_eq!(counter(&k5out, "digest.invalid.unmarked"), 0);
        assert_eq!(counter(&k5out, "digest.instantiated.unheld"), 0);
        assert_eq!(counter(&k5out, "digest.emitted.unplaced"), 0);
    }

    #[test]
    fn wp136_kind4_and_unmarked_error() {
        use slip_kiln::kiln::ast::decl::SubstFailedKind;

        // Kind 4: class ambiguous partial specialisation vs no attempt.
        let kind4_tried = "namespace a {\n\
            struct S {};\n\
            template <class T, class U> struct X {};\n\
            template <class T> struct X<T, S> { using t = int; };\n\
            template <class U> struct X<S, U> { using t = int; };\n\
            template <class T> auto probe(T) -> typename X<T, S>::t;\n\
            }\n\
            int probe(...);\n\
            a::X<a::S, a::S> *p = nullptr;\n\
            int u() { return probe(a::S{}); }\n";
        let kind4_untried = "namespace a {\n\
            struct S {};\n\
            template <class T, class U> struct X {};\n\
            template <class T> struct X<T, S> { using t = int; };\n\
            template <class U> struct X<S, U> { using t = int; };\n\
            template <class T> auto probe(T) -> typename X<T, S>::t;\n\
            }\n\
            int probe(...);\n\
            a::X<a::S, a::S> *p = nullptr;\n\
            int u() { return ::probe(a::S{}); }\n";
        with_walk(kind4_tried, |w| {
            let marks = w.ast.decls.subst_failed_decls();
            assert!(
                marks.iter().any(|&(_, k)| k == SubstFailedKind::ClassPartialAmbiguity),
                "kind4: {marks:?}"
            );
        });
        let (a, err_a) = digest_src(kind4_tried);
        let (b, err_b) = digest_src(kind4_untried);
        assert_eq!((err_a, err_b), (0, 0));
        assert!(a.causes.is_empty(), "kind4 tried: {:?}", a.causes);
        assert!(b.causes.is_empty(), "kind4 untried: {:?}", b.causes);
        assert_eq!(a.digests, b.digests, "kind4: failed attempt and no attempt must be equal");
        assert_eq!(counter(&a, "digest.invalid.unmarked"), 0);

        // Kind 4 fail-closed path (M10): a SFINAE ambiguity on `X<int, int>`
        // followed by an explicit specialisation `template<> struct X<int, int>`
        // leaves `ClassPartialAmbiguity` on a record with a definition/members,
        // so `invalid_cause` fails closed with `decl/class-partial-ambiguity`.
        let kind4_then_spec = "template <class T, class U> struct X {};\n\
                               template <class T> struct X<T, int> { using t = int; };\n\
                               template <class U> struct X<int, U> { using t = int; };\n\
                               template <class T> auto probe(T) -> typename X<T, int>::t;\n\
                               int probe(...);\n\
                               int first = probe(1);\n\
                               template <> struct X<int, int> { using t = int; };\n\
                               X<int, int>::t after = 0;\n";
        let (k4_out, k4_err) = digest_src(kind4_then_spec);
        assert_eq!(k4_err, 0);
        assert_eq!(counter(&k4_out, "digest.invalid.unmarked"), 0);
        assert!(
            k4_out.causes.iter().any(|&(k, c)| {
                c == crate::Cause::Unaccounted("decl/class-partial-ambiguity")
                    && k4_out.names.iter().any(|(nk, n)| *nk == k && n == "X<int, int>")
            }),
            "expected decl/class-partial-ambiguity on X<int, int>, got: {:?}",
            k4_out.causes
        );

        // Real error (unmarked INVALID): stays Cause::Error and counts in
        // `digest.invalid.unmarked`.
        let real_err = "template <class T, class U> struct X {};\n\
                        template <class T> struct X<T, int> {};\n\
                        template <class U> struct X<int, U> {};\n\
                        X<int, int> z;\n";
        let (err_out, errors) = digest_src(real_err);
        assert!(errors > 0, "expected a real error");
        assert!(
            err_out.causes.iter().any(|&(_, c)| c == crate::Cause::Error),
            "expected Cause::Error: {:?}",
            err_out.causes
        );
        assert!(counter(&err_out, "digest.invalid.unmarked") >= 1);
    }

    // WP-136 item 2 kind 6 (`spec/digest.md` §10): an `OtherTrapped`
    // declaration yields `Unaccounted("decl/trapped")`, not `Cause::Error`,
    // and is not counted as unmarked invalid.
    #[test]
    fn wp136_kind6_other_trapped_is_decl_trapped() {
        use slip_kiln::kiln::ast::Ast;
        use slip_kiln::kiln::ast::decl::{DeclFlags, SubstFailedKind};
        let files = [("main.cpp", "int x = 1; struct S;\n")];
        let snippet = slip_kiln::snippet::Snippet {
            files: &files,
            main: "main.cpp",
            include: &[""],
            std: "c++20",
        };
        let rules = RuleTable::empty();
        slip_kiln::snippet::parse(&snippet, |p| {
            let bytes = slip_kiln::kiln::snap::to_bytes(p.ast);
            let mut ast: Ast = slip_kiln::kiln::snap::from_bytes(&bytes).expect("decode ast");
            let x = ast
                .decls
                .ids()
                .find(|&d| {
                    ast.decls.get(d).kind() == DeclKind::Var
                        && ast
                            .decls
                            .get(d)
                            .name()
                            .as_ident()
                            .is_some_and(|s| ast.symbols().as_str(s) == "x")
                })
                .expect("var x");
            ast.decls.set_flags(x, DeclFlags::INVALID);
            ast.decls.set_subst_failed(x, SubstFailedKind::OtherTrapped);
            assert_eq!(
                super::invalid_cause(&ast, x),
                Some(crate::Cause::Unaccounted("decl/trapped"))
            );
            assert!(!super::is_no_entity_failed_substitution(&ast, x));
            assert_eq!(super::unmarked_invalid(&ast), 0);

            let paths: Vec<Vec<u8>> =
                p.sm.file_ids()
                    .map(|f| p.path(f).map(|s: &str| s.as_bytes().to_vec()).unwrap_or_default())
                    .collect();
            let sm = p.sm;
            let out = std::thread::scope(|sc| {
                std::thread::Builder::new()
                    .stack_size(crate::STACK_SIZE)
                    .spawn_scoped(sc, || {
                        let path = |f: slip_kiln::kiln::source::FileId| {
                            paths.get(f.index()).cloned().unwrap_or_default()
                        };
                        let input = crate::Input { ast: &ast, sm, subject_path: &path };
                        let options = crate::Options {
                            rules: &rules,
                            read_only: crate::ReadOnly::NO_FILE,
                            existing: &[],
                            existing_local: &[],
                            detail: crate::Detail::All,
                            names: true,
                            rule_use: false,
                        };
                        crate::digest(&input, &options)
                    })
                    .expect("spawn")
                    .join()
                    .expect("join")
            });
            assert_eq!(out.causes.len(), 1, "{:?}", out.causes);
            assert_eq!(out.causes[0].1, crate::Cause::Unaccounted("decl/trapped"));
            assert_eq!(counter(&out, "digest.invalid.unmarked"), 0);

            // Kind 4 (`ClassPartialAmbiguity`) on an incomplete record has no
            // indeterminate cause.
            let s_decl = ast
                .decls
                .ids()
                .find(|&d| {
                    ast.decls.get(d).kind() == DeclKind::CXXRecord
                        && ast
                            .decls
                            .get(d)
                            .name()
                            .as_ident()
                            .is_some_and(|sym| ast.symbols().as_str(sym) == "S")
                })
                .expect("struct S");
            ast.decls.set_flags(s_decl, DeclFlags::INVALID);
            ast.decls.set_subst_failed(s_decl, SubstFailedKind::ClassPartialAmbiguity);
            assert_eq!(super::invalid_cause(&ast, s_decl), None);
        })
        .expect("parse");
    }

    // WP-136 item 3 (`spec/digest.md` §10): a `Ref` to a failed substitution
    // (kinds 1, 2, 3, 5, or a closure owned by one) produces
    // `Unaccounted("ref/failed-substitution")` and does not queue the entity;
    // a `Ref` to kind 4 (`ClassPartialAmbiguity`) is determinate.
    #[test]
    fn wp136_ref_to_failed_substitution_is_unaccounted() {
        use slip_kiln::kiln::ast::decl::SubstFailedKind;

        let cases = [
            (
                "kind1",
                "template <class T> auto g(T t) -> decltype(t.size());\n\
                 int g(...);\n\
                 int u() { return g(1); }\n",
                SubstFailedKind::FunctionSignature,
            ),
            (
                "kind2",
                "struct S { template <class T> explicit(T::value) S(T) {} S(...) {} };\n\
                 S s = 1;\n",
                SubstFailedKind::ExplicitSpecifier,
            ),
            (
                "kind3",
                "template <class T, class U> constexpr int v = 0;\n\
                 template <class T> constexpr int v<T, int> = 1;\n\
                 template <class U> constexpr int v<int, U> = 2;\n\
                 template <class T> auto f(T) -> decltype(v<T, int>);\n\
                 long f(...);\n\
                 long x = f(1);\n",
                SubstFailedKind::VariablePartialAmbiguity,
            ),
            (
                "kind5",
                "template <class L> constexpr bool callable = requires(L l) { l(1); };\n\
                 void g() {\n\
                   auto l = [](auto t) -> decltype(({ int x = t.foo(); x; })) { return 0; };\n\
                   static_assert(!callable<decltype(l)>);\n\
                 }\n",
                SubstFailedKind::LocalOfFailedSignature,
            ),
        ];
        for (label, src, kind) in cases {
            with_walk(src, |w| {
                let target = w
                    .ast
                    .decls
                    .subst_failed_decls()
                    .into_iter()
                    .find(|&(_, k)| k == kind)
                    .map(|(d, _)| d)
                    .expect(label);
                let mut s = crate::stream::Stream::new();
                w.write_ref(&mut s, target);
                assert_eq!(
                    s.cause(),
                    Some(crate::Cause::Unaccounted("ref/failed-substitution")),
                    "{label}"
                );
                assert!(!w.queued.contains(&w.entity(target)), "{label}: must not queue");
            });
        }

        // Valid closure owned by a failed signature.
        let closure_src = "template <class T> auto f(T t) -> decltype([]{ return 1; }(), t.size());\n\
                           int f(...);\n\
                           int u() { return f(1); }\n";
        with_walk(closure_src, |w| {
            let closure = w
                .ast
                .decls
                .ids()
                .find(|&d| {
                    w.ast.decls.record(d).is_some_and(|r| r.lambda.is_some())
                        && w.ast.closure_owner(d).is_some_and(|o| {
                            w.ast.decls.subst_failed(o) == Some(SubstFailedKind::FunctionSignature)
                        })
                })
                .expect("closure owned by failed signature");
            let mut s = crate::stream::Stream::new();
            w.write_ref(&mut s, closure);
            assert_eq!(s.cause(), Some(crate::Cause::Unaccounted("ref/failed-substitution")));
        });

        // Kind 4 (`ClassPartialAmbiguity`): `write_ref` is determinate and
        // does not queue a unit.
        let kind4_src = "template <class T, class U> struct X {};\n\
                         template <class T> struct X<T, int> { using t = int; };\n\
                         template <class U> struct X<int, U> { using t = int; };\n\
                         template <class T> auto f(T) -> typename X<T, int>::t;\n\
                         long f(...);\n\
                         long y = f(1);\n\
                         X<int, int>* p = nullptr;\n";
        with_walk(kind4_src, |w| {
            let target = w
                .ast
                .decls
                .subst_failed_decls()
                .into_iter()
                .find(|&(_, k)| k == SubstFailedKind::ClassPartialAmbiguity)
                .map(|(d, _)| d)
                .expect("kind 4");
            let mut s = crate::stream::Stream::new();
            w.write_ref(&mut s, target);
            assert_eq!(s.cause(), None);
            assert!(!w.queued.contains(&w.entity(target)));
        });
    }

    // WP-136 item 4 ("Its kin"): check what Kiln keeps and what the digest
    // produces for all seven kin shapes.
    type KinCase = (
        &'static str,
        &'static str,
        &'static [(DeclKind, slip_kiln::kiln::ast::decl::SubstFailedKind)],
    );

    const WP136_KIN_CASES: &[KinCase] = {
        use slip_kiln::kiln::ast::decl::SubstFailedKind;
        &[
            (
                "member-function-template-of-instantiation",
                "template <class T> struct S {\n\
                   template <class U> auto m(U u) -> decltype(u.size());\n\
                   int m(...);\n\
                 };\n\
                 int u() { S<int> s; return s.m(1); }\n",
                &[(DeclKind::CXXMethod, SubstFailedKind::FunctionSignature)],
            ),
            (
                "constructor-template",
                "struct R {\n\
                   template <class F> R(F, decltype(F::tag) * = nullptr);\n\
                   R(int);\n\
                 };\n\
                 int u() { R r(1); (void)r; return 0; }\n",
                &[(DeclKind::CXXConstructor, SubstFailedKind::FunctionSignature)],
            ),
            (
                "conversion-function-template-explicit",
                "struct D {\n\
                   template <class T> explicit(T::value) operator T() const;\n\
                   operator int() const;\n\
                 };\n\
                 int u() { D d; return d; }\n",
                &[(DeclKind::CXXConversion, SubstFailedKind::ExplicitSpecifier)],
            ),
            (
                "conversion-function-template-default-targ",
                "struct C {\n\
                   template <class T, class = typename T::type> operator T() const;\n\
                   operator int() const;\n\
                 };\n\
                 int u() { C c; return c; }\n",
                &[],
            ),
            (
                "deduction-guide",
                "template <class T> struct A { A(T); };\n\
                 template <class T> A(T) -> A<typename T::type>;\n\
                 A a(1);\n",
                &[(DeclKind::CXXDeductionGuide, SubstFailedKind::FunctionSignature)],
            ),
            (
                "class-template-partial-spec-no-match",
                "template <class T, class = void> struct P { static constexpr int v = 1; };\n\
                 template <class T> struct P<T, typename T::type> { static constexpr int v = 2; };\n\
                 int u() { return P<int>::v; }\n",
                &[],
            ),
            (
                "variable-template-partial-ambiguity",
                "template <class T, class U> constexpr int v = 0;\n\
                 template <class T> constexpr int v<T, int> = 1;\n\
                 template <class U> constexpr int v<int, U> = 2;\n\
                 template <class T> auto f(T) -> decltype(v<T, int>);\n\
                 long f(...);\n\
                 long x = f(1);\n",
                &[
                    (DeclKind::Function, SubstFailedKind::FunctionSignature),
                    (DeclKind::Var, SubstFailedKind::VariablePartialAmbiguity),
                ],
            ),
            (
                "alias-template-enable-if-in-return",
                "template <bool, class T = void> struct enable_if {};\n\
                 template <class T> struct enable_if<true, T> { using type = T; };\n\
                 template <bool B, class T = void> using enable_if_t = typename enable_if<B, T>::type;\n\
                 template <class T> enable_if_t<sizeof(T) == 100, int> f(T);\n\
                 int f(...);\n\
                 int u() { return f(1); }\n",
                &[(DeclKind::Function, SubstFailedKind::FunctionSignature)],
            ),
            (
                "alias-template-enable-if-in-default-targ",
                "template <bool, class T = void> struct enable_if {};\n\
                 template <class T> struct enable_if<true, T> { using type = T; };\n\
                 template <bool B, class T = void> using enable_if_t = typename enable_if<B, T>::type;\n\
                 template <class T, enable_if_t<sizeof(T) == 100, int> = 0> int f(T);\n\
                 int f(...);\n\
                 int u() { return f(1); }\n",
                &[],
            ),
        ]
    };

    #[test]
    fn wp136_its_kin_all_seven_cases() {
        for &(label, src, want_marks) in WP136_KIN_CASES {
            with_walk(src, |w| {
                let mut got: Vec<_> = w
                    .ast
                    .decls
                    .subst_failed_decls()
                    .into_iter()
                    .map(|(d, k)| (w.ast.decls.get(d).kind(), k))
                    .collect();
                got.sort_by_key(|&(_, k)| k.value());
                assert_eq!(got.as_slice(), want_marks, "{label}");
            });
            let (out, errors) = digest_src(src);
            assert_eq!(errors, 0, "{label}");
            assert!(out.causes.is_empty(), "{label}: {:?}", out.causes);
            assert_eq!(counter(&out, "digest.invalid.unmarked"), 0, "{label}");
            assert_eq!(counter(&out, "digest.instantiated.unheld"), 0, "{label}");
            assert_eq!(counter(&out, "digest.emitted.unplaced"), 0, "{label}");
        }
    }

    // WP-136 fix round 1 (M8): `unit_class` and `query::Keys::unit_of` skip
    // failed substitutions independently of `units::collect`.
    #[test]
    fn wp136_unit_class_and_query_skip_failed_substitution() {
        use slip_kiln::kiln::ast::decl::SubstFailedKind;

        let src = "template <class T> auto g(T t) -> decltype(t.size());\n\
                   int g(...);\n\
                   int u() { return g(1); }\n";
        with_walk(src, |w| {
            let failed = w
                .ast
                .decls
                .subst_failed_decls()
                .into_iter()
                .find(|&(_, k)| k == SubstFailedKind::FunctionSignature)
                .map(|(d, _)| d)
                .expect("failed g<int>");
            assert_eq!(unit_class(w, w.entity(failed)), None);
        });
        let files = [("main.cpp", src)];
        let snippet = slip_kiln::snippet::Snippet {
            files: &files,
            main: "main.cpp",
            include: &[""],
            std: "c++20",
        };
        slip_kiln::snippet::parse(&snippet, |p| {
            assert_eq!(p.errors, 0, "{}", p.diagnostics);
            let path = |f| p.path(f).map(|s: &str| s.as_bytes().to_vec()).unwrap_or_default();
            let input = crate::Input { ast: p.ast, sm: p.sm, subject_path: &path };
            let mut keys = crate::query::Keys::new(&input);
            let failed = p
                .ast
                .decls
                .subst_failed_decls()
                .into_iter()
                .find(|&(_, k)| k == SubstFailedKind::FunctionSignature)
                .map(|(d, _)| d)
                .expect("failed g<int>");
            assert_eq!(keys.unit_of(failed), None);
        })
        .expect("parse");
    }

    // WP-136 fix round 1 (item 5): `failed_substitution` has an O(1) empty
    // fast-path when `ast.decls.cold.subst_failed` is empty, and fails closed
    // (`FailedSubst::DepthLimit` -> `Unaccounted("decl/subst-failed-depth")`)
    // past its step bound rather than silently returning `false`.
    #[test]
    fn wp136_subst_failed_depth_limit_fails_closed_and_empty_fast_path() {
        let digest_with_limit = |src: &str, limit: usize| -> crate::Output {
            let files = [("main.cpp", src)];
            let snippet = slip_kiln::snippet::Snippet {
                files: &files,
                main: "main.cpp",
                include: &[""],
                std: "c++20",
            };
            let rules = RuleTable::empty();
            slip_kiln::snippet::parse(&snippet, |p| {
                assert_eq!(p.errors, 0, "{}", p.diagnostics);
                let paths: Vec<Vec<u8>> =
                    p.sm.file_ids()
                        .map(|f| p.path(f).map(|s: &str| s.as_bytes().to_vec()).unwrap_or_default())
                        .collect();
                let (ast, sm) = (p.ast, p.sm);
                std::thread::scope(|s| {
                    std::thread::Builder::new()
                        .stack_size(crate::STACK_SIZE)
                        .spawn_scoped(s, || {
                            with_subst_failed_limit(limit, || {
                                let path = |f: slip_kiln::kiln::source::FileId| {
                                    paths.get(f.index()).cloned().unwrap_or_default()
                                };
                                let input = crate::Input { ast, sm, subject_path: &path };
                                let options = crate::Options {
                                    rules: &rules,
                                    read_only: crate::ReadOnly::NO_FILE,
                                    existing: &[],
                                    existing_local: &[],
                                    detail: crate::Detail::All,
                                    names: true,
                                    rule_use: false,
                                };
                                crate::digest(&input, &options)
                            })
                        })
                        .expect("spawn")
                        .join()
                        .expect("join")
                })
            })
            .expect("parse")
        };

        // 1. Empty fast-path: no failed substitutions in the TU -> even with
        // limit=2, a 4-deep declaration (`a::b::c::x`) is determinate.
        let clean = "namespace a { namespace b { namespace c { int x = 1; } } }\n";
        let clean_out = digest_with_limit(clean, 2);
        assert!(clean_out.causes.is_empty(), "{:?}", clean_out.causes);

        // 2. Non-empty `subst_failed` with lowered limit=2: `a::b::c::x`
        // exceeds the bound and fails closed with `decl/subst-failed-depth`,
        // staying a unit rather than being silently dropped or treated as clean.
        let with_sf = "namespace a { namespace b { namespace c { int x = 1; } } }\n\
                       template <class T> auto g(T t) -> decltype(t.size());\n\
                       int g(...);\n\
                       int u() { return g(1); }\n";
        let normal_out = digest_with_limit(with_sf, super::SUBST_FAILED_LIMIT);
        assert!(normal_out.causes.is_empty(), "{:?}", normal_out.causes);

        let limited_out = digest_with_limit(with_sf, 2);
        let x_key = limited_out
            .names
            .iter()
            .find(|(_, n)| n == "a::b::c::x")
            .map(|&(k, _)| k)
            .expect("a::b::c::x must remain a unit, not be silently dropped");
        assert!(
            limited_out
                .causes
                .contains(&(x_key, crate::Cause::Unaccounted("decl/subst-failed-depth"))),
            "expected decl/subst-failed-depth on a::b::c::x, got: {:?}",
            limited_out.causes
        );

        // 3. `write_ref` to a declaration past the bound also fails closed.
        with_walk(with_sf, |w| {
            let x = named(w, "x", DeclKind::Var)[0];
            with_subst_failed_limit(2, || {
                assert_eq!(super::failed_substitution(w.ast, x), super::FailedSubst::DepthLimit);
                let mut s = crate::stream::Stream::new();
                w.write_ref(&mut s, x);
                assert_eq!(s.cause(), Some(crate::Cause::Unaccounted("decl/subst-failed-depth")));
            });
        });
    }
}
