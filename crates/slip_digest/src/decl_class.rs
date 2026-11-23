//! The payload records of classes, enumerations, their members and
//! typedefs (`spec/digest-kinds.md` §5.5): `RecordData`, `TagKind`,
//! `DefinitionBits`, `BaseSpec`, `FieldData`, `EnumData`,
//! `EnumeratorData`, `TypedefData`, `CtorInitKind` and `CtorInit`, with
//! digest §5 (which order is kept) and §2.3 (read from the defining
//! declaration).
//!
//! Each function writes one `record T` for the declaration `d`, the
//! declaration the unit is read from (§2.3). What the spec left open is in
//! `spec/landed/digest-kinds/WP-027.md`.

use std::collections::HashSet;

use slip_kiln::kiln::ast::decl::{
    Access, BaseSpec, CtorInit, CtorInitKind, DeclFlags, DeclKind, DefinitionBits, EnumData,
    EnumeratorData, FieldData, RecordData, TagKind, TypedefData,
};
use slip_kiln::kiln::ast::ids::TypeId;
use slip_kiln::kiln::ast::templates::SpecKind;
use slip_kiln::kiln::ast::{DeclId, QualType};
use slip_model::Digest;

use crate::keys::{Role, info};
use crate::stream::{FlagWord, Stream};
use crate::tables::{access, ctor_init_kind, init_style, tag_kind};
use crate::units::UnitClass;
use crate::walk::Walk;
use crate::{Area, Cause};

/// The `in` rows of `DefinitionBits`, in table order (§5.5): bit 0 of the
/// flag word is `0-complete`. `0-being-defined` (`out`) and
/// `0-complete-definition-required` (`demand`, §11.8) are not written.
const DEFINITION_BITS: [DefinitionBits; 61] = [
    DefinitionBits::COMPLETE,
    DefinitionBits::ANONYMOUS,
    DefinitionBits::LAMBDA,
    DefinitionBits::AGGREGATE,
    DefinitionBits::POLYMORPHIC,
    DefinitionBits::ABSTRACT,
    DefinitionBits::EMPTY,
    DefinitionBits::STANDARD_LAYOUT,
    DefinitionBits::TRIVIALLY_COPYABLE,
    DefinitionBits::TRIVIAL_DEFAULT_CTOR,
    DefinitionBits::TRIVIAL_COPY_CTOR,
    DefinitionBits::TRIVIAL_MOVE_CTOR,
    DefinitionBits::TRIVIAL_COPY_ASSIGN,
    DefinitionBits::TRIVIAL_MOVE_ASSIGN,
    DefinitionBits::TRIVIAL_DTOR,
    DefinitionBits::LITERAL,
    DefinitionBits::STRUCTURAL,
    DefinitionBits::USER_DECLARED_CTOR,
    DefinitionBits::HAS_CONSTEXPR_CTOR,
    DefinitionBits::HAS_VARIANT_MEMBERS,
    DefinitionBits::HAS_MUTABLE_FIELDS,
    DefinitionBits::HAS_VIRTUAL_BASES,
    DefinitionBits::TRIVIALLY_RELOCATABLE,
    DefinitionBits::USER_DECLARED_COPY_CTOR,
    DefinitionBits::USER_DECLARED_MOVE_CTOR,
    DefinitionBits::USER_DECLARED_COPY_ASSIGN,
    DefinitionBits::USER_DECLARED_MOVE_ASSIGN,
    DefinitionBits::USER_DECLARED_DTOR,
    DefinitionBits::USER_PROVIDED_DEFAULT_CTOR,
    DefinitionBits::HAS_IN_CLASS_INIT,
    DefinitionBits::HAS_INHERITED_CTOR,
    DefinitionBits::HAS_INHERITED_ASSIGN,
    DefinitionBits::IMPLICIT_COPY_CTOR_CONST_PARAM,
    DefinitionBits::IMPLICIT_COPY_ASSIGN_CONST_PARAM,
    DefinitionBits::DEFAULTED_DEFAULT_CTOR_CONSTEXPR,
    DefinitionBits::NEEDS_OVL_COPY_CTOR,
    DefinitionBits::NEEDS_OVL_MOVE_CTOR,
    DefinitionBits::NEEDS_OVL_COPY_ASSIGN,
    DefinitionBits::NEEDS_OVL_MOVE_ASSIGN,
    DefinitionBits::NEEDS_OVL_DTOR,
    DefinitionBits::DEFAULTED_DEFAULT_CTOR_DELETED,
    DefinitionBits::DEFAULTED_COPY_CTOR_DELETED,
    DefinitionBits::DEFAULTED_MOVE_CTOR_DELETED,
    DefinitionBits::DEFAULTED_DTOR_DELETED,
    DefinitionBits::HAS_UNINIT_REF_OR_CONST,
    DefinitionBits::HAS_PRIVATE_FIELDS,
    DefinitionBits::HAS_PROTECTED_FIELDS,
    DefinitionBits::POD,
    DefinitionBits::CAN_CONST_DEFAULT_INIT,
    DefinitionBits::FINAL,
    DefinitionBits::LAMBDA_CAPTURES,
    DefinitionBits::HAS_FLEXIBLE_ARRAY_MEMBER,
    DefinitionBits::TRIVIAL_ABI,
    DefinitionBits::COPY_CTOR_FOR_CALL,
    DefinitionBits::MOVE_CTOR_FOR_CALL,
    DefinitionBits::DTOR_FOR_CALL,
    DefinitionBits::NONTRIVIAL_DEFAULT_CTOR,
    DefinitionBits::NONTRIVIAL_COPY_CTOR,
    DefinitionBits::NONTRIVIAL_MOVE_CTOR,
    DefinitionBits::NONTRIVIAL_COPY_ASSIGN,
    DefinitionBits::NONTRIVIAL_MOVE_ASSIGN,
];

/// `FieldData` `flags-init-style`: bits 2-3 (the row's note; Kiln's
/// `FieldData::init_style` reads the same two bits).
const FIELD_INIT_STYLE_BITS: u32 = 0b11 << 2;

/// `EnumData` `flags`: every bit a row names.
const ENUM_FLAGS: u32 = EnumData::SCOPED
    | EnumData::SCOPED_STRUCT
    | EnumData::FIXED
    | EnumData::BEING_DEFINED
    | EnumData::COMPLETE;

/// How many members of one class are read; a longer list is
/// `Unaccounted("members/limit")` (the walk stays linear in what it reads).
const MEMBER_LIMIT: usize = 1 << 24;

/// `TypedefData`.
pub(crate) fn typedef_data(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    let Some(&TypedefData { redecl: _, underlying, templ }) = w.ast.decls.typedef(d) else {
        s.uv(0).indeterminate(Cause::Unaccounted("TypedefData/missing"));
        return;
    };
    // underlying: canonical, like every type.
    w.write_type(s, underlying);
    crate::templ::templ_link(w, s, templ);
}

