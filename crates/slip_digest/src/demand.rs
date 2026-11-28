//! Demand-driven fields and resolved units (`spec/digest-kinds.md` §11):
//! the deferrable class as far as stored facts decide it, and the
//! resolved units of §11.1 (`spec/digest.md` §2.2).
//!
//! # Resolved units
//!
//! A resolved unit is a fact Kiln computes for an entity only when
//! something needs it. It is a unit of its own, of the derived class, so
//! that a unit both parses have does not depend on whether the fact was
//! computed (digest §1 rule 5). [`resolved_units`] is called by the walk
//! for every unit it has just written, after that unit; it has one arm
//! per [`Property`], and each arm decides whether its fact exists for the
//! entity and, if so, writes it with [`write_resolved`], the one function
//! that makes a resolved unit:
//!
//! - its key: kind `resolved`, parent the owner's mapped key, name
//!   `property(n)`, and a discriminator when the property has one (the
//!   parameter's index of `default_immediates`, as `uv`);
//! - its stream: what the arm writes, with the unit state of its own (a
//!   fresh one, as a unit has: local ordinals start at 0);
//! - its class: derived; its file: the owner's origin file, as a derived
//!   unit's is (`units::origin_file`);
//! - its cause: the stream's, merged with the cause the owner's key
//!   carries (its key's parent), as `units::write` merges a unit's.
//!
//! `Written.entity` of a resolved unit is the owner's entity and
//! `Written.resolved` names the property: `names.rs` writes the owner's
//! name followed by `::<property>`, and `units::apply_taints` makes the
//! first unit of an entity indeterminate, which is the owner (the walk
//! appends resolved units after it).

use std::collections::HashMap;

use slip_kiln::kiln::ast::decl::{DeclFlags, DeclKind, Defaulted, VarEvalState};
use slip_kiln::kiln::ast::templates::SpecKind;
use slip_kiln::kiln::ast::types::{ExcSpec, NoexceptValue, TypeView};
use slip_kiln::kiln::ast::{ConstValueId, DeclId, ExprId, QualType};
use slip_model::digests::GroupClass;
use slip_model::keys::{DISC_TAG, Key, KeyKind, Name, Parent};

use crate::Cause;
use crate::keys::KeyRes;
use crate::stream::{Stream, UNIT_TAG};
use crate::tables::property;
use crate::units::{Written, merge};
use crate::walk::Walk;

/// The properties of digest-kinds §11.1, by their numbers. The numbers
/// are Slip's own and never reused (3 is retired).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Property {
    ExceptionSpec,
    Value,
    KeyFunction,
    DtorDelete,
    InheritedConstructor,
    DefaultImmediates,
    Definition,
}

impl Property {
    /// Every property, in the order of their numbers: the order
    /// [`resolved_units`] tries them in (not behaviour: groups sort).
    pub(crate) const ALL: [Property; 7] = [
        Property::ExceptionSpec,
        Property::Value,
        Property::KeyFunction,
        Property::DtorDelete,
        Property::InheritedConstructor,
        Property::DefaultImmediates,
        Property::Definition,
    ];

    /// Its number in §11.1, the `uv` of the name `property`.
    pub(crate) fn number(self) -> u32 {
        match self {
            Property::ExceptionSpec => property::EXCEPTION_SPEC,
            Property::Value => property::VALUE,
            Property::KeyFunction => property::KEY_FUNCTION,
            Property::DtorDelete => property::DTOR_DELETE,
            Property::InheritedConstructor => property::INHERITED_CONSTRUCTOR,
            Property::DefaultImmediates => property::DEFAULT_IMMEDIATES,
            Property::Definition => property::DEFINITION,
        }
    }

    /// Its name in §11.1, for `Output::names`.
    pub(crate) fn word(self) -> &'static str {
        match self {
            Property::ExceptionSpec => "exception_spec",
            Property::Value => "value",
            Property::KeyFunction => "key_function",
            Property::DtorDelete => "dtor_delete",
            Property::InheritedConstructor => "inherited_constructor",
            Property::DefaultImmediates => "default_immediates",
            Property::Definition => "definition",
        }
    }
}

/// What makes a written unit a resolved one (`Written::resolved`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Resolved {
    pub property: Property,
    /// The discriminator, a `uv` (the parameter's index of
    /// `default_immediates`); `None` for a property without one.
    pub disc: Option<u64>,
}

/// The resolved units of the unit `owner` just written for entity `e`
/// (a first declaration), appended to `out`: one arm per property, each
/// of which writes its unit with [`write_resolved`] when the fact exists.
pub(crate) fn resolved_units(w: &mut Walk<'_>, e: DeclId, owner: &Written, out: &mut Vec<Written>) {
    // Resolved units belong to entities' units only (not to the order
    // unit, nor to another resolved or `default_argument` unit: the arm
    // that writes a `default_argument` unit writes its resolved units).
    if owner.entity != Some(e) || owner.resolved.is_some() || owner.default_argument.is_some() {
        return;
    }
    for p in Property::ALL {
        match p {
            Property::ExceptionSpec => exception_spec(w, e, owner, out),
            Property::Value => value(w, e, owner, out),
            Property::KeyFunction => key_function(w, e, owner, out),
            Property::DtorDelete => dtor_delete(w, e, owner, out),
            Property::InheritedConstructor => inherited_constructor(w, e, owner, out),
            Property::DefaultImmediates => default_immediates(w, e, owner, out),
            Property::Definition => definition(w, e, owner, out),
        }
    }
    default_arguments(w, e, owner, out);
}

