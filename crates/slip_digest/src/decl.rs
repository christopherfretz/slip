//! A declaration's stream (`spec/digest-kinds.md` §5.1) and the rows of
//! `DeclNode`, `DeclFlags` and `DeclMisc` (§5.3).

use slip_kiln::kiln::ast::DeclId;
use slip_kiln::kiln::ast::decl::{DeclFlags, DeclKind, FriendKind};
use slip_kiln::kiln::ast::types::{ExcSpec, FuncProto};

use crate::Cause;
use crate::keys::info;
use crate::stream::{FlagWord, Hashed, Stream, UNIT_TAG};
use crate::tables::{Decision, access, decl_kind, friend_kind};
use crate::walk::{TOO_DEEP, Walk};

/// A content-keyed entity whose key needs its own digest
/// (`spec/digest.md` §3.3).
const CONTENT_CYCLE: Cause = Cause::Unaccounted("key/content-cycle");

/// The `DeclMisc` friend field holding 3, which no row accounts for.
const MISC_FRIEND_SHIFT: u32 = 2;
/// The reserved `DeclMisc` bits 5-7, zero since Kiln WP-2477.
const MISC_RESERVED_SHIFT: u32 = 5;

impl Walk<'_> {
    /// A declaration's stream (§5.1): read from `d`, for the entity `e`
    /// (§2.3).
    pub(crate) fn decl_stream(&mut self, s: &mut Stream, d: DeclId, e: DeclId) {
        if !self.enter() {
            s.uv(0).indeterminate(TOO_DEEP);
            return;
        }
        crate::operands::decl(self, d);
        self.decl_node(s, d, e);
        s.mark_decls(e);
        crate::decl_scope::cold(self, s, d);
        crate::attrs::decl_attrs(self, s, d);
        crate::templ::decl_maps(self, s, d);
        crate::other::module_rows(self, s, d);
        self.leave();
    }

    /// The unit digest of a content-keyed entity (`spec/digest.md` §3.3),
    /// which is also its discriminator: written once, with the state of
    /// the unit being written put aside.
    pub(crate) fn content_hash(&mut self, e: DeclId) -> Hashed {
        if let Some(h) = self.content.get(&e) {
            return *h;
        }
        if !self.content_in_progress.insert(e) {
            // Not memoised above this point (`Walk::aborts`).
            self.aborts += 1;
            return Hashed {
                digest: slip_model::Digest([0; 32]),
                cause: Some(CONTENT_CYCLE),
                bytes: 0,
            };
        }
        let aborts = self.aborts;
        let read = crate::units::read_decl(self, e);
        // A unit digest of its own: a fresh unit state, and not inside the
        // key computation that may have asked for it.
        let saved = self.take_unit();
        let types = self.take_types();
        let in_key = std::mem::take(&mut self.in_key);
        // Nor inside the keys being computed: a key under `e` (its
        // closure's call operator, asked for first) is computed again
        // inside, under `local::SELF_KEY`; `content_in_progress` ends a
        // real cycle.
        let keys = std::mem::take(&mut self.keys_in_progress);
        self.unit.local.content = Some(e);
        crate::operands::begin(self, e, Some(read));
        let mut s = Stream::new();
        self.decl_stream(&mut s, read, e);
        crate::own_spelling::write_unit_list(self, &mut s);
        // What rule 1 of the mark met, read when the unit is written
        // (`units::write`).
        let emits = std::mem::take(&mut self.unit.emits);
        // The unit's stream ends before its operand unit's begins (an
        // armed stream module records one stream at a time, `trace.rs`).
        let h = s.finish(UNIT_TAG);
        // Its operand unit (`operands.rs`), read when the unit is written.
        let operands = crate::operands::finish(self);
        self.content_operands.insert(e, operands);
        self.keys_in_progress = keys;
        self.in_key = in_key;
        self.restore_types(types);
        self.unit = saved;
        let h = self.hashed(h);
        self.content_in_progress.remove(&e);
        if self.aborts == aborts {
            self.content.insert(e, h);
        }
        if !emits.is_empty() {
            self.content_emits.insert(e, emits);
        }
        h
    }

    /// The rows of `DeclNode` (§5.3) of `d`, a declaration of entity `e`.
    #[allow(
        clippy::many_single_char_names,
        reason = "the names of digest-kinds: s stream, d declaration, e entity, n node, q type"
    )]
    pub(crate) fn decl_node(&mut self, s: &mut Stream, d: DeclId, e: DeclId) {
        let ast = self.ast;
        let n = ast.decls.get(d);
        let kind = n.kind();
        // kind
        let row = decl_kind::row(&kind);
        s.tag(row);
        if row.decision == Decision::Unsupported {
            s.indeterminate(Cause::Unsupported("DeclKind"));
        }
        let redecls = self.redecls(e);
        if redecls.len() > crate::units::REDECL_LIMIT {
            s.indeterminate(Cause::Unaccounted("redecls/limit"));
        }
        // flags: record DeclFlags.
        if redecls.iter().any(|&x| ast.decls.get(x).has(DeclFlags::INVALID)) {
            s.indeterminate(Cause::Error);
        }
        let implicit = redecls.iter().all(|&x| ast.decls.get(x).has(DeclFlags::IMPLICIT));
        s.flags(FlagWord::new().bit(implicit).bit(n.has(DeclFlags::INJECTED)));
        // misc: record DeclMisc.
        self.decl_misc(s, d, e, &redecls);
        // name, ctx: keyed.
        // ty-type
        let function = info(kind).function || kind == DeclKind::CXXDeductionGuide;
        // A variable's type is the completed one (an array of unknown
        // bound that another declaration completes, WP-026).
        let ty = if ast.decls.var(d).is_some() {
            crate::decl_func::var_type(self, d, e)
        } else {
            n.ty()
        };
        // A function or variable of the definition class whose pattern's
        // declared type holds a placeholder: the type is deduced with the
        // definition, and is the resolved unit `definition`'s (§11.9);
        // its empty form here.
        let moved = (function || ast.decls.var(d).is_some())
            && crate::demand::definition_class(self, e)
            && crate::demand::pattern_holds_placeholder(self, e);
        let ty = if moved { None } else { ty };
        match ty {
            Some(q) => {
                s.opt(true);
                let t = self.type_digest(q, function);
                if kind == DeclKind::ParmVar
                    && ast.decls.param(d).is_some_and(|p| p.original_ty.is_some())
                {
                    // An adjusted parameter type holds the canonical
                    // element or function type: the type as written is
                    // `ParamData` `original_ty`, read there.
                    s.digest(&t.digest).absorb(t.cause);
                } else {
                    self.absorb_type(s, t, q, function.then_some(d));
                }
            }
            None => {
                s.opt(false);
            }
        }
        // ty-exception-spec: functions only.
        if function {
            self.own_exception_spec(s, d);
        }
        // Arguments written in a template's explicit specialisation
        // (digest-kinds §4.4 "Spellings", rule 11).
        crate::own_spelling::explicit_specialisation(self, s, d);
        // An out-of-line member of a partial specialisation whose
        // qualifier's arguments Kiln does not keep (rule 12).
        crate::own_spelling::out_of_line_member(self, s, d);
        self.decl_data(s, d, kind);
    }

    /// `record DeclMisc`, merged over the declarations of the entity as
    /// its rows say.
    fn decl_misc(&mut self, s: &mut Stream, d: DeclId, e: DeclId, redecls: &[DeclId]) {
        let ast = self.ast;
        let misc = ast.decls.get(d).misc();
        for &x in redecls {
            let bits = ast.decls.get(x).misc().bits();
            if (bits >> MISC_FRIEND_SHIFT) & 0b11 == 0b11 {
                s.indeterminate(Cause::Unaccounted("DeclMisc/0-friend"));
            }
            if (bits >> MISC_RESERVED_SHIFT) & 0b111 != 0 {
                s.indeterminate(Cause::Unaccounted("DeclMisc/0-reserved"));
            }
        }
        // Kiln gives a specialisation its template's first declaration's
        // friend kind: that of the template's written declaration, when it
        // has one, not of one an instantiation made first (WP-032).
        let of_written_template = ast.spec_info(e).is_some_and(|(_, i)| {
            let t = self.representative(self.entity(i.template));
            ast.decls.get(t).misc().friend() != FriendKind::Undeclared
        });
        let undeclared = !of_written_template
            && redecls.iter().all(|&x| ast.decls.get(x).misc().friend() == FriendKind::Undeclared);
        let friend = if undeclared { FriendKind::Undeclared } else { FriendKind::None };
        let local_extern = redecls.iter().all(|&x| self.is_local_extern(x));
        // In `definition` for the definition class (§11.9), whose
        // definition Kiln makes on use: its empty form here
        // (`spec/landed/digest-kinds/WP-026.md`).
        let definition = !crate::demand::definition_class(self, e)
            && crate::units::defining_decl(self, e).is_some();
        s.tag(access::row(&misc.access()));
        s.tag(friend_kind::row(&friend));
        s.flags(FlagWord::new().bit(local_extern).bit(misc.name_independent()).bit(definition));
    }

    /// `DeclNode` `ty-exception-spec` (§5.3, §11.2): `enum ExcSpec?`,
    /// absent for a function of the deferrable class, whose specification
    /// is the resolved unit `exception_spec` (`demand.rs`). A member of a
    /// local class has no key, so no resolved unit can hold it: its stream
    /// holds the specification as Kiln holds it, indeterminate
    /// (`local::DEFERRED_EXCEPTION_SPEC`) only while that is still
    /// `Deferred`.
    fn own_exception_spec(&mut self, s: &mut Stream, d: DeclId) {
        let ast = self.ast;
        let local = self.is_local(d);
        if !local && crate::demand::deferrable(self, d) {
            s.opt(false);
            return;
        }
        match self.own_proto(d) {
            Some(p) => {
                s.opt(true);
                let e = ast.types.exc_spec(&p);
                let deferred = matches!(e, ExcSpec::Deferred(_));
                self.write_exc_spec(s, e, !local);
                if local && deferred {
                    s.indeterminate(crate::local::DEFERRED_EXCEPTION_SPEC);
                }
            }
            None => {
                s.opt(false);
            }
        }
    }

    /// The prototype of function declaration `d` that holds its own
    /// exception specification: before C++17 the one `TypeContext::proto`
    /// gives (Kiln records the written one apart, `exc_protos`); from
    /// C++17 on, where `proto` gives the canonical prototype and so the
    /// first spelling's operand, the first function node of `d`'s type as
    /// written.
    pub(crate) fn own_proto(&self, d: DeclId) -> Option<FuncProto> {
        let types = &self.ast.types;
        let q = self.ast.decls.get(d).ty()?;
        let p = types.proto(q).copied();
        let canonical = types.proto_of(types.canonical(q).ty()).copied();
        if p != canonical {
            return p;
        }
        let mut cur = q;
        for _ in 0..256 {
            if let Some(w) = types.proto_of(cur.ty()) {
                // A dynamic specification too: canonically `None` from
                // C++17 on, but Kiln keeps its types and checks an
                // overrider's against them (`check_override_exc_spec`).
                return Some(*w);
            }
            let next = types.desugar_once(cur);
            if next.ty() == cur.ty() {
                break;
            }
            cur = next;
        }
        p
    }

    /// The `data` row of `d`'s kind (§5.3).
    #[allow(clippy::match_same_arms, reason = "one arm per row of §5.3 `DeclNode` `<Kind>.data`")]
    fn decl_data(&mut self, s: &mut Stream, d: DeclId, kind: DeclKind) {
        let ast = self.ast;
        match kind {
            DeclKind::NamespaceAlias | DeclKind::UsingDirective | DeclKind::UsingEnum => {
                match ast.using_target(d) {
                    Some(t) => {
                        if kind == DeclKind::UsingEnum {
                            self.write_ref(s, t);
                        } else {
                            // R2 (WP-141, `ns_refs.rs`): the namespace named
                            // whole.
                            self.write_ns_whole(s, d, ast.resolve_alias(t));
                        }
                    }
                    None => {
                        s.uv(crate::refs::REF_NONE)
                            .indeterminate(Cause::Unaccounted("DeclNode/data-target"));
                    }
                }
                if kind == DeclKind::UsingEnum {
                    crate::own_spelling::using_enum(self, s, d);
                }
            }
            DeclKind::FileScopeAsm => crate::decl_scope::file_scope_asm(self, s, d),
            // The qualifier as written: rule 11 (its key writes the
            // canonical one).
            DeclKind::Using
            | DeclKind::UnresolvedUsingValue
            | DeclKind::UnresolvedUsingTypename => {
                crate::own_spelling::using_qualifier(self, s, d);
            }
            DeclKind::UsingShadow | DeclKind::ConstructorUsingShadow => {}
            DeclKind::Typedef | DeclKind::TypeAlias => crate::decl_class::typedef_data(self, s, d),
            DeclKind::Enum => crate::decl_class::enum_data(self, s, d),
            DeclKind::EnumConstant => crate::decl_class::enumerator_data(self, s, d),
            DeclKind::CXXRecord
            | DeclKind::ClassTemplateSpecialization
            | DeclKind::ClassTemplatePartialSpecialization => {
                crate::decl_class::record_data(self, s, d);
            }
            DeclKind::Field => crate::decl_class::field_data(self, s, d),
            DeclKind::IndirectField => {
                let chain: Vec<DeclId> =
                    ast.lists.iter::<DeclId>(ast.indirect_field_chain(d)).collect();
                self.write_refs(s, &chain);
            }
            DeclKind::Friend => crate::decl_scope::friend_target(self, s, d),
            DeclKind::Function
            | DeclKind::CXXMethod
            | DeclKind::CXXConstructor
            | DeclKind::CXXDestructor
            | DeclKind::CXXConversion
            | DeclKind::CXXDeductionGuide => crate::decl_func::function_data(self, s, d),
            DeclKind::Var
            | DeclKind::VarTemplateSpecialization
            | DeclKind::VarTemplatePartialSpecialization => crate::decl_func::var_data(self, s, d),
            DeclKind::ParmVar => crate::decl_func::param_data(self, s, d),
            DeclKind::ClassTemplate
            | DeclKind::FunctionTemplate
            | DeclKind::VarTemplate
            | DeclKind::TypeAliasTemplate
            | DeclKind::Concept => crate::templ::template_info(self, s, d),
            DeclKind::TemplateTypeParm
            | DeclKind::NonTypeTemplateParm
            | DeclKind::TemplateTemplateParm => crate::templ::template_parm_info(self, s, d),
            DeclKind::Namespace => crate::decl_scope::namespace_data(self, s, d),
            DeclKind::StaticAssert | DeclKind::ConstevalBlock => {
                crate::decl_scope::static_assert_data(self, s, d);
            }
            DeclKind::LifetimeExtendedTemporary => crate::decl_func::let_data(self, s, d),
            // `out`, or 0 in `data`.
            DeclKind::BuiltinTemplate
            | DeclKind::TranslationUnit
            | DeclKind::LinkageSpec
            | DeclKind::Export
            | DeclKind::RequiresExprBody
            | DeclKind::Binding
            | DeclKind::Label
            | DeclKind::ContractResultName
            | DeclKind::AccessSpec
            | DeclKind::Empty
            | DeclKind::UnresolvedUsingIfExists => {}
            // Its value, read with `TemplateTables::param_object_value`.
            DeclKind::TemplateParamObject => crate::templ::param_object(self, s, d),
            // Unsupported kinds: the `kind` row made the unit indeterminate.
            DeclKind::UsingPack
            | DeclKind::FriendTemplate
            | DeclKind::ImplicitParam
            | DeclKind::ImplicitConceptSpecialization => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use slip_kiln::kiln::ast::DeclId;
    use slip_kiln::kiln::ast::decl::{DeclFlags, DeclKind};

    use crate::Cause;
    use crate::decl_func::tests::{named, of_kind, with_walk};
    use crate::stream::{FlagWord, Stream};
    use crate::tables::{Decision, Row, decl_kind};
    use crate::walk::Walk;

    /// The bytes and cause `own_exception_spec` writes for the only
    /// declaration of `kind` inside `k` (a member of a local class).
    fn local_spec(src: &str, kind: DeclKind) -> (Vec<u8>, Option<Cause>) {
        with_walk(src, |w| {
            let ds: Vec<DeclId> = of_kind(w, kind).into_iter().filter(|&d| w.is_local(d)).collect();
            assert_eq!(ds.len(), 1, "{src}");
            let mut s = Stream::new();
            w.own_exception_spec(&mut s, ds[0]);
            (s.as_bytes().to_vec(), s.cause())
        })
    }

    // digest-kinds §11.2 (WP-046 review S2): a member of a local class has
    // no resolved unit; its own stream holds the specification Kiln
    // resolved or was given, and is indeterminate only while Kiln still
    // holds it deferred.
    #[test]
    fn a_local_class_member_holds_its_specification() {
        let dtor = |spec: &str| {
            local_spec(
                &format!("void k() {{ struct L {{ ~L() {spec} {{}} }}; L l; }}\n"),
                DeclKind::CXXDestructor,
            )
        };
        let (thrown, c1) = dtor("noexcept(false)");
        let (none, c2) = dtor("");
        assert_eq!((c1, c2), (None, None));
        assert_ne!(thrown, none);
        let ctor = |spec: &str| {
            local_spec(
                &format!(
                    "struct M {{ M() noexcept(false); }};\n\
                     void k() {{ struct L {{ M m; L() {spec} = default; }}; L l; }}\n"
                ),
                DeclKind::CXXConstructor,
            )
        };
        let (written, c1) = ctor("noexcept");
        let (computed, c2) = ctor("");
        assert_eq!((c1, c2), (None, None));
        assert_ne!(written, computed);
        // Never used: Kiln still holds it deferred.
        let (_, cause) = local_spec(
            "struct M { M() noexcept(false); };\nvoid k() { struct L { M m; L() = default; }; }\n",
            DeclKind::CXXConstructor,
        );
        assert_eq!(cause, Some(crate::local::DEFERRED_EXCEPTION_SPEC));
    }

    /// The tag `spec/digest-kinds.md` §5.3 gives a declaration of kind
    /// `k` (`TypeAlias` is written as `Typedef`), written out here rather
    /// than read from the generated table, so that a table or a writer
    /// that disagrees with the spec fails; `None` for a kind that makes
    /// no unit (`out`) or that Kiln never builds (`unsupported`).
    fn spec_tag(k: DeclKind) -> Option<u32> {
        match k {
            DeclKind::TranslationUnit
            | DeclKind::LinkageSpec
            | DeclKind::Export
            | DeclKind::Empty
            | DeclKind::AccessSpec
            | DeclKind::RequiresExprBody
            | DeclKind::UsingPack
            | DeclKind::FriendTemplate
            | DeclKind::ImplicitParam
            | DeclKind::ImplicitConceptSpecialization => None,
            DeclKind::Namespace => Some(258),
            DeclKind::NamespaceAlias => Some(259),
            DeclKind::FileScopeAsm => Some(263),
            DeclKind::UsingDirective => Some(264),
            DeclKind::Using => Some(265),
            DeclKind::UsingEnum => Some(266),
            DeclKind::UsingShadow => Some(267),
            DeclKind::ConstructorUsingShadow => Some(268),
            DeclKind::UnresolvedUsingValue => Some(270),
            DeclKind::UnresolvedUsingTypename => Some(271),
            DeclKind::UnresolvedUsingIfExists => Some(272),
            DeclKind::Typedef | DeclKind::TypeAlias => Some(273),
            DeclKind::Enum => Some(275),
            DeclKind::EnumConstant => Some(276),
            DeclKind::CXXRecord => Some(277),
            DeclKind::ClassTemplateSpecialization => Some(278),
            DeclKind::ClassTemplatePartialSpecialization => Some(279),
            DeclKind::Field => Some(280),
            DeclKind::IndirectField => Some(281),
            DeclKind::Friend => Some(283),
            DeclKind::Function => Some(285),
            DeclKind::CXXMethod => Some(286),
            DeclKind::CXXConstructor => Some(287),
            DeclKind::CXXDestructor => Some(288),
            DeclKind::CXXConversion => Some(289),
            DeclKind::CXXDeductionGuide => Some(290),
            DeclKind::Var => Some(291),
            DeclKind::ParmVar => Some(292),
            DeclKind::Binding => Some(294),
            DeclKind::VarTemplateSpecialization => Some(295),
            DeclKind::VarTemplatePartialSpecialization => Some(296),
            DeclKind::ClassTemplate => Some(297),
            DeclKind::FunctionTemplate => Some(298),
            DeclKind::VarTemplate => Some(299),
            DeclKind::TypeAliasTemplate => Some(300),
            DeclKind::Concept => Some(301),
            DeclKind::BuiltinTemplate => Some(302),
            DeclKind::TemplateTypeParm => Some(303),
            DeclKind::NonTypeTemplateParm => Some(304),
            DeclKind::TemplateTemplateParm => Some(305),
            DeclKind::LifetimeExtendedTemporary => Some(308),
            DeclKind::TemplateParamObject => Some(309),
            DeclKind::StaticAssert => Some(310),
            DeclKind::Label => Some(311),
            DeclKind::ContractResultName => Some(312),
            DeclKind::ConstevalBlock => Some(313),
        }
    }

    /// Every kind [`spec_tag`] gives a tag: the 46 `in` rows of §5.3 and
    /// `TypeAlias`.
    const TAGGED: [DeclKind; 47] = [
        DeclKind::Namespace,
        DeclKind::NamespaceAlias,
        DeclKind::FileScopeAsm,
        DeclKind::UsingDirective,
        DeclKind::Using,
        DeclKind::UsingEnum,
        DeclKind::UsingShadow,
        DeclKind::ConstructorUsingShadow,
        DeclKind::UnresolvedUsingValue,
        DeclKind::UnresolvedUsingTypename,
        DeclKind::UnresolvedUsingIfExists,
        DeclKind::Typedef,
        DeclKind::TypeAlias,
        DeclKind::Enum,
        DeclKind::EnumConstant,
        DeclKind::CXXRecord,
        DeclKind::ClassTemplateSpecialization,
        DeclKind::ClassTemplatePartialSpecialization,
        DeclKind::Field,
        DeclKind::IndirectField,
        DeclKind::Friend,
        DeclKind::Function,
        DeclKind::CXXMethod,
        DeclKind::CXXConstructor,
        DeclKind::CXXDestructor,
        DeclKind::CXXConversion,
        DeclKind::CXXDeductionGuide,
        DeclKind::Var,
        DeclKind::ParmVar,
        DeclKind::Binding,
        DeclKind::VarTemplateSpecialization,
        DeclKind::VarTemplatePartialSpecialization,
        DeclKind::ClassTemplate,
        DeclKind::FunctionTemplate,
        DeclKind::VarTemplate,
        DeclKind::TypeAliasTemplate,
        DeclKind::Concept,
        DeclKind::BuiltinTemplate,
        DeclKind::TemplateTypeParm,
        DeclKind::NonTypeTemplateParm,
        DeclKind::TemplateTemplateParm,
        DeclKind::LifetimeExtendedTemporary,
        DeclKind::TemplateParamObject,
        DeclKind::StaticAssert,
        DeclKind::Label,
        DeclKind::ContractResultName,
        DeclKind::ConstevalBlock,
    ];

    /// The kinds with a tag ([`spec_tag`]) that no snippet below makes,
    /// with the reason.
    const UNREACHED: &[(DeclKind, &str)] = &[
        (DeclKind::ClassTemplateSpecialization, KILN_RECORD),
        (DeclKind::ClassTemplatePartialSpecialization, KILN_RECORD),
        (DeclKind::VarTemplateSpecialization, KILN_VAR),
        (DeclKind::VarTemplatePartialSpecialization, KILN_VAR),
    ];

    /// Why a class template specialisation kind is never made.
    const KILN_RECORD: &str = "Kiln builds every class template specialisation, partial or not, \
        as a `CXXRecord` with a `SpecInfo`; no builder of `kiln_ast` makes the kind";
    /// Why a variable template specialisation kind is never made.
    const KILN_VAR: &str = "Kiln builds every variable template specialisation, partial or not, \
        as a `Var` with a `SpecInfo`; no builder of `kiln_ast` makes the kind";

    /// A declaration of every kind a snippet can make.
    const EVERY_KIND: &str = r#"
namespace N { int x; }
namespace NA = N;
asm("nop");
using namespace N;
namespace M { void g(); void g(int); }
using M::g;
namespace Q {}
using Q::absent __attribute__((using_if_exists));
enum class E { a, b };
using enum E;
typedef int T;
using U = int;
struct B { B(int); };
struct D : B { using B::B; };
template <class X> struct Dep : X { using X::v; using typename X::t; };
struct S { int f; union { int u; }; friend void fr(); S(); ~S(); operator int(); void m(); };
template <class X> struct CT { CT(X); };
CT(int) -> CT<int>;
template <> struct CT<char> { CT(char); };
template <class X> struct CT<X *> { CT(X *); };
template <class X> void ft(X);
template <class X> X vt = X();
template <> int vt<int> = 1;
template <class X> int vt<X *> = 2;
template <class X> using AT = X *;
template <class X> concept C = requires (X t) { t; };
template <int I> struct NT {};
template <template <class> class P> struct TT {};
struct P2 { int a, b; };
auto [p, q] = P2{};
static_assert(true);
const int &r = 1;
struct V { int a; };
template <V v> struct NV {};
NV<V{1}> nv;
void lab() { l:; }
int post_(int x) post (res: res > 0);
consteval {}
"#;

    /// The stream of declaration `d` as `decl_node` writes it.
    fn node(w: &mut Walk<'_>, d: DeclId) -> Stream {
        let mut s = Stream::new();
        let e = w.entity(d);
        w.decl_node(&mut s, d, e);
        s
    }

    fn tag(t: u32) -> Row {
        Row { tag: t, decision: Decision::In }
    }

    // WP-039 fix round item 6 (mutant D5): a declaration's stream begins
    // with the spec's tag of its kind, for every kind a snippet makes; no
    // pair test can see that tag, since two declarations of different
    // kinds have different keys.
    #[test]
    fn a_declaration_begins_with_its_kind() {
        let seen = with_walk(EVERY_KIND, |w| {
            let mut seen = BTreeSet::new();
            for d in w.ast.decls.ids() {
                let kind = w.ast.decls.get(d).kind();
                let Some(t) = spec_tag(kind) else { continue };
                let mut want = Stream::new();
                want.tag(tag(t));
                let s = node(w, d);
                assert!(s.as_bytes().starts_with(want.as_bytes()), "{kind:?}: not tag {t}");
                seen.insert(format!("{kind:?}"));
            }
            seen
        });
        let tagged: BTreeSet<String> = TAGGED.iter().map(|k| format!("{k:?}")).collect();
        assert_eq!(tagged.len(), TAGGED.len(), "TAGGED repeats a kind");
        for k in TAGGED {
            assert!(spec_tag(k).is_some(), "{k:?} is in TAGGED without a tag");
        }
        let mut covered = seen.clone();
        for (k, why) in UNREACHED {
            assert!(covered.insert(format!("{k:?}")), "{k:?} is listed unreached ({why}) but made");
        }
        assert_eq!(covered, tagged, "every kind with a tag is made or listed unreached");
    }

    // Mutant D1: `DeclNode` `flags` bit 0 (§5.3 `0-implicit`) is set when
    // every declaration of the entity is implicit, and right after the
    // kind. No pair can flip it alone: an entity all of whose declarations
    // are implicit is a derived or on-demand unit, a written one a
    // declared unit (`units::unit_class`), so the bit follows the unit's
    // class, except for an indirect field, which Kiln always makes
    // implicit. It stays because the row is `in` and a unit class rule
    // may change.
    #[test]
    fn the_implicit_bit_is_written() {
        with_walk("struct S {}; S s; struct W { W(); };", |w| {
            let ctors: Vec<DeclId> = w
                .ast
                .decls
                .ids()
                .filter(|&d| w.ast.decls.get(d).kind() == DeclKind::CXXConstructor)
                .collect();
            let mut checked = [false; 2];
            for d in ctors {
                let implicit = w.ast.decls.get(d).has(DeclFlags::IMPLICIT);
                let mut want = Stream::new();
                want.tag(decl_kind::CXX_CONSTRUCTOR)
                    .flags(FlagWord::new().bit(implicit).bit(false));
                assert!(node(w, d).as_bytes().starts_with(want.as_bytes()), "implicit {implicit}");
                checked[usize::from(implicit)] = true;
            }
            assert_eq!(checked, [true, true], "a written and an implicit constructor");
        });
    }

    // Mutant D3: `DeclMisc` `0-name-independent` (§5.3) is written. It is
    // redundant with the key: Kiln marks a member name-independent from
    // its name `_` and its kind alone (`is_placeholder_name`, in every
    // language mode), and both are in the key (§3.2, and the ordinal of
    // §3.3 `field`), so no pair flips it alone. It stays because the row
    // is `in`, and so that a future Kiln that tells `_` members apart by
    // mode or context changes the digest with it.
    #[test]
    fn the_name_independent_bit_is_written() {
        with_walk("struct S { int _; int a; int b; };", |w| {
            let field = |w: &Walk<'_>, n: &str| named(w, n, DeclKind::Field)[0];
            let (u, a, b) = (field(w, "_"), field(w, "a"), field(w, "b"));
            assert!(w.ast.decls.get(u).misc().name_independent());
            let (u, a, b) = (node(w, u), node(w, a), node(w, b));
            assert_eq!(a.as_bytes(), b.as_bytes(), "two fields alike but for their names");
            assert_ne!(u.as_bytes(), a.as_bytes(), "the bit is in the stream");
        });
    }
}
