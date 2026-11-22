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

use slip_kiln::kiln::ast::DeclId;
use slip_kiln::kiln::ast::decl::{DeclFlags, DeclKind, Defaulted};
use slip_kiln::kiln::ast::templates::SpecKind;
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
            Property::KeyFunction => key_function(w, e, owner, out),
            // WP-046.
            Property::ExceptionSpec
            | Property::Value
            | Property::DtorDelete
            | Property::InheritedConstructor
            | Property::DefaultImmediates => {}
            Property::Definition => {}
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
#[allow(dead_code, reason = "its caller, the `default_argument` arm, lands in the next commit")]
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
    let mut s = Stream::new();
    body(w, &mut s);
    w.unit = saved;
    let h = s.finish(UNIT_TAG);
    let h = w.hashed(h);
    let disc_digest = disc.map(|n| {
        let mut d = Stream::new();
        d.uv(n);
        let h = d.finish(DISC_TAG);
        w.hashed(h).digest
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