/// `EnumData`.
pub(crate) fn enum_data(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    let ast = w.ast;
    let Some(&EnumData { redecl: _, members: _, int_type, promotion_type, flags, templ }) =
        ast.decls.enumeration(d)
    else {
        s.uv(0).indeterminate(Cause::Unaccounted("EnumData/missing"));
        return;
    };
    if flags & !ENUM_FLAGS != 0 {
        s.indeterminate(Cause::Unaccounted("EnumData/flags"));
    }
    let scoped = flags & EnumData::SCOPED != 0;
    if definition_class(w, d) {
        // §11.9: `members`, `int_type`, `promotion_type`, `flags-fixed` and
        // `flags-complete` are in the resolved unit `definition`; here
        // their empty forms. That unit is the demand area's.
        s.count(0).opt(false).opt(false);
        s.flags(FlagWord::new().bit(scoped).bit(false).bit(false));
        s.indeterminate(Cause::Pending(Area::Demand));
        crate::templ::templ_link(w, s, templ);
        return;
    }
    // members: the enumerators, in declaration order; a local enumeration
    // writes them in place (§2.4).
    let local = w.is_local(d);
    let members = members_of(w, s, d);
    let mut enumerators: Vec<DeclId> = Vec::with_capacity(members.len());
    for m in members {
        if ast.decls.get(m).kind() == DeclKind::EnumConstant {
            enumerators.push(m);
        } else if ast.closure_owner(m).is_some() {
            // The closure of a lambda in an enumerator's initialiser:
            // its owner is the enumeration, which numbers no closure
            // (digest-kinds §10.4).
            s.indeterminate(
                crate::local::owner_res(w, m).err().unwrap_or(crate::local::CLOSURE_OWNER),
            );
        } else {
            s.indeterminate(Cause::Unaccounted("EnumData/members"));
        }
    }
    s.count(enumerators.len());
    for &m in &enumerators {
        if local {
            crate::local::write_decl(w, s, m);
        } else {
            w.write_key(s, m);
        }
    }
    w.write_type_opt(s, int_type);
    w.write_type_opt(s, promotion_type);
    // flags-scoped, flags-fixed, flags-complete.
    s.flags(
        FlagWord::new()
            .bit(scoped)
            .bit(flags & EnumData::FIXED != 0)
            .bit(flags & EnumData::COMPLETE != 0),
    );
    crate::templ::templ_link(w, s, templ);
}

/// `EnumeratorData`.
pub(crate) fn enumerator_data(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    let Some(&EnumeratorData { value, init }) = w.ast.decls.enumerator(d) else {
        s.uv(0).indeterminate(Cause::Unaccounted("EnumeratorData/missing"));
        return;
    };
    s.opt(value.is_some());
    if let Some(v) = value {
        crate::value::value(w, s, v);
    }
    crate::expr::child_opt(w, s, init);
}

/// `RecordData`.
pub(crate) fn record_data(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    let ast = w.ast;
    let Some(&RecordData { redecl: _, members: _, bases: _, defn, templ, lambda, tag }) =
        ast.decls.record(d)
    else {
        s.uv(0).indeterminate(Cause::Unaccounted("RecordData/missing"));
        return;
    };
    if definition_class(w, d) {
        // §11.9: `members-fields`, `members-virtual`, `members-other`,
        // `bases` and `defn` are in the resolved unit `definition`; here
        // their empty forms (three empty lists, no base, a flag word with
        // no bit set). That unit is the demand area's.
        s.count(0).count(0).count(0).count(0);
        s.flags(FlagWord::new());
        s.indeterminate(Cause::Pending(Area::Demand));
    } else {
        members(w, s, d);
        bases(w, s, d, tag);
        definition_bits(s, defn);
    }
    crate::templ::templ_link(w, s, templ);
    // lambda: record LambdaData?
    s.opt(lambda.is_some());
    if let Some(l) = lambda {
        crate::local::lambda_data(w, s, l);
    }
    // tag: `class` is written as `struct` (`as Struct`).
    s.tag(tag_kind::row(&tag));
}

/// `FieldData`.
pub(crate) fn field_data(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    let Some(f) = w.ast.decls.field(d) else {
        s.uv(0).indeterminate(Cause::Unaccounted("FieldData/missing"));
        return;
    };
    let style = f.init_style();
    let FieldData { bit_width, init, flags } = *f;
    let known = FieldData::MUTABLE
        | FieldData::NO_UNIQUE_ADDRESS
        | FIELD_INIT_STYLE_BITS
        | FieldData::HAS_DEFAULT_INIT;
    if flags & !known != 0 {
        s.indeterminate(Cause::Unaccounted("FieldData/flags"));
    }
    crate::expr::child_opt(w, s, bit_width);
    // init: §11.4, absent for a field instantiated from a pattern whatever
    // Kiln holds: Kiln instantiates it on use, under any kind of
    // instantiation, and the instantiated initialiser is the derived unit
    // `default_argument` (`demand.rs`).
    if instantiated_field(w, d) {
        s.opt(false);
    } else {
        crate::expr::child_opt(w, s, init);
    }
    // flags-mutable, flags-no-unique-address.
    s.flags(
        FlagWord::new()
            .bit(flags & FieldData::MUTABLE != 0)
            .bit(flags & FieldData::NO_UNIQUE_ADDRESS != 0),
    );
    // flags-init-style; flags-has-default-init is out.
    s.tag(init_style::row(&style));
}

/// `record CtorInit` (§5.5): one constructor initialiser, for
/// `ColdDeclData` `ctor_inits` (the cold-map package calls it). The rows
/// without a kind prefix first (`kind`, `init`), then the `target` row of
/// the kind (§2.1).
#[allow(dead_code, reason = "called by the cold-map package (WP-027), which lands later")]
pub(crate) fn ctor_init(w: &mut Walk<'_>, s: &mut Stream, init: &CtorInit) {
    let ast = w.ast;
    let CtorInit { target, kind, written_index: _, init, loc: _ } = *init;
    s.tag(ctor_init_kind::row(&kind));
    crate::expr::child(w, s, init);
    match kind {
        CtorInitKind::Base | CtorInitKind::VirtualBase => {
            let what = if kind == CtorInitKind::Base {
                "CtorInit/Base.target"
            } else {
                "CtorInit/VirtualBase.target"
            };
            match TypeId::try_new(target).filter(|_| target as usize <= ast.types.len()) {
                Some(t) => w.write_type(s, QualType::unqual(t)),
                None => {
                    s.digest(&Digest([0; 32])).indeterminate(Cause::Unaccounted(what));
                }
            }
        }
        CtorInitKind::Field | CtorInitKind::IndirectField => {
            let (want, what) = if kind == CtorInitKind::Field {
                (DeclKind::Field, "CtorInit/Field.target")
            } else {
                (DeclKind::IndirectField, "CtorInit/IndirectField.target")
            };
            match DeclId::try_new(target).filter(|_| target as usize <= ast.decls.len()) {
                Some(f) if ast.decls.get(f).kind() == want => w.write_ref(s, f),
                Some(_) | None => {
                    s.uv(crate::refs::REF_NONE).indeterminate(Cause::Unaccounted(what));
                }
            }
        }
        CtorInitKind::Delegating => {
            // No `target` row: Kiln stores 0.
            if target != 0 {
                s.indeterminate(Cause::Unaccounted("CtorInit/target"));
            }
        }
    }
}