/// Writes one resolved unit of `owner` (the unit of entity `e`): the
/// property `p`, with the discriminator `disc`, whose stream `body`
/// writes. See the module documentation for its key, class, file and
/// cause.
pub(crate) fn write_resolved(
    w: &mut Walk<'_>,
    e: DeclId,
    owner: &Written,
    p: Property,
    disc: Option<u64>,
    body: impl FnOnce(&mut Walk<'_>, &mut Stream),
) -> Written {
    let name = Name::Property(u64::from(p.number()));
    let mut u = write_part(w, e, owner, KeyKind::Resolved, name, disc, body);
    u.resolved = Some(Resolved { property: p, disc });
    u
}

/// Writes the `default_argument` unit (digest-kinds §11.4) of parameter
/// `index` (0 for a field) of `owner`, the unit of function or field `e`:
/// key kind `default_argument`, parent the owner's key, name
/// `parameter(index)`, no discriminator; otherwise as a resolved unit
/// (derived, the owner's origin file, the owner's key cause merged in).
pub(crate) fn write_default_argument(
    w: &mut Walk<'_>,
    e: DeclId,
    owner: &Written,
    index: u64,
    body: impl FnOnce(&mut Walk<'_>, &mut Stream),
) -> Written {
    let name = Name::Parameter(index);
    let mut u = write_part(w, e, owner, KeyKind::DefaultArgument, name, None, body);
    u.default_argument = Some(index);
    u
}

/// A derived unit keyed under `owner` (a resolved or `default_argument`
/// unit): key `kind` / `Parent::Unit(owner.key)` / `name` / the `uv`
/// discriminator `disc`, whose stream `body` writes with a fresh unit
/// state.
fn write_part(
    w: &mut Walk<'_>,
    e: DeclId,
    owner: &Written,
    kind: KeyKind,
    name: Name,
    disc: Option<u64>,
    body: impl FnOnce(&mut Walk<'_>, &mut Stream),
) -> Written {
    let saved = w.take_unit();
    let mut stream = Stream::new();
    body(w, &mut stream);
    let emits = std::mem::take(&mut w.unit.emits);
    w.unit = saved;
    // Rule 1 of the mark "emitted unreferenced" (`emitted.rs`, WP-155).
    let emitted = crate::emitted::written(w, e, emits);
    let finished = stream.finish(UNIT_TAG);
    let h = w.hashed(finished);
    let disc_digest = disc.map(|n| {
        let d = disc_hashed(n);
        w.hashed(d).digest
    });
    let key = Key { kind, parent: Parent::Unit(owner.key), name, disc: disc_digest };
    let key = w.rules.map(key);
    w.bytes += key.to_bytes().len() as u64;
    // The cause the owner's key carries, which its parent passes on.
    let key_cause = match w.key(e) {
        KeyRes::Key { cause, .. } => cause,
        KeyRes::None(cause) => Some(cause.unwrap_or(Cause::Unaccounted("key/none"))),
    };
    let mut cause = h.cause;
    if let Some(c) = key_cause {
        cause = Some(merge(cause, c));
    }
    let file = match owner.class {
        // A namespace's unit is of the `tu` group: no origin file.
        GroupClass::Tu => Vec::new(),
        GroupClass::Declared | GroupClass::Derived => {
            crate::units::origin_file(w, owner.read.unwrap_or(e))
        }
    };
    Written {
        entity: Some(e),
        read: owner.read,
        key: key.id(),
        kind,
        digest: h.digest,
        cause,
        class: GroupClass::Derived,
        file,
        resolved: None,
        default_argument: owner.default_argument,
        owner: Some(owner.key),
        collided: false,
        view: None,
        denotes: None,
        emitted,
    }
}

/// The discriminator stream of a part unit: one `uv`.
fn disc_hashed(n: u64) -> crate::stream::Hashed {
    let mut ds = Stream::new();
    ds.uv(n);
    ds.finish(DISC_TAG)
}

/// The mapped `Key` of the written part unit `u` (a resolved or
/// `default_argument` unit), computed again from `entity`, the mapped key
/// of its entity, as [`write_part`] builds it: the `default_argument` unit
/// first if `u` is one or is under one, then the resolved unit. `None` for
/// a unit that is neither (its key is `entity`). For unit records
/// (`records.rs`); hashes into no unit.
pub(crate) fn part_key(w: &Walk<'_>, entity: Key, u: &Written) -> Option<Key> {
    if u.default_argument.is_none() && u.resolved.is_none() {
        return None;
    }
    let mut key = entity;
    if let Some(i) = u.default_argument {
        let k = Key {
            kind: KeyKind::DefaultArgument,
            parent: Parent::Unit(key.id()),
            name: Name::Parameter(i),
            disc: None,
        };
        key = w.rules.map(k);
    }
    if let Some(r) = u.resolved {
        let k = Key {
            kind: KeyKind::Resolved,
            parent: Parent::Unit(key.id()),
            name: Name::Property(u64::from(r.property.number())),
            disc: r.disc.map(|n| disc_hashed(n).digest),
        };
        key = w.rules.map(k);
    }
    Some(key)
}

/// Property 4, `key_function` (§11.1): a class's key function, `ref`,
/// present if Kiln computed it. Kiln keys it by the class's definition
/// (`kiln_sema/src/class/virtual/complete.rs:366-373`), and computes it
/// when the class's virtual contexts are acted on.
fn key_function(w: &mut Walk<'_>, e: DeclId, owner: &Written, out: &mut Vec<Written>) {
    let ast = w.ast;
    if ast.decls.record(e).is_none() {
        return;
    }
    let Some(def) = ast.definition(w.representative(e)) else { return };
    let Some(&k) = ast.decls.cold.key_function.get(def) else { return };
    let u = write_resolved(w, e, owner, Property::KeyFunction, None, |w, s| w.write_ref(s, k));
    out.push(u);
}

/// Property 8, `definition` (§11.9): the rows of the definition of an
/// entity of the definition class, or of a definition an instantiation
/// added outside its class for an entity with a written declaration,
/// present when Kiln holds the definition ([`defining_declaration`]).
fn definition(w: &mut Walk<'_>, e: DeclId, owner: &Written, out: &mut Vec<Written>) {
    let Some(def) = defining_declaration(w, e) else { return };
    let u = write_resolved(w, e, owner, Property::Definition, None, |w, s| {
        definition_stream(w, s, e, def);
    });
    out.push(u);
}

/// What the numbering of owner `e` walks after `e`'s own stream
/// (`local::compute`, digest §4.3 "Numbering an owner"), into the sink
/// `s`: the streams of the units of `e` that hold what its own stream
/// leaves out, each with a fresh unit state, as they are written. The
/// resolved unit `definition` (§11.9), which holds the body or
/// initialiser of an entity of the definition class: what the own stream
/// declared is met again there first, in the same order (the
/// parameters), and recorded once. Then, for a field an instantiation
/// made, its `default_argument` unit (§11.4), whose initialiser the own
/// stream writes empty.
pub(crate) fn number_parts(w: &mut Walk<'_>, s: &mut Stream, e: DeclId) {
    let ast = w.ast;
    if let Some(def) = defining_declaration(w, e) {
        drop(w.take_unit());
        definition_stream(w, s, e, def);
    }
    let d = crate::units::read_decl(w, e);
    if let Some(f) = ast.decls.field(d)
        && let Some(x) = f.init
        && crate::decl_class::instantiated_field(w, d)
    {
        drop(w.take_unit());
        w.unit.local.holder = Some(e);
        crate::expr::child(w, s, x);
    }
}

/// The stream of the resolved unit `definition` of entity `e`, read from
/// its defining declaration `def` (`defining_declaration`), into `s`,
/// whose unit state is fresh: what `definition` writes, and what the
/// numbering of `e` walks after `e`'s own stream (`local::compute`), since
/// the local types and objects of a body `definition` holds are `e`'s
/// (digest §4.3 "Numbering an owner").
pub(crate) fn definition_stream(w: &mut Walk<'_>, s: &mut Stream, e: DeclId, def: DeclId) {
    let ast = w.ast;
    // Every declaration of the entity is read, an out-of-line one an
    // instantiation made included (`Walk::redecls`).
    w.unit.definition = true;
    // The stream is `e`'s: its local objects are referred to as its own,
    // and the names around a unit under a local type are written
    // (`local::outer`, `local::write_scope_names`).
    w.unit.local.holder = Some(e);
    crate::templ::spec_definition(w, s, e);
    if ast.decls.function(def).is_some() {
        crate::decl_func::function_definition(w, s, e, def);
    } else if ast.decls.var(def).is_some() {
        crate::decl_func::var_definition(w, s, e, def);
    } else if ast.decls.record(def).is_some() {
        crate::decl_class::record_definition(w, s, def);
        crate::attrs::definition_attrs(w, s, e);
    } else if ast.decls.enumeration(def).is_some() {
        crate::decl_class::enum_definition(w, s, def);
    }
}

/// The declaration the resolved unit `definition` of entity `e` is read
/// from, if it exists (§11.9): what Kiln holds at the end of the
/// translation unit.
///
/// - For an entity of the definition class: a function's `Ast::definition`
///   when it has a body (a function deleted on its first declaration is
///   not defined by that: `= delete` is a fact of the declaration); a
///   variable's first declaration, over every declaration Kiln chained,
///   that is a definition (`units::is_var_definition`), else the one that
///   holds an initialiser (an in-class `static const int x = 1;` whose
///   initialiser an instantiation substituted); a class's or an
///   enumeration's `Ast::definition`. Any other entity has none.
/// - For any other function or variable: the definition an instantiation
///   added outside its class (`units::instantiated_outside_class`), a
///   friend defined in a class template, read from `Ast::definition`
///   (WP-032 rule 4).
pub(crate) fn defining_declaration(w: &Walk<'_>, e: DeclId) -> Option<DeclId> {
    let ast = w.ast;
    let rep = w.representative(e);
    let with_body =
        |d: DeclId| ast.decls.function(d).is_some_and(|f| f.body.is_some()).then_some(d);
    if !definition_class(w, e) {
        ast.decls.function(rep)?;
        return ast
            .definition(rep)
            .filter(|&d| crate::units::instantiated_outside_class(w, d))
            .and_then(with_body);
    }
    if ast.decls.function(rep).is_some() {
        return ast.definition(rep).and_then(with_body);
    }
    if ast.decls.var(rep).is_some() {
        let mut chain: Vec<DeclId> = w
            .chain(e)
            .into_iter()
            .filter(|&d| !w.is_local_extern(d) && !crate::units::in_templated_body(w, d))
            .collect();
        chain.reverse();
        let has_init = |d: DeclId| ast.decls.var(d).is_some_and(|v| v.init.is_some());
        // A variable whose pattern has an initialiser is defined once Kiln
        // has instantiated the initialiser, not before: Kiln makes the
        // declaration of a specialisation or of an inline static member
        // before its initialiser (`kiln_sema/src/templ/var.rs:229`,
        // `templ/class.rs:2061`).
        if pattern_has_init(w, e) && !chain.iter().copied().any(has_init) {
            return None;
        }
        return chain
            .iter()
            .copied()
            .find(|&d| crate::units::is_var_definition(w, d))
            .or_else(|| chain.iter().copied().find(|&d| has_init(d)));
    }
    if ast.decls.record(rep).is_some() || ast.decls.enumeration(rep).is_some() {
        return ast.definition(rep);
    }
    None
}

/// Whether the declared type of the pattern that entity `e` is
/// instantiated from holds a placeholder (`auto`, `decltype(auto)`, a
/// class template to deduce): its return type for a function, its type
/// for a variable, on any declaration of the pattern. The type of the
/// instantiation is then deduced with its definition, and is the
/// resolved unit `definition`'s (§11.9). `false` for an entity that is
/// not instantiated from a pattern.
pub(crate) fn pattern_holds_placeholder(w: &Walk<'_>, e: DeclId) -> bool {
    let ast = w.ast;
    let Some(p) = instantiated_pattern(w, e) else { return false };
    ast.redecls_of(p).any(|r| ast.decls.get(r).ty().is_some_and(|q| holds_placeholder(w, q)))
}

/// The pattern entity `e` is instantiated from: for a variable
/// specialisation, the declaration `SpecInfo` `from` names (the primary's
/// or a partial specialisation's declaration that holds the initialiser,
/// else its definition: Kiln sets it when it declares the specialisation,
/// `kiln_sema/src/templ/var.rs:109-161`, and moves it later only for an
/// explicit instantiation, `templ/var.rs:289-298`); for another
/// specialisation, the pattern of its template (not what `from` names: a
/// function's is moved to a friend's definition when the definition is
/// instantiated, `templ/class.rs:1578`, a class's is set at its
/// definition); for a member of a specialisation, the member it was
/// instantiated from.
fn instantiated_pattern(w: &Walk<'_>, e: DeclId) -> Option<DeclId> {
    let ast = w.ast;
    let e = w.entity(e);
    match ast.spec_info(e) {
        Some((_, i)) if ast.decls.var(e).is_some() => Some(i.from),
        Some((_, i)) => ast.template_info(i.template).and_then(|t| t.pattern),
        None => ast.member_spec_info(e).map(|(_, i)| i.instantiated_from),
    }
}

/// Whether a declaration of the pattern of variable `e` (`instantiated_pattern`)
/// holds an initialiser. Without one, the initialisation Kiln adds when
/// it instantiates the definition is default-initialisation, which the
/// variable's type and the units of its class determine, and which
/// `definition` does not write (§11.9).
pub(crate) fn pattern_has_init(w: &Walk<'_>, e: DeclId) -> bool {
    let ast = w.ast;
    instantiated_pattern(w, e).is_some_and(|p| {
        ast.redecls_of(p).any(|d| ast.decls.var(d).is_some_and(|v| v.init.is_some()))
    })
}

/// Whether type `q` holds a placeholder, walked as Kiln's
/// `Sema::contains_placeholder` walks it
/// (`kiln_sema/src/templ/auto.rs:588-619`: through pointers, references,
/// member pointers, a function's return type and arrays' elements),
/// looking through sugar at each step.
fn holds_placeholder(w: &Walk<'_>, q: QualType) -> bool {
    let types = &w.ast.types;
    let mut cur = q;
    for _ in 0..PLACEHOLDER_LIMIT {
        let mut c = cur;
        for _ in 0..PLACEHOLDER_LIMIT {
            match types.view(c.ty()) {
                TypeView::Auto { .. } | TypeView::DeducedTemplateSpecialization { .. } => {
                    return true;
                }
                _ => {}
            }
            let next = types.desugar_once(c);
            if next == c {
                break;
            }
            c = next;
        }
        cur = match types.view(c.ty()) {
            TypeView::Pointer(p) | TypeView::LValueRef(p) | TypeView::RValueRef(p) => p,
            TypeView::MemberPointer { pointee, .. } => pointee,
            TypeView::Function(f) => f.ret(),
            TypeView::ConstantArray(a) => a.elem,
            TypeView::IncompleteArray(x)
            | TypeView::DependentSizedArray { elem: x, .. }
            | TypeView::VariableArray { elem: x, .. } => x,
            _ => return false,
        };
    }
    false
}

/// Steps [`holds_placeholder`] takes at most, as Kiln's walk.
const PLACEHOLDER_LIMIT: usize = 256;

/// Contexts deeper than this are not climbed by [`effective_kind`].
const KIND_LIMIT: usize = 256;

/// The rank of a kind in the order of decision 1 of
/// `docs/wp/WP-045.md`: `ImplicitInstantiation` <
/// `ExplicitInstantiationDeclaration` < `ExplicitInstantiationDefinition`,
/// with `Undeclared` and no kind least. `ExplicitSpecialization` is never
/// compared (it decides before any comparison).
fn rank(k: Option<SpecKind>) -> u8 {
    match k {
        None | Some(SpecKind::Undeclared) => 0,
        Some(SpecKind::ImplicitInstantiation) => 1,
        Some(SpecKind::ExplicitInstantiationDeclaration) => 2,
        Some(SpecKind::ExplicitInstantiationDefinition | SpecKind::ExplicitSpecialization) => 3,
    }
}

/// The *effective kind* of entity `e` (`spec/digest-kinds.md` §11.10):
/// the one specialisation kind the digest reads for it, from facts no use
/// moves.
///
/// - An entity with its own `SpecInfo` (a specialisation of a template)
///   has that kind.
/// - A member whose `MemberSpecInfo` kind is `ExplicitSpecialization` has
///   that kind, and so has a member template of an implicit instantiation
///   that `template <> template <...>` specialises (Kiln gives it no
///   `SpecInfo`: `units::specialised_member_template`).
/// - Any other member of a class or enumeration that has an effective
///   kind of an instantiation has the greater of its own `MemberSpecInfo`
///   kind (none counts as least) and that of its class: Kiln leaves a
///   member class defined before an explicit instantiation of its class
///   `ImplicitInstantiation` (`kiln_sema/src/templ/spec.rs:1366-1398`), a
///   member function too, and gives a field none.
/// - A member of an explicit specialisation, or of a class that is not
///   made by instantiation, has none: it is written in source.
/// - A declaration that is no member (a friend an instantiation
///   declared) has none, whatever its `MemberSpecInfo` says: a friend's
///   class is decided by `units::outside_only`, and it can be written in
///   source as well.
/// - A local entity has none: anything declared in a function body is
///   written in full, in the stream of what holds the body.
///
/// `MemberSpecInfo` and `SpecInfo` are keyed by the entity's first
/// declaration.
pub(crate) fn effective_kind(w: &Walk<'_>, e: DeclId) -> Option<SpecKind> {
    let ast = w.ast;
    let e = w.entity(e);
    if w.is_local(e) {
        return None;
    }
    // The members' own kinds, innermost first, up to the context that
    // decides.
    let mut owns: Vec<Option<SpecKind>> = Vec::new();
    let mut x = e;
    let mut base = None;
    for _ in 0..KIND_LIMIT {
        if let Some((_, i)) = ast.spec_info(x) {
            base = Some(i.kind);
            break;
        }
        // A member template of an implicit instantiation that
        // `template <> template <...>` specialises is written in source,
        // and Kiln gives it no `SpecInfo` (`units::specialised_member_template`).
        if crate::units::specialised_member_template(w, x) {
            base = Some(SpecKind::ExplicitSpecialization);
            break;
        }
        let own = ast.member_spec_info(x).map(|(_, i)| i.kind);
        if own == Some(SpecKind::ExplicitSpecialization) {
            base = own;
            break;
        }
        let class = ast
            .decls
            .get(x)
            .ctx()
            .filter(|&c| ast.decls.record(c).is_some() || ast.decls.enumeration(c).is_some());
        match class {
            Some(c) => {
                owns.push(own);
                x = w.entity(c);
            }
            // No member (a friend an instantiation declared): its kind
            // is not the entity's, which `units::outside_only` decides.
            None => break,
        }
    }
    let mut kind = base;
    for own in owns.into_iter().rev() {
        kind = match kind {
            Some(
                c @ (SpecKind::ImplicitInstantiation
                | SpecKind::ExplicitInstantiationDeclaration
                | SpecKind::ExplicitInstantiationDefinition),
            ) => {
                if rank(own) > rank(Some(c)) {
                    own
                } else {
                    Some(c)
                }
            }
            Some(SpecKind::ExplicitSpecialization | SpecKind::Undeclared) | None => None,
        };
    }
    kind
}

/// Whether entity `e` is of the *definition class* (§11.9): Kiln makes
/// its definition on use, so the own unit writes the rows of the
/// definition in their empty form and the resolved unit `definition`
/// holds them.
///
/// - A specialisation whose `SpecInfo` kind is `ImplicitInstantiation` or
///   `ExplicitInstantiationDeclaration` (not `ExplicitInstantiationDefinition`:
///   its definition is made at the explicit instantiation, used or not).
/// - A member with an effective kind other than `ExplicitSpecialization`.
/// - A function or deduction guide every declaration of which is
///   implicit (a special member, a comparison, a deduction guide, a
///   closure's conversion function), or explicitly defaulted on its
///   first declaration.
/// - An entity every declaration of which an instantiation made outside
///   its class (`units::outside_only`).
pub(crate) fn definition_class(w: &Walk<'_>, e: DeclId) -> bool {
    let ast = w.ast;
    let e = w.entity(e);
    if let Some((_, i)) = ast.spec_info(e) {
        return matches!(
            i.kind,
            SpecKind::ImplicitInstantiation | SpecKind::ExplicitInstantiationDeclaration
        );
    }
    match effective_kind(w, e) {
        Some(
            SpecKind::ImplicitInstantiation
            | SpecKind::ExplicitInstantiationDeclaration
            | SpecKind::ExplicitInstantiationDefinition,
        ) => return true,
        Some(SpecKind::ExplicitSpecialization | SpecKind::Undeclared) | None => {}
    }
    let kind = ast.decls.get(e).kind();
    if crate::keys::info(kind).function || kind == DeclKind::CXXDeductionGuide {
        let all_implicit = w.redecls(e).iter().all(|&x| ast.decls.get(x).has(DeclFlags::IMPLICIT));
        let first = w.representative(e);
        let defaulted_first =
            ast.decls.function(first).is_some_and(|f| f.spec.defaulted() == Defaulted::Explicit);
        if all_implicit || defaulted_first {
            return true;
        }
    }
    crate::units::outside_only(w, e)
}

/// The declaration the unit of entity `e` was read from (`units::write`):
/// a resolved arm reads its facts where the owner's stream did.
fn read_of(w: &Walk<'_>, e: DeclId, owner: &Written) -> DeclId {
    if crate::units::outside_only(w, e) {
        owner.read.unwrap_or(e)
    } else {
        crate::units::read_decl(w, e)
    }
}

/// The function, variable or field whose facts the unit read from `d`
/// holds: `d` itself, or for a template its pattern, read from its
/// defining declaration as `templ::template_info` writes it.
fn subject(w: &Walk<'_>, d: DeclId) -> DeclId {
    match w.ast.template_info(d).and_then(|i| i.pattern) {
        Some(p) => crate::units::defining_decl(w, p).unwrap_or(p),
        None => d,
    }
}

/// Property 1, `exception_spec` (§11.2): the specification of a function
/// of the deferrable class, `enum ExcSpec` in normal form, present when
/// its type no longer holds `Deferred`.
fn exception_spec(w: &mut Walk<'_>, e: DeclId, owner: &Written, out: &mut Vec<Written>) {
    let ast = w.ast;
    let f = subject(w, read_of(w, e, owner));
    if ast.decls.function(f).is_none() || !deferrable(w, f) {
        return;
    }
    // The declaration's own specification, not the canonical prototype's
    // (the first spelling's operand, `Walk::own_proto`).
    let Some(proto) = w.own_proto(f) else {
        return;
    };
    let spec = ast.types.exc_spec(&proto);
    if matches!(spec, ExcSpec::Deferred(_)) {
        return;
    }
    let u = write_resolved(w, e, owner, Property::ExceptionSpec, None, |w, s| {
        w.write_exc_spec(s, spec, true);
    });
    out.push(u);
}

/// Property 2, `value` (§11.3): what the Oven found for a variable
/// (`VarData` `eval`, as `decl_func::var_eval` reads it), present when it
/// evaluated it; then one `value?` per temporary the variable extends, in
/// `mangling_index` order (`LetData` `value`).
fn value(w: &mut Walk<'_>, e: DeclId, owner: &Written, out: &mut Vec<Written>) {
    let ast = w.ast;
    let v = read_of(w, e, owner);
    if ast.decls.var(v).is_none() {
        return;
    }
    // The evaluation is read over every declaration of the variable, the
    // out-of-line definition an instantiation made for a static data
    // member included, as `definition` reads its initialiser: Kiln
    // records the evaluation on the declaration that holds the
    // initialiser (`decl_func::var_eval`), which the own unit's chain
    // leaves out (`Walk::redecls`).
    let in_definition = std::mem::replace(&mut w.unit.definition, true);
    let eval = crate::decl_func::var_eval(w, v);
    w.unit.definition = in_definition;
    let own = match eval.state() {
        VarEvalState::Value(c) => Some(c),
        VarEvalState::NotConstant => None,
        VarEvalState::NotEvaluated | VarEvalState::Evaluating => return,
    };
    let lets = extended_temporaries(w, ast.first(v));
    let u = write_resolved(w, e, owner, Property::Value, None, |w, s| {
        s.opt(own.is_some());
        if let Some(c) = own {
            crate::value::value(w, s, c);
        }
        for t in lets {
            let value = w.ast.decls.lifetime_extended(t).and_then(|l| l.value);
            s.opt(value.is_some());
            if let Some(c) = value {
                crate::value::value(w, s, c);
            }
        }
    });
    out.push(u);
}

/// The temporaries whose lifetime the variable whose first declaration is
/// `first` extends (`LetData` `extending`, any declaration of it), in
/// `mangling_index` order, then by declaration (two with one index are
/// not met in a valid parse). The index of all of them is built once.
fn extended_temporaries(w: &mut Walk<'_>, first: DeclId) -> Vec<DeclId> {
    let ast = w.ast;
    let index = w.lets.get_or_insert_with(|| {
        let mut m: HashMap<DeclId, Vec<(u32, DeclId)>> = HashMap::new();
        for d in ast.decls.ids() {
            if let Some(l) = ast.decls.lifetime_extended(d) {
                m.entry(ast.first(l.extending)).or_default().push((l.mangling_index, d));
            }
        }
        for v in m.values_mut() {
            v.sort_unstable();
        }
        m
    });
    index.get(&first).map(|v| v.iter().map(|&(_, d)| d).collect()).unwrap_or_default()
}

/// Property 5, `dtor_delete` (§11.1): a virtual destructor's deallocation
/// function and the `this` argument of a destroying one. Kiln keys both by
/// the destructor's first declaration
/// (`kiln_sema/src/new_delete.rs:2666-2721`).
fn dtor_delete(w: &mut Walk<'_>, e: DeclId, owner: &Written, out: &mut Vec<Written>) {
    let ast = w.ast;
    if ast.decls.get(e).kind() != DeclKind::CXXDestructor {
        return;
    }
    let first = ast.first(e);
    let Some(&op) = ast.decls.cold.dtor_delete.get(first) else {
        if ast.decls.cold.dtor_delete_this.contains(first) {
            let u = write_resolved(w, e, owner, Property::DtorDelete, None, |_, s| {
                s.indeterminate(Cause::Unaccounted("ColdDeclData/dtor_delete_this"));
            });
            out.push(u);
        }
        return;
    };
    let this = ast.decls.cold.dtor_delete_this.get(first).copied();
    let u = write_resolved(w, e, owner, Property::DtorDelete, None, |w, s| {
        w.write_ref(s, op);
        crate::expr::child_opt(w, s, this);
    });
    out.push(u);
}

/// Property 6, `inherited_constructor` (§11.1): an inheriting
/// constructor's using-shadow and the base constructor, which Kiln sets
/// together when it declares it (`kiln_sema/src/class/inhctor.rs:78-79`).
fn inherited_constructor(w: &mut Walk<'_>, e: DeclId, owner: &Written, out: &mut Vec<Written>) {
    let ast = w.ast;
    let first = ast.first(e);
    let inheriting = ast.decls.cold.inheriting_ctor.get(first).copied();
    let inherited = ast.decls.cold.inherited_ctor.get(first).copied();
    if inheriting.is_none() && inherited.is_none() {
        return;
    }
    let u = write_resolved(w, e, owner, Property::InheritedConstructor, None, |w, s| {
        for d in [inheriting, inherited] {
            match d {
                Some(d) => w.write_ref(s, d),
                None => {
                    s.uv(0).indeterminate(Cause::Unaccounted("ColdDeclData/inherited_ctor"));
                }
            }
        }
    });
    out.push(u);
}

/// Property 7, `default_immediates` (§11.4), of the default arguments
/// and default member initialiser the owner's own stream writes (written
/// in source): one unit per argument with at least one value, its
/// discriminator the parameter's index (none for a field).
fn default_immediates(w: &mut Walk<'_>, e: DeclId, owner: &Written, out: &mut Vec<Written>) {
    let ast = w.ast;
    if ast.exprs.cold.default_imm.is_empty() {
        return;
    }
    let d = subject(w, read_of(w, e, owner));
    let mut found: Vec<(Option<u64>, Immediates)> = Vec::new();
    if let Some(f) = ast.decls.function(d) {
        let params: Vec<DeclId> = ast.lists.iter::<DeclId>(f.params).collect();
        for (i, p) in params.into_iter().enumerate() {
            let (_, default) = crate::decl_func::written_default(w, p);
            if let Some(x) = default {
                found.push((Some(i as u64), immediates(w, x)));
            }
        }
    } else if let Some(f) = ast.decls.field(d)
        && let Some(x) = f.init
        && !crate::decl_class::instantiated_field(w, d)
    {
        found.push((None, immediates(w, x)));
    }
    for (disc, (values, cause)) in found {
        if let Some(u) = immediates_unit(w, e, owner, disc, values, cause) {
            out.push(u);
        }
    }
}

/// The values of the immediate invocations a walk found, and its cause.
type Immediates = (Vec<ConstValueId>, Option<Cause>);

/// The resolved unit `default_immediates` with discriminator `disc` under
/// `owner`, holding `values`; none when there is no value. `cause` is that
/// of the walk that found them: a list found by an indeterminate walk may
/// be incomplete, so the unit takes it.
fn immediates_unit(
    w: &mut Walk<'_>,
    e: DeclId,
    owner: &Written,
    disc: Option<u64>,
    values: Vec<ConstValueId>,
    cause: Option<Cause>,
) -> Option<Written> {
    if values.is_empty() {
        return None;
    }
    Some(write_resolved(w, e, owner, Property::DefaultImmediates, disc, |w, s| {
        s.absorb(cause);
        s.count(values.len());
        for v in values {
            crate::value::value(w, s, v);
        }
    }))
}

/// The `default_imm` values of the immediate invocations in `x`, in the
/// order the expression walk meets them: `x` walked again, with a unit
/// state of its own, into a stream that is dropped but for its cause.
fn immediates(w: &mut Walk<'_>, x: ExprId) -> Immediates {
    let saved = w.take_unit();
    w.unit.expr.imms = Some(Vec::new());
    let mut scratch = Stream::new();
    w.dropped += 1;
    crate::expr::child(w, &mut scratch, x);
    w.dropped -= 1;
    let values = w.unit.expr.imms.take().unwrap_or_default();
    w.unit = saved;
    (values, scratch.cause())
}

/// Records the `default_imm` value of `e` for [`immediates`]
/// (`ColdExprData` `default_imm` writes nothing in a stream, §11.4).
pub(crate) fn record_immediate(w: &mut Walk<'_>, _: &mut Stream, e: ExprId) {
    let v = w.ast.exprs.cold.default_imm.get(e).copied();
    if let (Some(imms), Some(v)) = (w.unit.expr.imms.as_mut(), v) {
        imms.push(v);
    }
}

/// The `default_argument` units (§11.4) of `owner`, the unit of entity `e`,
/// and their `default_immediates`: for each parameter of an instantiated
/// declaration of a function whose default argument Kiln instantiated
/// (the parameter holds it: `default_arg` is set at instantiation,
/// `kiln_sema/src/templ/default_args.rs:36-52`, and
/// `flags-default-arg-pending` stays set), and for a field instantiated
/// from a pattern whose initialiser Kiln instantiated. Written whenever
/// the owner is: they do not wait for a `Ref`.
fn default_arguments(w: &mut Walk<'_>, e: DeclId, owner: &Written, out: &mut Vec<Written>) {
    let ast = w.ast;
    let d = read_of(w, e, owner);
    if ast.decls.function(d).is_some() {
        let mut chain = w.redecls(ast.first(e));
        chain.reverse();
        let decls: Vec<DeclId> =
            chain.into_iter().filter(|&x| crate::decl_func::instantiated_decl(w, x)).collect();
        let params = chain_params(w, e);
        for i in 0..params.len() {
            let default = decls.iter().find_map(|&x| {
                let func = ast.decls.function(x)?;
                let param = ast.lists.iter::<DeclId>(func.params).nth(i)?;
                ast.decls.param(param)?.default_arg
            });
            if let Some(x) = default {
                default_argument(w, e, owner, i as u64, &params, x, Some(i as u64), out);
            }
        }
    } else if let Some(f) = ast.decls.field(d)
        && let Some(x) = f.init
        && crate::decl_class::instantiated_field(w, d)
    {
        default_argument(w, e, owner, 0, &[], x, None, out);
    }
}

/// The `default_argument` units of every local function that is no unit
/// and whose declaration an instantiation made (a member of a local class,
/// or the call operator of a lambda that is not generic, in an
/// instantiated function body), appended to `out` (WP-158; digest-kinds
/// §11.4). Kiln instantiates such a default argument with the body that
/// declares the function (`kiln_sema/src/templ/subst/decls.rs`
/// `defer_default_args_to`, in local scope), and the parameter holds it
/// (`ParamData` `default_arg`); the function's own stream, written in
/// place in its owner's, writes the row absent, as for every instantiated
/// declaration (`decl_func::written_default`). The unit is that of
/// [`default_arguments`]: parent the function's key (a member of a local
/// type has an ordinary key under it, digest §4.3), name `parameter(i)`,
/// derived, the function's origin file. Run once, after every unit is
/// written.
pub(crate) fn local_default_arguments(w: &mut Walk<'_>, out: &mut Vec<Written>) {
    let ast = w.ast;
    for d in ast.decls.ids() {
        if ast.first(d) != d || !is_local_default_holder(w, d) {
            continue;
        }
        let owner = local_owner(w, d);
        local_parts(w, d, &owner, out);
    }
}

/// `digest.instantiated.unheld` (WP-158, `spec/digest.md` §2.2, §12,
/// §13): what Kiln instantiated in this parse that no unit's stream
/// holds. Read from Kiln, not from the units the walk chose:
///
/// - **bodies**: every function definition in Kiln's list of
///   instantiated definitions (`DeclArena::fn_def_insts`, which Sema
///   appends to when it finishes instantiating a function's body), and
///   every function declaration with a body that an instantiation made
///   (`templ::made_by_instantiation`: its own `SpecInfo` or
///   `MemberSpecInfo` kind, else the kind governing its lexical context,
///   which reaches a local class's member and a lambda's call operator
///   in an instantiated body); its body (`FunctionData` `body`) must have
///   been written by some unit (`Walk::hold_body`);
/// - **default arguments**: every parameter that holds one
///   (`ParamData` `default_arg`) of a function declaration an
///   instantiation made, and every field an instantiation made
///   (`Ast::instantiated_from`) that holds a default member initialiser
///   (`FieldData` `init`); the expression must have been written by some
///   unit (`Walk::hold_expr`);
/// - **variable initialisers**: of every variable an instantiation made
///   that is not at block scope ([`unheld_var_init`]: a variable template
///   specialisation, a static data member of a class template
///   specialisation), when its pattern has one.
///
/// Held means written by the own stream of a unit that is kept
/// (`Walk::in_own_stream`, between `Walk::hold_begin` and
/// `Walk::hold_end`): not by a stream that is dropped (the mark's walks,
/// `immediates`, the candidates of `units::canonical_decl`, a trace).
///
/// A declaration in a dependent context (`DeclFlags::DEPENDENT_CTX`) is
/// left out: it is a pattern, written as one. So is one inside a local
/// type that a unit's stream met but could not number (digest §4.3, the
/// closure of a lambda in an instantiated default argument): the stream
/// writes the type as indeterminate and not its members, so the unit
/// that holds them is unequal to everything (`Walk::hold_type`), which
/// is the known limit of §4.3, not a missing unit. Zero on every fixture
/// (`tests/common/mod.rs`); a non-zero count names a route by which an
/// instantiation has no unit.
pub(crate) fn unheld(w: &Walk<'_>) -> u64 {
    let ast = w.ast;
    let mut bodies: std::collections::HashSet<DeclId> =
        ast.decls.fn_def_insts().iter().map(|&(d, _)| d).collect();
    let mut n = 0u64;
    // Inside a local type a stream wrote as indeterminate (its members
    // unwritten): held by that unit, unequal to everything.
    let in_held_type = |d: DeclId| {
        let mut c = ast.decls.get(d).ctx();
        while let Some(p) = c {
            if w.held_types.contains(&ast.first(p)) {
                return true;
            }
            c = ast.decls.get(p).ctx();
        }
        false
    };
    for d in ast.decls.ids() {
        if ast.decls.get(d).has(DeclFlags::DEPENDENT_CTX)
            || in_held_type(d)
            || crate::units::in_failed_substitution(ast, d)
        {
            bodies.remove(&d);
            continue;
        }
        if let Some(f) = ast.decls.function(d) {
            if !crate::templ::made_by_instantiation(w, d) {
                continue;
            }
            if f.body.is_some() {
                bodies.insert(d);
            }
            for p in ast.lists.iter::<DeclId>(f.params) {
                if let Some(x) = ast.decls.param(p).and_then(|q| q.default_arg)
                    && !w.held_exprs.contains(&x)
                {
                    n += 1;
                }
            }
        } else if let Some(f) = ast.decls.field(d)
            && let Some(x) = f.init
            && ast.instantiated_from(d).is_some()
            && !w.held_exprs.contains(&x)
        {
            n += 1;
        } else if unheld_var_init(w, d) {
            n += 1;
        }
    }
    for d in bodies {
        if let Some(b) = ast.decls.function(d).and_then(|f| f.body)
            && !w.held_bodies.contains(&b)
        {
            n += 1;
        }
    }
    n
}

/// The `default_argument` units of the local function `e` under `owner`
/// ([`local_owner`]), appended to `out`: what [`local_default_arguments`]
/// writes for it (`trace.rs` writes them again).
pub(crate) fn local_parts(w: &mut Walk<'_>, e: DeclId, owner: &Written, out: &mut Vec<Written>) {
    default_arguments(w, e, owner, out);
}

/// Whether `e` (a first declaration) is a function [`local_default_arguments`]
/// writes `default_argument` units for: local (not a block-scope extern),
/// no unit, made by instantiation (`decl_func::instantiated_decl`), with a
/// parameter of an instantiated declaration that holds a default
/// argument.
pub(crate) fn is_local_default_holder(w: &Walk<'_>, e: DeclId) -> bool {
    let ast = w.ast;
    if ast.decls.function(e).is_none()
        || !w.is_local(e)
        || w.is_local_extern(e)
        || crate::units::in_failed_substitution(ast, e)
    {
        return false;
    }
    let decls: Vec<DeclId> =
        w.redecls(e).into_iter().filter(|&x| crate::decl_func::instantiated_decl(w, x)).collect();
    let holds = decls.iter().any(|&x| {
        ast.decls.function(x).is_some_and(|f| {
            ast.lists
                .iter::<DeclId>(f.params)
                .any(|p| ast.decls.param(p).is_some_and(|q| q.default_arg.is_some()))
        })
    });
    holds && crate::units::unit_class(w, e).is_none()
}

/// The unit a local function's `default_argument` units are keyed under
/// ([`local_default_arguments`]): no unit is written for it; it gives the
/// parts their parent key (the function's, or a fallback under its
/// nearest keyed context when it has none, whose cause the parts carry,
/// `write_part`), their origin file and the declaration they are read
/// from.
pub(crate) fn local_owner(w: &mut Walk<'_>, e: DeclId) -> Written {
    let (key, kind) = match w.key(e) {
        KeyRes::Key { id, kind, .. } => (id, kind),
        KeyRes::None(_) => crate::units::fallback_key(w, e, slip_model::Digest([0; 32]))
            .map_or((slip_model::Digest([0; 32]), KeyKind::Function), |(id, kind, _)| (id, kind)),
    };
    Written {
        entity: Some(e),
        read: Some(crate::units::read_decl(w, e)),
        key,
        kind,
        digest: slip_model::Digest([0; 32]),
        cause: None,
        class: GroupClass::Derived,
        file: crate::units::origin_file(w, e),
        resolved: None,
        default_argument: None,
        owner: None,
        collided: false,
        view: None,
        denotes: None,
        emitted: false,
    }
}

/// Whether `d` is the first declaration of a variable an instantiation
/// made, not at block scope, whose initialiser Kiln instantiated and no
/// kept stream wrote ([`unheld`]): a variable template specialisation, a
/// static data member of a class template specialisation. A variable at
/// block scope (a local, a `static` local) is in its function's body,
/// which is counted as a body. The initialiser is any declaration's of
/// the chain (`var_data` and `var_definition` write the one
/// `init_holder` names); a pattern with no initialiser makes none to
/// count, as `var_definition` writes none (default-initialisation).
fn unheld_var_init(w: &Walk<'_>, d: DeclId) -> bool {
    let ast = w.ast;
    if ast.decls.get(d).kind() != DeclKind::Var
        || ast.first(d) != d
        || w.is_local(d)
        || !crate::templ::made_by_instantiation(w, d)
    {
        return false;
    }
    let inits: Vec<_> =
        ast.redecls_of(d).filter_map(|x| ast.decls.var(x).and_then(|v| v.init)).collect();
    if inits.is_empty() || (instantiated_pattern(w, d).is_some() && !pattern_has_init(w, d)) {
        return false;
    }
    !inits.iter().any(|x| w.held_exprs.contains(x))
}

/// One `default_argument` unit: the expression `x`, walked as an
/// initialiser is, with parameter j of the function (each declaration's
/// parameter j, `params[j]`) the local entity of ordinal j, as the
/// function's own stream numbers them, and later locals numbered from
/// there; then its `default_immediates` (discriminator `disc`).
#[allow(clippy::too_many_arguments, reason = "one unit and its resolved one")]
fn default_argument(
    w: &mut Walk<'_>,
    e: DeclId,
    owner: &Written,
    index: u64,
    params: &[Vec<DeclId>],
    x: ExprId,
    disc: Option<u64>,
    out: &mut Vec<Written>,
) {
    let mut values = Vec::new();
    let u = write_default_argument(w, e, owner, index, |w, s| {
        number_params(w, params);
        w.unit.expr.imms = Some(Vec::new());
        w.hold_expr(x);
        crate::expr::child(w, s, x);
        values = w.unit.expr.imms.take().unwrap_or_default();
    });
    let imms = immediates_unit(w, e, &u, disc, values, u.cause);
    out.push(u);
    out.extend(imms);
}

/// Parameter i of every declaration of function `e`'s chain (their first
/// declarations), by index i.
fn chain_params(w: &Walk<'_>, e: DeclId) -> Vec<Vec<DeclId>> {
    let ast = w.ast;
    let mut params: Vec<Vec<DeclId>> = Vec::new();
    for x in w.redecls(ast.first(e)) {
        let Some(f) = ast.decls.function(x) else { continue };
        for (i, p) in ast.lists.iter::<DeclId>(f.params).enumerate() {
            if params.len() <= i {
                params.resize_with(i + 1, Vec::new);
            }
            params[i].push(ast.first(p));
        }
    }
    params
}

/// Numbers the parameters `params` (`chain_params`) in the unit being
/// written as the function's own stream does: parameter j is local j, and
/// the next local is the parameter count.
fn number_params(w: &mut Walk<'_>, params: &[Vec<DeclId>]) {
    for (j, ps) in params.iter().enumerate() {
        for &p in ps {
            w.unit.locals.entry(p).or_insert(j as u64);
        }
    }
    w.unit.next_local = params.len() as u64;
}

/// Whether function `f` is of the deferrable class (§11.2). It reads the
/// entity's representative declaration and facts resolution leaves alone:
///
/// - a destructor (rule 4);
/// - a function every declaration of which is implicit: an implicitly
///   declared special member, comparison operator or inheriting
///   constructor (rules 1 and 2), and an inheriting constructor by its
///   `inheriting_ctor` entry (rule 2);
/// - a function explicitly defaulted on its representative declaration
///   (rule 1): once resolved, its type does not say whether a
///   specification was written (`kiln_sema/src/class/special/excspec.rs:53-118`
///   re-points every declaration), so every one is of the class;
/// - an instantiated declaration (`instantiated`) whose pattern's
///   specification Kiln defers (rule 3), or whose pattern is of the class.
pub(crate) fn deferrable(w: &Walk<'_>, f: DeclId) -> bool {
    deferrable_at(w, f, 0)
}

/// Patterns deeper than this are not followed: the function is of the
/// class (over-inclusion is safe, §11.2).
const PATTERN_LIMIT: u32 = 64;

fn deferrable_at(w: &Walk<'_>, f: DeclId, depth: u32) -> bool {
    let ast = w.ast;
    // Rule 4: every destructor.
    if ast.decls.get(f).kind() == DeclKind::CXXDestructor {
        return true;
    }
    let first = ast.first(f);
    // Rules 1 and 2: implicitly declared members and inheriting constructors.
    if w.redecls(first).iter().all(|&d| ast.decls.get(d).has(DeclFlags::IMPLICIT))
        || ast.decls.cold.inheriting_ctor.contains(first)
    {
        return true;
    }
    let rep = w.representative(first);
    match ast.decls.function(rep).map(|d| d.spec.defaulted()) {
        Some(Defaulted::Explicit | Defaulted::Implicit) => return true,
        Some(Defaulted::No) | None => {}
    }
    // Rule 3.
    if !instantiated(w, rep) {
        return false;
    }
    if depth >= PATTERN_LIMIT {
        return true;
    }
    let Some(pattern) = pattern_of(w, rep) else { return true };
    w.redecls(ast.first(pattern)).into_iter().any(|d| defers(w, d))
        || deferrable_at(w, pattern, depth + 1)
}

/// Whether the declaration `x` was made by instantiation, of any form
/// (`templ::made_by_instantiation`, WP-045's test): Kiln defers the
/// specification of the declarations it makes, under every form
/// (`kiln_sema/src/templ/subst/decls.rs:766-770`, `:1170-1195`), and
/// resolves it when it defines the function
/// (`kiln_sema/src/decl.rs:9670-9676`), so an explicit instantiation
/// definition resolves it only for the members it defines. A friend
/// written in source that redeclares a member of an instantiation was
/// not made by one, and its specification is not deferred.
fn instantiated(w: &Walk<'_>, x: DeclId) -> bool {
    crate::templ::made_by_instantiation(w, x)
}

/// The pattern the instantiated declaration `x` was made from: for a
/// function template's specialisation, the primary template's pattern
/// (`SpecInfo` `from` names the template until the definition is
/// instantiated, so it is not read); else `MemberSpecInfo`
/// `instantiated_from` or Kiln's map (`Ast::instantiated_from`). The
/// pattern of a member template's pattern in an instantiated class is
/// named through the template it was made from: that template's pattern.
fn pattern_of(w: &Walk<'_>, x: DeclId) -> Option<DeclId> {
    let ast = w.ast;
    if let Some((_, i)) = ast.spec_info(x) {
        return ast.template_info(i.template).and_then(|t| t.pattern);
    }
    let from = ast.instantiated_from(x).or_else(|| ast.instantiated_from(w.entity(x)))?;
    Some(ast.template_info(from).and_then(|t| t.pattern).unwrap_or(from))
}

/// Whether the declaration `d`'s specification is one Kiln defers in an
/// instantiation (`kiln_sema/src/except.rs:729-754`): a value-dependent
/// `noexcept` operand, a dynamic specification naming a dependent type, or
/// one already deferred.
fn defers(w: &Walk<'_>, d: DeclId) -> bool {
    let ast = w.ast;
    let Some(p) = ast.decls.get(d).ty().and_then(|q| ast.types.proto(q)) else { return false };
    match ast.types.exc_spec(p) {
        ExcSpec::NoexceptExpr { value: NoexceptValue::Dependent(_), .. }
        | ExcSpec::DependentNoexcept(_)
        | ExcSpec::Deferred(_) => true,
        ExcSpec::Dynamic(l) => {
            ast.types.type_list_types(l).iter().any(|&t| ast.types.is_dependent(t))
        }
        ExcSpec::None
        | ExcSpec::Nothrow
        | ExcSpec::DynamicNone
        | ExcSpec::NothrowAttr
        | ExcSpec::BasicNoexcept
        | ExcSpec::NoexceptExpr { .. } => false,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use slip_kiln::kiln::ast::DeclId;
    use slip_kiln::kiln::ast::decl::DeclKind;

    use super::{chain_params, inherited_constructor, number_params};
    use crate::stream::{Stream, UNIT_TAG};
    use slip_kiln::kiln::ast::decl::DeclFlags;
    use slip_kiln::kiln::ast::types::ExcSpec;

    use slip_model::Digest;
    use slip_model::digests::GroupClass;
    use slip_model::keys::{Key, KeyKind, Name, Parent, RuleTable};

    /// The units of `src` by name: (class, file, key, digest).
    fn units(src: &str) -> BTreeMap<String, (GroupClass, Vec<u8>, Digest, Digest)> {
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
            let o = crate::digest(&input, &options);
            let names: BTreeMap<Digest, String> = o.names.iter().cloned().collect();
            let mut out = BTreeMap::new();
            for g in &o.digests.groups {
                for u in g.units.iter().flatten() {
                    let name = names.get(&u.key).cloned().unwrap_or_default();
                    out.insert(name, (g.class, g.file.clone(), u.key, u.digest));
                }
            }
            out
        })
        .expect("the snippet parses")
    }

    // digest-kinds §11.4: a `default_argument` unit numbers the
    // function's parameters as the function's own stream does, so a
    // default argument naming an earlier parameter names the same local.
    // (Its bytes cannot show it while `sizeof` is `Pending(Expr)`: the
    // operand is not written yet.)
    #[test]
    fn default_argument_numbers_parameters_as_the_function() {
        let src = "template <class T> struct A { void f(T p, int q = sizeof(p)); };
                   A<long> x; void h() { x.f(1); }";
        crate::decl_func::tests::with_walk(src, |w| {
            let ast = w.ast;
            let f = crate::decl_func::tests::named(w, "f", DeclKind::CXXMethod)
                .into_iter()
                .find(|&d| crate::decl_func::instantiated_decl(w, d))
                .expect("A<long>::f");
            let ps: Vec<DeclId> =
                ast.lists.iter::<DeclId>(ast.decls.function(f).expect("f").params).collect();
            // The function's own stream.
            let saved = w.take_unit();
            let mut s = Stream::new();
            crate::decl_func::function_data(w, &mut s, f);
            let own: Vec<u64> = ps.iter().map(|&p| w.unit.locals[&ast.first(p)]).collect();
            w.unit = saved;
            // The `default_argument` unit's.
            let saved = w.take_unit();
            number_params(w, &chain_params(w, f));
            let unit: Vec<u64> = ps.iter().map(|&p| w.unit.locals[&ast.first(p)]).collect();
            assert_eq!(w.unit.next_local, 2);
            w.unit = saved;
            assert_eq!(own, vec![0, 1]);
            assert_eq!(unit, own);
        });
    }

    // digest-kinds §11.3: a variable's `value` stream lists the values of
    // the temporaries it extends in `mangling_index` order.
    #[test]
    fn value_lists_temporaries_in_mangling_order() {
        let src = "struct P { const int &x; const int &y; const int &z; };\n\
                   constexpr P p{1, 2, 3};\n";
        crate::decl_func::tests::with_walk(src, |w| {
            let ast = w.ast;
            let v = crate::decl_func::tests::named(w, "p", DeclKind::Var)[0];
            let mut lets: Vec<(u32, DeclId)> = ast
                .decls
                .ids()
                .filter_map(|d| ast.decls.lifetime_extended(d).map(|l| (l.mangling_index, d)))
                .collect();
            assert_eq!(lets.len(), 3);
            lets.sort_unstable();
            let owner = crate::units::write(w, v, crate::units::UnitClass::Derived).expect("unit");
            let mut out = Vec::new();
            super::value(w, v, &owner, &mut out);
            assert_eq!(out.len(), 1);
            let saved = w.take_unit();
            let mut s = Stream::new();
            let own = crate::decl_func::var_eval(w, v).state();
            let slip_kiln::kiln::ast::decl::VarEvalState::Value(c) = own else {
                panic!("p has a value: {own:?}")
            };
            s.opt(true);
            crate::value::value(w, &mut s, c);
            for (_, t) in &lets {
                let c = ast.decls.lifetime_extended(*t).and_then(|l| l.value).expect("a value");
                s.opt(true);
                crate::value::value(w, &mut s, c);
            }
            w.unit = saved;
            assert_eq!(out[0].digest, s.finish(UNIT_TAG).digest);
        });
    }

    // digest-kinds §11.2 rule 3 (WP-046 review F2): the specialisation of
    // a member template of a class template's instantiation. Its pattern
    // is the member template's pattern in the instantiated class
    // (`SpecInfo`), which Kiln holds `Deferred`; that pattern's own pattern
    // (through the template Kiln's map names) has a dependent `noexcept`.
    #[test]
    fn member_template_specialisation_reads_its_patterns() {
        let src = "template <class T> struct A { template <class U> void f() noexcept(sizeof(U) > 1); };\n\
                   void (A<char>::*p)() = &A<char>::f<int>;\n";
        crate::decl_func::tests::with_walk(src, |w| {
            let ast = w.ast;
            let fs = crate::decl_func::tests::named(w, "f", DeclKind::CXXMethod);
            // The specialisation A<char>::f<int>, and the member template's
            // pattern in A<char>, each made by instantiation.
            let spec = *fs.iter().find(|&&d| ast.spec_info(d).is_some()).expect("f<int>");
            let member = super::pattern_of(w, spec).expect("its pattern");
            assert!(super::instantiated(w, member));
            assert!(fs.contains(&member));
            let spec_ty = |d: DeclId| {
                let p = ast.decls.get(d).ty().and_then(|q| ast.types.proto(q)).expect("proto");
                ast.types.exc_spec(p)
            };
            assert!(matches!(spec_ty(member), ExcSpec::Deferred(_)));
            assert!(super::defers(w, member));
            let primary = super::pattern_of(w, member).expect("the member's pattern");
            assert!(!super::instantiated(w, primary));
            assert!(matches!(spec_ty(primary), ExcSpec::DependentNoexcept(_)));
            assert!(super::deferrable(w, member));
            assert!(super::deferrable(w, spec));
        });
    }

    // digest-kinds §11.2 rule 2: an inheriting constructor is `IMPLICIT`
    // on every declaration, so rule 1 already holds it (the
    // `inheriting_ctor` test is a second reading of the same fact).
    #[test]
    fn inheriting_constructor_is_implicit_everywhere() {
        let src = "struct B0 { B0(int); B0(long) noexcept; };\n\
                   struct D : B0 { using B0::B0; };\n\
                   void h() { D a(1); D b(1L); }\n";
        crate::decl_func::tests::with_walk(src, |w| {
            let ast = w.ast;
            let ctors: Vec<DeclId> =
                ast.decls.ids().filter(|&d| ast.decls.cold.inheriting_ctor.contains(d)).collect();
            assert_eq!(ctors.len(), 2);
            for c in ctors {
                assert!(
                    w.redecls(ast.first(c))
                        .iter()
                        .all(|&d| ast.decls.get(d).has(DeclFlags::IMPLICIT))
                );
                assert!(super::deferrable(w, c));
            }
        });
    }

    // digest-kinds §11.1 property 6: the stream is the `ref` of the
    // using-declaration's shadow, then that of the base constructor.
    #[test]
    fn inherited_constructor_stream_is_the_two_refs() {
        let src = "struct B0 { B0(int); }; struct D : B0 { using B0::B0; }; void h() { D d(1); }";
        crate::decl_func::tests::with_walk(src, |w| {
            let ast = w.ast;
            let dc = ast
                .decls
                .ids()
                .find(|&d| ast.decls.cold.inheriting_ctor.contains(d))
                .expect("the inheriting constructor");
            let shadow = ast.decls.cold.inheriting_ctor.get(dc).copied().expect("shadow");
            let base = ast.decls.cold.inherited_ctor.get(dc).copied().expect("base");
            let owner = crate::units::write(w, dc, crate::units::UnitClass::Derived).expect("unit");
            let mut out = Vec::new();
            inherited_constructor(w, dc, &owner, &mut out);
            assert_eq!(out.len(), 1);
            let saved = w.take_unit();
            let mut s = Stream::new();
            w.write_ref(&mut s, shadow);
            w.write_ref(&mut s, base);
            w.unit = saved;
            let h = s.finish(UNIT_TAG);
            assert_eq!(out[0].digest, h.digest);
            // Empty, or one ref only, is not the same digest.
            let mut one = Stream::new();
            w.write_ref(&mut one, base);
            assert_ne!(out[0].digest, one.finish(UNIT_TAG).digest);
        });
    }

    // WP-158 (`spec/digest.md` §2.2, §13): `unheld` reads what Kiln
    // instantiated, not the units the walk chose. In a walk that has
    // written no unit, every instantiated body, default argument, default
    // member initialiser and variable initialiser is unheld; holding them
    // all makes the count 0. A count read from the digest's own list of
    // units would be 0 here as well.
    const UNHELD_X: &str = "struct X { static int f(int) { return 1; } static int f(long) { return 2; } \
                            int mf(int) { return 1; } int mf(long) { return 2; } };\n";

    /// Holds every body, default argument, default member initialiser and
    /// variable initialiser of the parse.
    fn hold_everything(w: &mut crate::walk::Walk<'_>) {
        let ast = w.ast;
        for d in ast.decls.ids() {
            if let Some(f) = ast.decls.function(d) {
                if let Some(b) = f.body {
                    w.held_bodies.insert(b);
                }
                for p in ast.lists.iter::<DeclId>(f.params) {
                    if let Some(x) = ast.decls.param(p).and_then(|q| q.default_arg) {
                        w.held_exprs.insert(x);
                    }
                }
            } else if let Some(x) = ast.decls.field(d).and_then(|f| f.init) {
                w.held_exprs.insert(x);
            } else if let Some(x) = ast.decls.var(d).and_then(|v| v.init) {
                w.held_exprs.insert(x);
            }
        }
    }

    #[test]
    fn unheld_reads_what_kiln_instantiated() {
        // The counts: the call operator's specialisation, and with the
        // conversion its invoker's and the conversion function's
        // specialisations (3, 1); `c<X>`'s body, the local function's
        // body and its default argument (3, 3); `D<X>`'s default member
        // initialiser and one function body Kiln made for `D<X>` (2); the
        // variable template specialisation's initialiser (1); the static
        // data member's (1).
        let cases: [(&str, &str, u64); 7] = [
            (
                "to pointer",
                "int w() { auto l = [](auto u) { return u.mf(1L); }; int (*p)(X) = l; return p(X()); }",
                3,
            ),
            (
                "decltype",
                "int w() { auto l = [](auto u) { return u.mf(1L); }; decltype(l(X())) x = 0; return x; }",
                1,
            ),
            (
                "lambda default",
                "template <class T> int c(T) { auto l = [](int k = T::f(1L)) { return k; }; return l(); }\nint w() { return c(X()); }",
                3,
            ),
            (
                "local class default",
                "template <class T> int c(T) { struct L { int g(int k = T::f(1L)) { return k; } }; return L().g(); }\nint w() { return c(X()); }",
                3,
            ),
            (
                "default member initialiser",
                "template <class T> struct D { int k = T::f(1L); };\nint w() { return D<X>().k; }",
                2,
            ),
            (
                "variable template",
                "template <class T> int v = T::f(1L);\nint w() { return v<X>; }",
                1,
            ),
            (
                "static data member",
                "template <class T> struct S { static int s; };\ntemplate <class T> int S<T>::s = T::f(1L);\nint w() { return S<X>::s; }",
                1,
            ),
        ];
        for (what, body, n) in cases {
            crate::decl_func::tests::with_walk(&format!("{UNHELD_X}{body}\n"), |w| {
                assert!(w.held_bodies.is_empty() && w.held_exprs.is_empty());
                assert_eq!(super::unheld(w), n, "{what}");
                hold_everything(w);
                assert_eq!(super::unheld(w), 0, "{what}");
            });
        }
    }

    // WP-158 fix round (review F4, mutation MD): a local type held as
    // indeterminate leaves out only what it encloses. `Q` is held; the
    // specialisation of `w`'s generic lambda is still counted.
    #[test]
    fn a_held_local_type_leaves_out_only_its_members() {
        let src = format!(
            "{UNHELD_X}int z() {{ struct Q {{ int q() {{ return 0; }} }}; return Q().q(); }}\n\
             int w() {{ auto l = [](auto u) {{ return u.mf(1L); }}; decltype(l(X())) x = 0; return x; }}\n"
        );
        crate::decl_func::tests::with_walk(&src, |w| {
            let q = crate::decl_func::tests::named(w, "Q", DeclKind::CXXRecord)[0];
            assert_eq!(super::unheld(w), 1);
            w.held_types.insert(w.ast.first(q));
            assert_eq!(super::unheld(w), 1);
        });
    }

    // WP-158 fix round (review F5): a stream that is dropped holds
    // nothing. `immediates` walks a default argument again into a
    // dropped stream, inside the run's window; the closure it meets (and
    // its call operator's body) must not be recorded as held, or a body
    // no kept stream wrote would leave the count.
    #[test]
    fn a_dropped_stream_holds_nothing() {
        let src = "consteval int id(int x) { return x; }\n\
                   void f(int a = id(3) + [] { struct Q *p = nullptr; return p ? 1 : 0; }());\n\
                   void h() { f(); }\n";
        crate::decl_func::tests::with_walk(src, |w| {
            let ast = w.ast;
            let f = crate::decl_func::tests::named(w, "f", DeclKind::Function)[0];
            let p = ast.lists.iter::<DeclId>(ast.decls.function(f).expect("f").params).next();
            let (_, x) = crate::decl_func::written_default(w, p.expect("a"));
            w.hold_begin();
            let _ = super::immediates(w, x.expect("the default"));
            let recorded = w.hold_pending.as_ref().map_or(0, Vec::len);
            w.hold_end(true);
            assert_eq!(recorded, 0);
            assert!(w.held_bodies.is_empty() && w.held_exprs.is_empty() && w.held_types.is_empty());
        });
    }

    // digest-kinds §11.1 property 7: the values of a written default are
    // found by a second walk into a dropped stream; its cause is kept, as
    // a list an indeterminate walk found may be short (a local class
    // that `struct Q *` declares with no declaration statement is
    // `UndeclaredLocal`, digest §4.3).
    #[test]
    fn immediates_keep_the_walks_cause() {
        let src = "consteval int id(int x) { return x; }\n\
                   void f(int a = id(3) + [] { struct Q *p = nullptr; return p ? 1 : 0; }());\n\
                   void h() { f(); }\n";
        crate::decl_func::tests::with_walk(src, |w| {
            let ast = w.ast;
            let f = crate::decl_func::tests::named(w, "f", DeclKind::Function)[0];
            let p = ast.lists.iter::<DeclId>(ast.decls.function(f).expect("f").params).next();
            let (_, x) = crate::decl_func::written_default(w, p.expect("a"));
            let (values, cause) = super::immediates(w, x.expect("the default"));
            assert_eq!(values.len(), 1);
            assert_eq!(cause, Some(crate::Cause::UndeclaredLocal));
        });
    }

    // digest-kinds §11.1 property 7: a type digest (and a profile walk)
    // puts the expression state aside, `imms` with it, and restores it.
    // Kiln records no `default_imm` entry inside an unevaluated operand
    // (a `decltype`), so nothing is lost there: the list is `id(1)`'s.
    #[test]
    fn immediates_inside_a_type_are_not_listed() {
        let src = "consteval int id(int x) { return x; }\n\
                   void f(int a = id(1) + sizeof(decltype(id(2))));\n\
                   void h() { f(); }\n";
        crate::decl_func::tests::with_walk(src, |w| {
            let ast = w.ast;
            assert_eq!(ast.exprs.cold.default_imm.len(), 1);
            let f = crate::decl_func::tests::named(w, "f", DeclKind::Function)[0];
            let p = ast.lists.iter::<DeclId>(ast.decls.function(f).expect("f").params).next();
            let (_, x) = crate::decl_func::written_default(w, p.expect("a"));
            let (values, cause) = super::immediates(w, x.expect("the default"));
            assert_eq!((values.len(), cause), (1, None));
            assert!(w.unit.expr.imms.is_none());
        });
    }

    // digest-kinds §11.3: the state is the initialiser holder's
    // (`decl_func::var_eval`), whichever declaration the unit reads.
    #[test]
    fn value_is_read_from_the_initialisers_holder() {
        for src in [
            "struct S { static constexpr int x = 1; };\nconstexpr int S::x;\n",
            "struct S { static const int x; };\nconst int S::x = 1;\n",
            "extern const int x;\nconstexpr int x = 1;\n",
        ] {
            let u = units(src);
            let name = if src.contains("S::") { "S::x::<value>" } else { "x::<value>" };
            assert!(u.contains_key(name), "{src}: {:?}", u.keys().collect::<Vec<_>>());
        }
    }

    // digest-kinds §11.1 property 6: the base constructor an inheriting
    // constructor names is in its resolved unit only. Two programs whose
    // inherited constructor is a different entity (the base's namespace)
    // give the inheriting constructor equal own units.
    #[test]
    fn inherited_constructor_is_not_in_the_own_unit() {
        let src = |ns: &str| {
            format!(
                "namespace {ns} {{ struct B0 {{ B0(int); }}; }}\n\
                 struct D : {ns}::B0 {{ using {ns}::B0::B0; }};\n\
                 void h() {{ D d(1); }}\n"
            )
        };
        let a = units(&src("n"));
        let b = units(&src("m"));
        let own = "D::B0(int)";
        let res = "D::B0(int)::<inherited_constructor>";
        let (ua, ub) = (a.get(own).expect("own A"), b.get(own).expect("own B"));
        assert_eq!(ua.2, ub.2, "the key");
        assert_eq!(ua.3, ub.3, "the own unit");
        assert_ne!(a.get(res).expect("A").3, b.get(res).expect("B").3);
    }

    // digest-kinds §11.1, digest §2.2: a resolved unit's key is kind
    // `resolved`, parent the owner's key, name `property(n)`, no
    // discriminator; it is derived, in the owner's file. Property 4, the
    // worked example: `S::f` is the key function.
    #[test]
    fn key_function_unit_is_keyed_under_its_class() {
        let u = units("struct S { virtual void f(); virtual void g() {} };\n");
        let (class, file, owner, _) = u.get("S").cloned().expect("S");
        assert_eq!(class, GroupClass::Declared);
        let (class, rfile, key, _) = u.get("S::<key_function>").cloned().expect("the unit");
        assert_eq!(class, GroupClass::Derived);
        assert_eq!(rfile, file);
        let want = Key {
            kind: KeyKind::Resolved,
            parent: Parent::Unit(owner),
            name: Name::Property(4),
            disc: None,
        };
        assert_eq!(key, want.id());
        // A class with no virtual function, or whose virtual functions are
        // all inline, has none.
        let u = units("struct T { virtual void f() {} };\nstruct V { void f(); };\n");
        assert!(!u.keys().any(|n| n.ends_with("::<key_function>")), "{:?}", u.keys());
    }

    // §11.1 property 4: the stream is the key function's `ref`, so two
    // classes whose key functions differ have different resolved units.
    #[test]
    fn key_function_stream_is_the_ref() {
        let a = units("struct S { virtual void f(); virtual void g(); };\n");
        let b = units("struct S { virtual void f() {} virtual void g(); };\n");
        let unit = |u: &BTreeMap<String, (GroupClass, Vec<u8>, Digest, Digest)>| {
            u.get("S::<key_function>").cloned().expect("the unit")
        };
        assert_eq!(unit(&a).2, unit(&b).2, "one key");
        assert_ne!(unit(&a).3, unit(&b).3, "f against g");
    }
}
