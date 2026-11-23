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

use slip_kiln::kiln::ast::decl::{DeclFlags, DeclKind, Defaulted};
use slip_kiln::kiln::ast::templates::SpecKind;
use slip_kiln::kiln::ast::types::TypeView;
use slip_kiln::kiln::ast::{DeclId, QualType};
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

/// The mapped `Key` of the written resolved unit `u`, computed again from
/// `entity`, the mapped key of its entity, as [`write_resolved`] builds
/// it; `None` for a unit that is not a resolved one (its key is
/// `entity`). For unit records (`records.rs`); hashes into no unit.
/// WP-046 has the same function, which also handles `default_argument`
/// units: whichever of the two merges second keeps WP-046's.
#[allow(
    clippy::needless_pass_by_value,
    reason = "WP-046's signature, which consumes `entity`; the second to merge keeps that one"
)]
pub(crate) fn part_key(w: &Walk<'_>, entity: Key, u: &Written) -> Option<Key> {
    let r = u.resolved?;
    let disc = r.disc.map(|n| {
        let mut ds = Stream::new();
        ds.uv(n);
        ds.finish(DISC_TAG).digest
    });
    Some(w.rules.map(Key {
        kind: KeyKind::Resolved,
        parent: Parent::Unit(entity.id()),
        name: Name::Property(u64::from(r.property.number())),
        disc,
    }))
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
    // unit, nor to another resolved unit).
    if owner.entity != Some(e) || owner.resolved.is_some() {
        return;
    }
    for p in Property::ALL {
        match p {
            Property::KeyFunction => key_function(w, e, owner, out),
            // WP-046.
            Property::ExceptionSpec
            | Property::Value
            | Property::DtorDelete
            | Property::InheritedConstructor
            | Property::DefaultImmediates => {}
            Property::Definition => definition(w, e, owner, out),
        }
    }
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
    let saved = w.take_unit();
    let mut s = Stream::new();
    body(w, &mut s);
    w.unit = saved;
    let head = s.finish(UNIT_TAG);
    let head = w.hashed(head);
    let disc_digest = disc.map(|n| {
        let mut ds = Stream::new();
        ds.uv(n);
        let dh = ds.finish(DISC_TAG);
        w.hashed(dh).digest
    });
    let key = Key {
        kind: KeyKind::Resolved,
        parent: Parent::Unit(owner.key),
        name: Name::Property(u64::from(p.number())),
        disc: disc_digest,
    };
    let key = w.rules.map(key);
    w.bytes += key.to_bytes().len() as u64;
    // The cause the owner's key carries, which its parent passes on.
    let key_cause = match w.key(e) {
        KeyRes::Key { cause, .. } => cause,
        KeyRes::None(cause) => Some(cause.unwrap_or(Cause::Unaccounted("key/none"))),
    };
    let mut cause = head.cause;
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
        kind: KeyKind::Resolved,
        digest: head.digest,
        cause,
        class: GroupClass::Derived,
        file,
        resolved: Some(Resolved { property: p, disc }),
    }
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
    let ast = w.ast;
    let u = write_resolved(w, e, owner, Property::Definition, None, |w, s| {
        // Every declaration of the entity is read, an out-of-line one an
        // instantiation made included (`Walk::redecls`).
        w.unit.definition = true;
        crate::templ::spec_definition(w, s, e);
        if ast.decls.function(def).is_some() {
            crate::decl_func::function_definition(w, s, e, def);
        } else if ast.decls.var(def).is_some() {
            crate::decl_func::var_definition(w, s, e, def);
        } else if ast.decls.record(def).is_some() {
            crate::decl_class::record_definition(w, s, def);
        } else if ast.decls.enumeration(def).is_some() {
            crate::decl_class::enum_definition(w, s, def);
        }
    });
    out.push(u);
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
        if pattern_of(w, e).is_some_and(|p| ast.redecls_of(p).any(has_init))
            && !chain.iter().copied().any(has_init)
        {
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
    let Some(p) = pattern_of(w, e) else { return false };
    ast.redecls_of(p).any(|r| ast.decls.get(r).ty().is_some_and(|q| holds_placeholder(w, q)))
}

/// The pattern entity `e` is instantiated from: the pattern of its
/// template for a specialisation (not the partial specialisation or the
/// definition `SpecInfo` `from` names, which Kiln sets on use), the
/// member it was instantiated from for a member of a specialisation.
fn pattern_of(w: &Walk<'_>, e: DeclId) -> Option<DeclId> {
    let ast = w.ast;
    let e = w.entity(e);
    match ast.spec_info(e) {
        Some((_, i)) => ast.template_info(i.template).and_then(|t| t.pattern),
        None => ast.member_spec_info(e).map(|(_, i)| i.instantiated_from),
    }
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
///   that kind.
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

/// Whether function `f` is of the deferrable class (§11.2); `None` where
/// the rule needs what the demand area decides (an explicitly defaulted
/// function, whose own specification may or may not have been written;
/// an implicit instantiation, whose pattern's specification is an
/// expression), so the caller writes a pending row.
pub(crate) fn deferrable(w: &Walk<'_>, f: DeclId) -> Option<bool> {
    let ast = w.ast;
    let n = ast.decls.get(f);
    // Rule 4: every destructor.
    if n.kind() == DeclKind::CXXDestructor {
        return Some(true);
    }
    // Rules 1 and 2: implicitly declared special members, comparison
    // operators and inheriting constructors.
    if w.redecls(f).iter().all(|&d| ast.decls.get(d).has(DeclFlags::IMPLICIT)) {
        return Some(true);
    }
    let first = ast.first(f);
    if let Some(data) = ast.decls.function(first) {
        match data.spec.defaulted() {
            Defaulted::Implicit => return Some(true),
            Defaulted::Explicit => return None,
            Defaulted::No => {}
        }
    }
    // Rule 3: implicit instantiations.
    let implicit_instantiation = ast
        .spec_info(f)
        .map(|(_, i)| i.kind)
        .or_else(|| ast.member_spec_info(f).map(|(_, i)| i.kind))
        .is_some_and(|k| match k {
            SpecKind::ImplicitInstantiation => true,
            SpecKind::Undeclared
            | SpecKind::ExplicitSpecialization
            | SpecKind::ExplicitInstantiationDeclaration
            | SpecKind::ExplicitInstantiationDefinition => false,
        });
    if implicit_instantiation {
        return None;
    }
    Some(false)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

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
            let options = crate::Options { rules: &rules, detail: crate::Detail::All, names: true };
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