/// `members-fields`, `members-virtual`, `members-other` of class `d`
/// (§5.5): each member in exactly one list, the lists holding exactly the
/// members that are declared units (`units::unit_class`), so that they
/// agree with the units and do not depend on which implicit members Kiln
/// declared. A local class writes them in place (§2.4).
fn members(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    let ast = w.ast;
    let class = w.entity(d);
    let local = w.is_local(d);
    let mut seen: HashSet<DeclId> = HashSet::new();
    let (mut fields, mut virtuals, mut others) = (Vec::new(), Vec::new(), Vec::new());
    for m in members_of(w, s, d) {
        // A declaration the class holds lexically but that is not a member
        // (`friend struct H;` declares `H` in the namespace).
        if ast.decls.get(m).ctx().map(|c| w.entity(c)) != Some(class) {
            continue;
        }
        let e = w.entity(m);
        if !seen.insert(e) {
            continue;
        }
        let listed = if local {
            local_member(w, e)
        } else {
            crate::units::unit_class(w, e) == Some(UnitClass::Declared)
        };
        if !listed {
            continue;
        }
        // An invalid member makes its class indeterminate too
        // (spec/landed/digest-kinds/WP-027.md).
        if w.redecls(e).iter().any(|&x| ast.decls.get(x).has(DeclFlags::INVALID)) {
            s.indeterminate(Cause::Error);
        }
        let kind = ast.decls.get(e).kind();
        if kind == DeclKind::Field || kind == DeclKind::IndirectField {
            fields.push(m);
        } else if is_virtual(w, e) {
            virtuals.push(m);
        } else {
            others.push(m);
        }
    }
    for list in [&fields, &virtuals] {
        s.count(list.len());
        for &m in list {
            if local {
                crate::local::write_decl(w, s, m);
            } else {
                w.write_key(s, m);
            }
        }
    }
    // members-other of a local class: sorted by key, each written in
    // place in that order (§2.4, `spec/landed/digest/WP-040.md`), so that
    // the ordinals its members' streams take do not follow the order of
    // declaration.
    if local && !crate::local::numbering_mode(w) {
        let mut keyed: Vec<(Vec<u8>, usize, DeclId)> = Vec::with_capacity(others.len());
        for (i, &m) in others.iter().enumerate() {
            keyed.push((w.key_bytes(m).unwrap_or_default(), i, m));
        }
        keyed.sort();
        s.count(keyed.len());
        for (_, _, m) in keyed {
            crate::local::write_decl(w, s, m);
        }
        return;
    }
    // members-other: sorted by encoded bytes, duplicates removed.
    let mut encoded: Vec<Vec<u8>> = Vec::with_capacity(others.len());
    for &m in &others {
        let mut one = Stream::new();
        if local {
            crate::local::write_decl(w, &mut one, m);
        } else {
            w.write_key(&mut one, m);
        }
        s.absorb(one.cause());
        encoded.push(one.as_bytes().to_vec());
    }
    encoded.sort();
    encoded.dedup();
    s.count(encoded.len());
    for e in &encoded {
        s.raw(e);
    }
}

/// The lexical members of `d`, bounded by [`MEMBER_LIMIT`].
fn members_of(w: &Walk<'_>, s: &mut Stream, d: DeclId) -> Vec<DeclId> {
    let mut out = Vec::new();
    for m in w.ast.members(d) {
        if out.len() >= MEMBER_LIMIT {
            s.indeterminate(Cause::Unaccounted("members/limit"));
            break;
        }
        out.push(m);
    }
    out
}

/// A member of a local class that its class lists (§5.5, §2.4): what would
/// be a declared unit if the class were not local. Implicit members are
/// left out, as for a class that is a unit; a using-shadow and an indirect
/// field are written by the declaration that holds them, as in
/// `units::unit_class`.
fn local_member(w: &Walk<'_>, e: DeclId) -> bool {
    let ast = w.ast;
    let kind = ast.decls.get(e).kind();
    // A closure type is never in a member list (§10.4).
    if ast.decls.record(e).is_some_and(|r| r.lambda.is_some()) {
        return false;
    }
    match info(kind).role {
        Role::Entity(_) | Role::Content(_) => {}
        Role::Nothing
        | Role::Transparent
        | Role::Root
        | Role::Local
        | Role::InParent
        | Role::Unsupported(_) => return false,
    }
    let written_by_another = kind == DeclKind::UsingShadow
        || kind == DeclKind::ConstructorUsingShadow
        || kind == DeclKind::IndirectField;
    written_by_another || !w.redecls(e).iter().all(|&x| ast.decls.get(x).has(DeclFlags::IMPLICIT))
}

/// A virtual member function (`FnSpec` `0-virtual`, set for an overrider
/// whether or not `virtual` was written), on any declaration.
fn is_virtual(w: &Walk<'_>, e: DeclId) -> bool {
    let ast = w.ast;
    w.redecls(e).iter().any(|&x| ast.decls.function(x).is_some_and(|f| f.spec.is_virtual()))
}

/// `bases`: `record BaseSpec*` in declaration order.
fn bases(w: &mut Walk<'_>, s: &mut Stream, d: DeclId, tag: TagKind) {
    let ast = w.ast;
    // `access`: the effective access, a stored `None` written as the
    // default of the class key of the definition ([class.access.base]/2).
    let default = match tag {
        TagKind::Class => Access::Private,
        TagKind::Struct | TagKind::Union => Access::Public,
    };
    let list: Vec<BaseSpec> = ast.decls.bases(d, &ast.lists).copied().collect();
    s.count(list.len());
    for b in list {
        base_spec(w, s, b, default);
    }
}

/// One `record BaseSpec`; `default` is the access of a base written
/// without one.
fn base_spec(w: &mut Walk<'_>, s: &mut Stream, b: BaseSpec, default: Access) {
    let BaseSpec { ty, loc: _, is_virtual, access: written, pack_expansion } = b;
    w.write_type(s, ty);
    s.bool(is_virtual);
    let effective = match written {
        Access::None => default,
        a @ (Access::Public | Access::Protected | Access::Private) => a,
    };
    s.tag(access::row(&effective));
    s.bool(pack_expansion);
}

/// `defn`: `record DefinitionBits`, one flag word.
fn definition_bits(s: &mut Stream, defn: DefinitionBits) {
    let mut word = FlagWord::new();
    let mut known =
        DefinitionBits::BEING_DEFINED.with(DefinitionBits::COMPLETE_DEFINITION_REQUIRED);
    for b in DEFINITION_BITS {
        word = word.bit(defn.contains(b));
        known = known.with(b);
    }
    s.flags(word);
    if defn.bits() & !known.bits() != 0 {
        s.indeterminate(Cause::Unaccounted("DefinitionBits/0"));
    }
}

/// The specialisation kind that governs class or enumeration `d`
/// (`templ::governing_kind`), none for a local entity: anything declared
/// in a function body is written in full, whatever function it is in
/// (§11.9).
fn governing_kind(w: &Walk<'_>, d: DeclId) -> Option<SpecKind> {
    if w.is_local(w.entity(d)) {
        return None;
    }
    crate::templ::governing_kind(w, d)
}

/// A class or enumeration of the definition class (§11.9): a
/// specialisation that is itself an implicit instantiation, or a member
/// class or enumeration of one reached through classes only.
fn definition_class(w: &Walk<'_>, d: DeclId) -> bool {
    governing_kind(w, d) == Some(SpecKind::ImplicitInstantiation)
}

/// Field `d` was instantiated from a pattern: its class is governed by an
/// implicit instantiation, an explicit instantiation declaration or an
/// explicit instantiation definition (§11.4,
/// `spec/landed/digest-kinds/WP-027.md`). A field written in source (a
/// primary template, a partial or explicit specialisation, a class that
/// is not a template, a local class) is not.
pub(crate) fn instantiated_field(w: &Walk<'_>, d: DeclId) -> bool {
    let Some(c) = w.ast.decls.get(d).ctx() else { return false };
    match governing_kind(w, c) {
        Some(
            SpecKind::ImplicitInstantiation
            | SpecKind::ExplicitInstantiationDeclaration
            | SpecKind::ExplicitInstantiationDefinition,
        ) => true,
        Some(SpecKind::Undeclared | SpecKind::ExplicitSpecialization) | None => false,
    }
}

#[cfg(test)]
mod tests {
    use slip_kiln::kiln::ast::decl::{CtorInit, CtorInitKind, DeclKind, DefinitionBits};
    use slip_model::keys::RuleTable;

    use super::*;
    use crate::tables::{Decision, definition_bits};
    use crate::{Area, Input};

    /// Runs `f` on a walk over `src` (`main.cpp`).
    fn with_walk<R>(src: &str, f: impl FnOnce(&mut Walk<'_>) -> R) -> R {
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
            let path = |f| p.path(f).map(|s: &str| s.as_bytes().to_vec()).unwrap_or_default();
            let input = Input { ast: p.ast, sm: p.sm, subject_path: &path };
            let mut w = Walk::new(&input, &rules);
            f(&mut w)
        })
        .expect("the snippet parses")
    }

    /// The ctor-initialisers of every constructor, in arena order.
    fn inits(w: &Walk<'_>) -> Vec<CtorInit> {
        let ast = w.ast;
        ast.decls
            .ids()
            .filter(|&d| ast.decls.get(d).kind() == DeclKind::CXXConstructor)
            .flat_map(|d| ast.ctor_inits(d).copied().collect::<Vec<_>>())
            .collect()
    }

    fn bytes(w: &mut Walk<'_>, i: &CtorInit) -> (Vec<u8>, Option<Cause>) {
        let mut s = Stream::new();
        ctor_init(w, &mut s, i);
        (s.as_bytes().to_vec(), s.cause())
    }

    const CTORS: &str = "
        struct B { B(int); };
        struct V { V(int); };
        struct C : B, virtual V {
          int f;
          union { int g; };
          C() : B(1), V(2), f(3), g(4) {}
          C(int) : C() {}
        };
    ";

    // §5.5 `CtorInitKind`, `CtorInit`: each kind is written with its own
    // tag, then `init` (the expression walk's bytes), then the kind's `target`.
    #[test]
    fn ctor_init_writes_each_kind() {
        with_walk(CTORS, |w| {
            let all = inits(w);
            let kinds: Vec<CtorInitKind> = all.iter().map(|i| i.kind).collect();
            for k in [
                CtorInitKind::Base,
                CtorInitKind::VirtualBase,
                CtorInitKind::Field,
                CtorInitKind::IndirectField,
                CtorInitKind::Delegating,
            ] {
                assert!(kinds.contains(&k), "no {k:?} in {kinds:?}");
            }
            let mut seen: Vec<Vec<u8>> = Vec::new();
            for i in &all {
                // `init` is the expression area's: its bytes are whatever
                // that walk writes, taken from the walk itself.
                let ((b, cause), walks) = crate::expr::capture::captured(|| bytes(w, i));
                let init: usize = walks.iter().map(Vec::len).sum();
                assert_eq!(walks.len(), 1, "{:?}: one expression, `init`", i.kind);
                let tag = ctor_init_kind::row(&i.kind).tag;
                assert_eq!(u32::from(b[0]), tag, "{:?} starts with its tag", i.kind);
                // No cause of this record's own: at most a kind of
                // expression that is still pending.
                assert!(
                    matches!(cause, None | Some(Cause::Pending(Area::Expr))),
                    "{:?}: {cause:?}",
                    i.kind
                );
                match i.kind {
                    // tag, `init`, then a type digest.
                    CtorInitKind::Base | CtorInitKind::VirtualBase => {
                        assert_eq!(b.len(), 1 + init + 32);
                    }
                    // tag, `init`, Ref::unit and a key.
                    CtorInitKind::Field | CtorInitKind::IndirectField => {
                        assert_eq!(b[1 + init], 1, "Ref::unit");
                        assert_eq!(b.len(), 1 + init + 1 + 32);
                    }
                    // no target.
                    CtorInitKind::Delegating => assert_eq!(b.len(), 1 + init),
                }
                seen.push(b);
            }
            seen.sort();
            seen.dedup();
            assert_eq!(seen.len(), all.len(), "every initialiser of C is written apart");
        });
    }

    // §5.5 `CtorInit`: `written_index` and `loc` are out; the kind and the
    // target are in (two bases with the same initialiser differ by type).
    #[test]
    fn ctor_init_rows_in_and_out() {
        with_walk(CTORS, |w| {
            let all = inits(w);
            let base = *all.iter().find(|i| i.kind == CtorInitKind::Base).expect("a base");
            let (b0, _) = bytes(w, &base);
            let moved = CtorInit { written_index: CtorInit::IMPLICIT, ..base };
            assert_eq!(bytes(w, &moved).0, b0, "written_index is out");
            let virt = CtorInit { kind: CtorInitKind::VirtualBase, ..base };
            assert_ne!(bytes(w, &virt).0, b0, "kind is in");
            let vb = *all.iter().find(|i| i.kind == CtorInitKind::VirtualBase).expect("vbase");
            let other = CtorInit { target: vb.target, ..base };
            assert_ne!(bytes(w, &other).0, b0, "Base.target is in");
            let field = *all.iter().find(|i| i.kind == CtorInitKind::Field).expect("a field");
            let ind = *all.iter().find(|i| i.kind == CtorInitKind::IndirectField).expect("ind");
            let (f0, _) = bytes(w, &field);
            let as_ind =
                CtorInit { kind: CtorInitKind::IndirectField, target: ind.target, ..field };
            assert_ne!(bytes(w, &as_ind).0, f0, "Field.target, IndirectField.target are in");
        });
    }

    // §5.5 `CtorInit`, common rule 2: a target that names nothing, or a
    // declaration of the wrong kind, is `Unaccounted`, never a panic; a
    // delegating initialiser with a target is `Unaccounted`.
    #[test]
    fn ctor_init_bad_targets() {
        with_walk(CTORS, |w| {
            let all = inits(w);
            let base = *all.iter().find(|i| i.kind == CtorInitKind::Base).expect("a base");
            let field = *all.iter().find(|i| i.kind == CtorInitKind::Field).expect("a field");
            let del = *all.iter().find(|i| i.kind == CtorInitKind::Delegating).expect("deleg");
            for (i, want) in [
                (CtorInit { target: 0, ..base }, "CtorInit/Base.target"),
                (CtorInit { target: u32::MAX, ..base }, "CtorInit/Base.target"),
                (
                    CtorInit { kind: CtorInitKind::VirtualBase, target: 0, ..base },
                    "CtorInit/VirtualBase.target",
                ),
                (CtorInit { target: 0, ..field }, "CtorInit/Field.target"),
                (CtorInit { target: u32::MAX, ..field }, "CtorInit/Field.target"),
                (
                    CtorInit { kind: CtorInitKind::IndirectField, ..field },
                    "CtorInit/IndirectField.target",
                ),
                (CtorInit { target: field.target, ..del }, "CtorInit/target"),
            ] {
                assert_eq!(bytes(w, &i).1, Some(Cause::Unaccounted(want)), "{i:?}");
            }
        });
    }

    // §2.1: the flag word of `DefinitionBits` has one bit per `in` row, in
    // table order; a set bit no row names is `Unaccounted`.
    #[test]
    fn definition_bits_follow_the_table() {
        let rows: Vec<&str> = definition_bits::ROWS
            .iter()
            .filter(|(_, d)| *d == Decision::In)
            .map(|(n, _)| *n)
            .collect();
        assert_eq!(rows.len(), DEFINITION_BITS.len());
        assert_eq!(rows[0], "0-complete");
        assert_eq!(rows[60], "0-nontrivial-move-assign");
        // Kiln's bit n+1 is row n: the bits are Kiln's in order.
        for (n, b) in DEFINITION_BITS.iter().enumerate() {
            assert_eq!(b.bits(), 1 << (n + 1), "row {}", rows[n]);
        }
        let mut s = Stream::new();
        definition_bits(&mut s, DefinitionBits::COMPLETE.with(DefinitionBits::BEING_DEFINED));
        assert_eq!(s.as_bytes(), &[1]);
        assert_eq!(s.cause(), None);
        let mut s = Stream::new();
        definition_bits(&mut s, DefinitionBits::COMPLETE_DEFINITION_REQUIRED);
        assert_eq!(s.as_bytes(), &[0], "0-complete-definition-required is not written");
        let mut s = Stream::new();
        definition_bits(&mut s, DefinitionBits::from_bits(1 << 63));
        assert_eq!(s.cause(), Some(Cause::Unaccounted("DefinitionBits/0")));
    }

    /// Runs `f` on a walk over one side of the pair test in `dir`.
    fn with_side<R>(dir: &std::path::Path, side_b: bool, f: impl FnOnce(&mut Walk<'_>) -> R) -> R {
        let main = std::fs::read_to_string(dir.join("t.cpp")).expect("t.cpp");
        let std = main
            .lines()
            .take(4)
            .find_map(|l| l.strip_prefix("// std:"))
            .map_or("c++20", str::trim)
            .to_owned();
        let side = if side_b { "#define B 1\n" } else { "" };
        let files = [("main.cpp", main.as_str()), ("side.h", side)];
        let snippet = slip_kiln::snippet::Snippet {
            files: &files,
            main: "main.cpp",
            include: &[""],
            std: &std,
        };
        let rules = RuleTable::empty();
        slip_kiln::snippet::parse(&snippet, |p| {
            assert_eq!(p.errors, 0, "{}: {}", dir.display(), p.diagnostics);
            let path = |f| p.path(f).map(|s: &str| s.as_bytes().to_vec()).unwrap_or_default();
            let input = Input { ast: p.ast, sm: p.sm, subject_path: &path };
            let mut w = Walk::new(&input, &rules);
            f(&mut w)
        })
        .expect("the pair test parses")
    }

    /// The `DefinitionBits` of every complete class of one side of the
    /// pair test in `dir`, for `bit`: how many have it set and written.
    fn classes_with(dir: &std::path::Path, side_b: bool, bit: DefinitionBits) -> usize {
        with_side(dir, side_b, |w| {
            w.ast
                .decls
                .ids()
                .filter_map(|d| w.ast.decls.record(d))
                .filter(|r| r.defn.contains(DefinitionBits::COMPLETE) && r.defn.contains(bit))
                .filter(|r| {
                    // The written word holds the bit: clearing it changes
                    // the bytes.
                    let cleared = DefinitionBits::from_bits(r.defn.bits() & !bit.bits());
                    let (mut with, mut without) = (Stream::new(), Stream::new());
                    definition_bits(&mut with, r.defn);
                    definition_bits(&mut without, cleared);
                    with.as_bytes() != without.as_bytes()
                })
                .count()
        })
    }

    /// Every declaration of kind `kind` whose identifier is `name`.
    fn named(w: &Walk<'_>, name: &str, kind: DeclKind) -> Vec<DeclId> {
        use slip_kiln::kiln::ast::names::DeclNameView;
        let ast = w.ast;
        ast.decls
            .ids()
            .filter(|&d| {
                let n = ast.decls.get(d);
                let ident = match ast.names.view(n.name()) {
                    DeclNameView::Identifier(s) => ast.symbols().as_str(s) == name,
                    DeclNameView::Empty
                    | DeclNameView::Operator(_)
                    | DeclNameView::Constructor(_)
                    | DeclNameView::Destructor(_)
                    | DeclNameView::Conversion(_)
                    | DeclNameView::LiteralOperator(_)
                    | DeclNameView::DeductionGuide(_) => false,
                };
                n.kind() == kind && ident
            })
            .collect()
    }

    /// The bytes of the record of every declaration of `kind` named
    /// `name` on one side of the pair test in `dir`, in arena order.
    fn records(
        dir: &std::path::Path,
        side_b: bool,
        kinds: &[DeclKind],
        name: &str,
    ) -> Vec<Vec<u8>> {
        with_side(dir, side_b, |w| {
            let ds: Vec<DeclId> = kinds.iter().flat_map(|&k| named(w, name, k)).collect();
            assert!(!ds.is_empty(), "{}: no {name}", dir.display());
            ds.into_iter()
                .map(|d| {
                    let mut s = Stream::new();
                    let k = w.ast.decls.get(d).kind();
                    if k == DeclKind::CXXRecord {
                        record_data(w, &mut s, d);
                    } else if k == DeclKind::Enum {
                        enum_data(w, &mut s, d);
                    } else if k == DeclKind::EnumConstant {
                        enumerator_data(w, &mut s, d);
                    } else if k == DeclKind::Field {
                        field_data(w, &mut s, d);
                    } else {
                        typedef_data(w, &mut s, d);
                    }
                    s.as_bytes().to_vec()
                })
                .collect()
        })
    }

    // §5.5, common rule 5: each pair test shows its row in the record
    // itself (the record's bytes differ, or are equal for an `equal`
    // test), not only in another row the edit also changed.
    #[test]
    fn pair_tests_show_their_row_in_the_record() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
        let class = &[DeclKind::CXXRecord][..];
        let enumeration = &[DeclKind::Enum][..];
        let typedef = &[DeclKind::Typedef, DeclKind::TypeAlias][..];
        let cases: &[(&str, &[DeclKind], &str, bool)] = &[
            ("differ/BaseSpec/ty", class, "C", true),
            ("differ/BaseSpec/is_virtual", class, "C", true),
            ("differ/BaseSpec/access", class, "C", true),
            ("differ/BaseSpec/pack_expansion", class, "C", true),
            ("differ/RecordData/members-fields", class, "C", true),
            ("differ/RecordData/members-virtual", class, "C", true),
            ("differ/RecordData/members-other", class, "C", true),
            ("differ/RecordData/bases", class, "C", true),
            ("differ/RecordData/defn", class, "C", true),
            ("differ/RecordData/tag", class, "C", true),
            ("differ/TagKind/Struct", class, "C", true),
            ("differ/TagKind/Union", class, "C", true),
            ("differ/_/field-order-against-members", class, "C", true),
            ("differ/FieldData/bit_width", &[DeclKind::Field], "a", true),
            ("differ/FieldData/init", &[DeclKind::Field], "a", true),
            ("differ/FieldData/flags-mutable", &[DeclKind::Field], "a", true),
            ("differ/FieldData/flags-init-style", &[DeclKind::Field], "a", true),
            ("differ/EnumData/members", enumeration, "E", true),
            ("differ/EnumData/int_type", enumeration, "E", true),
            ("differ/EnumData/promotion_type", enumeration, "E", true),
            ("differ/EnumData/flags-scoped", enumeration, "E", true),
            ("differ/EnumData/flags-fixed", enumeration, "E", true),
            ("differ/EnumData/flags-complete", enumeration, "E", true),
            ("differ/EnumeratorData/init", &[DeclKind::EnumConstant], "a", true),
            ("differ/TypedefData/underlying", typedef, "X", true),
            ("differ/_/class-key-default-base-access", class, "C", true),
            ("equal/TagKind/Class", class, "C", false),
            ("equal/EnumData/flags-scoped-struct", enumeration, "E", false),
            ("equal/_/typedef-against-alias", typedef, "X", false),
            ("equal/_/member-functions-swapped", class, "C", false),
            ("equal/_/friends-swapped", class, "C", false),
        ];
        let mut problems = Vec::new();
        for &(dir, kinds, name, differ) in cases {
            let dir = root.join(dir);
            let (a, b) = (records(&dir, false, kinds, name), records(&dir, true, kinds, name));
            if (a != b) != differ {
                problems.push(format!("{}: differ={differ}, a={a:?}, b={b:?}", dir.display()));
            }
        }
        assert!(problems.is_empty(), "{problems:#?}");
    }

    // §5.5 `DefinitionBits`, common rule 5: every pair test of a bit row
    // flips that bit in some class, so that the test shows the row and not
    // only another fact the edit changed.
    #[test]
    fn definition_bit_pairs_flip_their_bit() {
        let rows: Vec<&str> = definition_bits::ROWS
            .iter()
            .filter(|(_, d)| *d == Decision::In)
            .map(|(n, _)| *n)
            .collect();
        let base =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/differ/DefinitionBits");
        let mut checked = 0;
        let mut problems = Vec::new();
        for (row, bit) in rows.iter().zip(DEFINITION_BITS) {
            let dir = base.join(row);
            let expect = std::fs::read_to_string(dir.join("expect.txt")).unwrap_or_default();
            if !dir.is_dir() || expect.lines().any(|l| l.trim() == "# pending") {
                continue;
            }
            checked += 1;
            let (a, b) = (classes_with(&dir, false, bit), classes_with(&dir, true, bit));
            if a == b {
                problems.push(format!("{row}: {a} classes with the bit on both sides"));
            }
        }
        assert!(problems.is_empty(), "{problems:#?}");
        assert!(checked >= 56, "{checked} directories checked");
    }

    /// For every field named `m` with a default member initialiser: how
    /// many write it and how many write it absent (a field instantiated
    /// from a pattern, whose initialiser is a `default_argument` unit's,
    /// with no cause of its own); Kiln holds it in each case it is counted.
    fn field_inits(w: &mut Walk<'_>) -> (usize, usize) {
        let ms: Vec<DeclId> = named(w, "m", DeclKind::Field)
            .into_iter()
            .filter(|&d| {
                w.ast.decls.field(d).is_some_and(|x| x.flags & FieldData::HAS_DEFAULT_INIT != 0)
            })
            .collect();
        let (mut written, mut absent) = (0, 0);
        for f in ms {
            let mut s = Stream::new();
            field_data(w, &mut s, f);
            // No cause, or a kind of expression still pending.
            assert!(matches!(s.cause(), None | Some(Cause::Pending(Area::Expr))), "{f:?}");
            if instantiated_field(w, f) {
                // bit_width absent, then init absent.
                assert_eq!(&s.as_bytes()[..2], &[0, 0]);
                absent += 1;
            } else {
                written += 1;
            }
        }
        (written, absent)
    }

    // §11.4 `FieldData` `init` (with spec/landed/digest-kinds/WP-027.md):
    // absent, with no cause, for a field instantiated from a pattern
    // under any kind of instantiation, even where Kiln has instantiated
    // it; written for a field written in source, a local class of an
    // instantiated function included.
    #[test]
    fn field_init_absent_when_instantiated_from_a_pattern() {
        let cases: &[(&str, (usize, usize))] = &[
            ("template <class T> struct X { int m = 1; X() {} }; X<int> v;", (1, 1)),
            ("template <class T> struct X { int m = 1; }; template struct X<int>;", (1, 1)),
            (
                "template <class T> struct X { int m = 1; void f() {} };
                 extern template struct X<int>; void u(X<int>& x) { x.f(); }",
                (1, 1),
            ),
            (
                "template <class T> struct X { int m = 1; X() {} }; X<int> v0;
                 template struct X<int>;",
                (1, 1),
            ),
            (
                "template <class T> struct X { struct In { int m = 1; In() {} }; };
                 X<int>::In i;",
                (1, 1),
            ),
            (
                "template <class T> struct X { int m = 1; };
                 template <> struct X<int> { int m = 2; }; X<int> v;",
                (2, 0),
            ),
            (
                "template <class T> struct X {}; template <class T> struct X<T*> { int m = 1; };",
                (1, 0),
            ),
            (
                "template <class T> int f() { struct L { int m = 1; int b; }; return L{}.m; }
                 int x = f<int>();",
                (2, 0),
            ),
            (
                "template <class T> struct A {
                   int g() { struct L { int m = 1; int b; }; return L{}.m; }
                 };
                 int y = A<int>().g();",
                (2, 0),
            ),
        ];
        for &(src, want) in cases {
            let got = with_walk(src, field_inits);
            assert_eq!(got, want, "{src}");
        }
    }

    // §11.9: a local class or enumeration is never of the definition
    // class, whatever function it is in: an instantiated function's local
    // class is written in full (the walk stops at a function).
    #[test]
    fn local_types_of_instantiated_functions_are_written_in_full() {
        for src in [
            "template <class T> int f() {
               struct L { int a = 1; int b; }; enum LE { q }; return L{}.a + q;
             }
             int x = f<int>();",
            "template <class T> struct A {
               int g() { struct L { int a = 1; int b; }; enum LE { q }; return L{}.a + q; }
             };
             int y = A<int>().g();",
        ] {
            with_walk(src, |w| {
                let ls = named(w, "L", DeclKind::CXXRecord);
                let les = named(w, "LE", DeclKind::Enum);
                assert_eq!((ls.len(), les.len()), (2, 2), "pattern and instantiation: {src}");
                for d in ls.into_iter().chain(les) {
                    assert!(!definition_class(w, d), "{d:?}");
                    let mut s = Stream::new();
                    if w.ast.decls.get(d).kind() == DeclKind::Enum {
                        enum_data(w, &mut s, d);
                    } else {
                        record_data(w, &mut s, d);
                    }
                    assert_ne!(s.cause(), Some(Cause::Pending(Area::Demand)), "{d:?}");
                    assert_ne!(s.as_bytes()[0], 0, "{d:?}: members written");
                }
            });
        }
    }

    // §11.9: a class or enumeration of the definition class writes the
    // empty forms and is `Pending(Demand)`; an explicit specialisation
    // writes its members.
    #[test]
    fn definition_class_writes_empty_forms() {
        let src = "template <class T> struct X { int a; enum E : int { e }; };
                   template <> struct X<char> { int a; enum E : int { e }; };
                   X<int> v; int n = sizeof(X<int>::E);";
        with_walk(src, |w| {
            let ast = w.ast;
            let mut seen = (0, 0, 0, 0);
            let specs: Vec<DeclId> = ast
                .decls
                .ids()
                .filter(|&d| ast.decls.record(d).is_some() && ast.spec_info(d).is_some())
                .collect();
            for d in specs.into_iter().chain(named(w, "E", DeclKind::Enum)) {
                let mut s = Stream::new();
                let is_enum = ast.decls.get(d).kind() == DeclKind::Enum;
                if is_enum {
                    enum_data(w, &mut s, d);
                } else {
                    record_data(w, &mut s, d);
                }
                if definition_class(w, d) {
                    assert_eq!(s.cause(), Some(Cause::Pending(Area::Demand)), "{d:?}");
                    if is_enum {
                        assert_eq!(&s.as_bytes()[..4], &[0, 0, 0, 0], "{d:?}");
                        seen.1 += 1;
                    } else {
                        assert_eq!(&s.as_bytes()[..5], &[0, 0, 0, 0, 0], "{d:?}");
                        seen.0 += 1;
                    }
                } else {
                    assert_ne!(s.cause(), Some(Cause::Pending(Area::Demand)), "{d:?}");
                    if is_enum { seen.3 += 1 } else { seen.2 += 1 }
                }
            }
            assert!(seen.0 >= 1 && seen.1 >= 1 && seen.2 >= 1 && seen.3 >= 1, "{seen:?}");
        });
    }

    // spec/landed/digest-kinds/WP-027.md: an invalid member makes its
    // class indeterminate with `Error`, the class itself being valid.
    #[test]
    fn invalid_member_makes_its_class_indeterminate() {
        let src = "struct S { int x; void x(); };";
        let files = [("main.cpp", src)];
        let snippet = slip_kiln::snippet::Snippet {
            files: &files,
            main: "main.cpp",
            include: &[""],
            std: "c++20",
        };
        let rules = RuleTable::empty();
        slip_kiln::snippet::parse(&snippet, |p| {
            assert!(p.errors > 0, "the second `x` is an error");
            let path = |f| p.path(f).map(|s: &str| s.as_bytes().to_vec()).unwrap_or_default();
            let input = Input { ast: p.ast, sm: p.sm, subject_path: &path };
            let mut w = Walk::new(&input, &rules);
            let ss = named(&w, "S", DeclKind::CXXRecord);
            assert_eq!(ss.len(), 1);
            let d = ss[0];
            assert!(!w.ast.decls.get(d).has(DeclFlags::INVALID), "S itself is valid");
            assert!(w.ast.members(d).any(|m| w.ast.decls.get(m).has(DeclFlags::INVALID)));
            let mut s = Stream::new();
            record_data(&mut w, &mut s, d);
            assert_eq!(s.cause(), Some(Cause::Error));
        })
        .expect("the snippet parses");
    }

    // §5.5 with §2.4 and §10.4: a local class lists what would be declared
    // units, not an implicit member Kiln declared for a use, and not a
    // closure type.
    #[test]
    fn local_class_lists_no_implicit_member_and_no_closure() {
        let src = "void f() {
                     struct L { int a; int (*p)() = [] { return 0; }; void g() {} };
                     L x; L y = x; y = x;
                   }";
        with_walk(src, |w| {
            let ast = w.ast;
            let ls = named(w, "L", DeclKind::CXXRecord);
            assert_eq!(ls.len(), 1);
            let (mut listed, mut implicit, mut closure) = (0, 0, 0);
            for m in ast.members(ls[0]) {
                let e = w.entity(m);
                let is_closure = ast.decls.record(e).is_some_and(|r| r.lambda.is_some());
                let is_implicit = ast.decls.get(e).has(DeclFlags::IMPLICIT);
                if local_member(w, e) {
                    assert!(!is_closure && !is_implicit, "{:?}", ast.decls.get(e).kind());
                    listed += 1;
                } else {
                    closure += usize::from(is_closure);
                    implicit += usize::from(is_implicit);
                }
            }
            assert_eq!(listed, 3, "a, p, g");
            assert!(closure >= 1 && implicit >= 1, "closure {closure}, implicit {implicit}");
        });
    }

    // §5.5 `BaseSpec`: `ty`, `is_virtual`, `access` and `pack_expansion`
    // are in, each on its own; `loc` is out; `access` is the effective one.
    #[test]
    fn base_spec_rows_in_and_out() {
        with_walk("struct P {}; struct Q {}; struct C : P, Q {};", |w| {
            let ast = w.ast;
            let c = named(w, "C", DeclKind::CXXRecord)[0];
            let list: Vec<BaseSpec> = ast.decls.bases(c, &ast.lists).copied().collect();
            assert_eq!(list.len(), 2);
            let b = list[0];
            let bytes = |w: &mut Walk<'_>, b: BaseSpec, default: Access| {
                let mut s = Stream::new();
                base_spec(w, &mut s, b, default);
                assert_eq!(s.cause(), None);
                s.as_bytes().to_vec()
            };
            let b0 = bytes(w, b, Access::Public);
            assert_eq!(bytes(w, BaseSpec { loc: list[1].loc, ..b }, Access::Public), b0, "loc");
            assert_ne!(bytes(w, BaseSpec { ty: list[1].ty, ..b }, Access::Public), b0, "ty");
            let virt = BaseSpec { is_virtual: !b.is_virtual, ..b };
            assert_ne!(bytes(w, virt, Access::Public), b0, "is_virtual");
            let pack = BaseSpec { pack_expansion: !b.pack_expansion, ..b };
            assert_ne!(bytes(w, pack, Access::Public), b0, "pack_expansion");
            let prot = BaseSpec { access: Access::Protected, ..b };
            assert_ne!(bytes(w, prot, Access::Public), b0, "access");
            let none = BaseSpec { access: Access::None, ..b };
            let public = BaseSpec { access: Access::Public, ..b };
            let private = BaseSpec { access: Access::Private, ..b };
            assert_eq!(bytes(w, none, Access::Public), bytes(w, public, Access::Public));
            assert_eq!(bytes(w, none, Access::Private), bytes(w, private, Access::Public));
        });
    }

    // §5.5 `EnumData`: the pair tests of `int_type` and `promotion_type`
    // change that type only, so that each shows its own row.
    #[test]
    fn enum_type_pairs_change_one_type() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/differ/EnumData");
        let types = |dir: &std::path::Path, b| {
            with_side(dir, b, |w| {
                let e = named(w, "E", DeclKind::Enum)[0];
                let data = w.ast.decls.enumeration(e).expect("EnumData");
                (data.int_type, data.promotion_type)
            })
        };
        for (row, int_changes) in [("int_type", true), ("promotion_type", false)] {
            let dir = root.join(row);
            let (a, b) = (types(&dir, false), types(&dir, true));
            assert_eq!(a.0 != b.0, int_changes, "{row}: int_type {a:?} {b:?}");
            assert_eq!(a.1 != b.1, !int_changes, "{row}: promotion_type {a:?} {b:?}");
        }
    }

    // §5.5 `TypedefData` `templ`, `EnumData` `templ`: the record ends with
    // the declaration's `TemplLink` (§9.1), here one that is set: an alias
    // template's pattern, a member enumeration of an implicit
    // instantiation (§11.9 empty forms) and an explicit specialisation of
    // a member enumeration (written in full).
    #[test]
    fn templ_ends_the_typedef_and_enum_records() {
        let src = "template <class T> using A = T*;
                   template <class T> struct X { enum E : int { e }; }; X<int> v;
                   template <class T> struct Y { enum E : int; };
                   template <> enum Y<int>::E : int { z };";
        with_walk(src, |w| {
            let ast = w.ast;
            let mut checked = (0, 0);
            let typedefs = named(w, "A", DeclKind::TypeAlias);
            let enums = named(w, "E", DeclKind::Enum);
            for d in typedefs.into_iter().chain(enums) {
                let is_enum = ast.decls.get(d).kind() == DeclKind::Enum;
                let templ = if is_enum {
                    ast.decls.enumeration(d).expect("EnumData").templ
                } else {
                    ast.decls.typedef(d).expect("TypedefData").templ
                };
                if templ.is_none() {
                    continue;
                }
                let mut link = Stream::new();
                crate::templ::templ_link(w, &mut link, templ);
                assert!(link.as_bytes().len() > 1, "a set link");
                let mut s = Stream::new();
                if is_enum {
                    enum_data(w, &mut s, d);
                    checked.1 += 1;
                } else {
                    typedef_data(w, &mut s, d);
                    checked.0 += 1;
                }
                assert!(s.as_bytes().ends_with(link.as_bytes()), "{d:?} ends with templ");
            }
            assert!(checked.0 >= 1 && checked.1 >= 2, "{checked:?}");
        });
    }

    // §5.5 `BaseSpec` `access` (landed WP-027): the class-key pair differs
    // in the bases only, so that the effective access is what it shows.
    #[test]
    fn class_key_pair_differs_in_the_bases_only() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/differ/_/class-key-default-base-access");
        let side = |b| {
            with_side(&dir, b, |w| {
                let c = named(w, "C", DeclKind::CXXRecord)[0];
                let r = *w.ast.decls.record(c).expect("RecordData");
                let mut bases = Stream::new();
                let tag = r.tag;
                let default = match tag {
                    TagKind::Class => Access::Private,
                    TagKind::Struct | TagKind::Union => Access::Public,
                };
                for b in w.ast.decls.bases(c, &w.ast.lists).copied().collect::<Vec<_>>() {
                    base_spec(w, &mut bases, b, default);
                }
                (r.defn, bases.as_bytes().to_vec())
            })
        };
        let (a, b) = (side(false), side(true));
        assert_eq!(a.0, b.0, "DefinitionBits are the same");
        assert_ne!(a.1, b.1, "the bases differ");
    }

    // §2.1: the record rows this file writes are those of the tables.
    #[test]
    fn record_rows_are_the_tables() {
        let names = |t: &[(&'static str, Decision)]| t.iter().map(|(n, _)| *n).collect::<Vec<_>>();
        assert_eq!(
            names(crate::tables::record_data::ROWS),
            [
                "redecl",
                "members-fields",
                "members-virtual",
                "members-other",
                "bases",
                "defn",
                "templ",
                "lambda",
                "tag"
            ]
        );
        assert_eq!(
            names(crate::tables::base_spec::ROWS),
            ["ty", "loc", "is_virtual", "access", "pack_expansion"]
        );
        assert_eq!(
            names(crate::tables::field_data::ROWS),
            [
                "bit_width",
                "init",
                "flags-mutable",
                "flags-no-unique-address",
                "flags-init-style",
                "flags-has-default-init"
            ]
        );
        assert_eq!(
            names(crate::tables::enum_data::ROWS),
            [
                "redecl",
                "members",
                "int_type",
                "promotion_type",
                "flags-scoped",
                "flags-scoped-struct",
                "flags-fixed",
                "flags-being-defined",
                "flags-complete",
                "templ"
            ]
        );
        assert_eq!(names(crate::tables::enumerator_data::ROWS), ["value", "init"]);
        assert_eq!(names(crate::tables::typedef_data::ROWS), ["redecl", "underlying", "templ"]);
        assert_eq!(
            names(crate::tables::ctor_init::ROWS),
            [
                "kind",
                "Base.target",
                "VirtualBase.target",
                "Field.target",
                "IndirectField.target",
                "written_index",
                "init",
                "loc"
            ]
        );
    }
}
