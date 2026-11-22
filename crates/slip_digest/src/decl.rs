//! A declaration's stream (`spec/digest-kinds.md` §5.1) and the rows of
//! `DeclNode`, `DeclFlags` and `DeclMisc` (§5.3).

use slip_kiln::kiln::ast::DeclId;
use slip_kiln::kiln::ast::decl::{DeclFlags, DeclKind, FriendKind};

use crate::keys::info;
use crate::stream::{FlagWord, Hashed, Stream, UNIT_TAG};
use crate::tables::{Decision, access, decl_kind, friend_kind, module_ownership};
use crate::walk::{TOO_DEEP, Walk};
use crate::{Area, Cause};

/// A content-keyed entity whose key needs its own digest
/// (`spec/digest.md` §3.3).
const CONTENT_CYCLE: Cause = Cause::Unaccounted("key/content-cycle");

/// The `DeclMisc` friend field holding 3, which no row accounts for.
const MISC_FRIEND_SHIFT: u32 = 2;
/// The `DeclMisc` module-ownership field (bits 5-7) holding 5 to 7.
const MISC_MODULE_SHIFT: u32 = 5;

impl Walk<'_> {
    /// A declaration's stream (§5.1): read from `d`, for the entity `e`
    /// (§2.3).
    pub(crate) fn decl_stream(&mut self, s: &mut Stream, d: DeclId, e: DeclId) {
        if !self.enter() {
            s.uv(0).indeterminate(TOO_DEEP);
            return;
        }
        self.decl_node(s, d, e);
        crate::decl_data::cold(self, s, d);
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
            return Hashed {
                digest: slip_model::Digest([0; 32]),
                cause: Some(CONTENT_CYCLE),
                bytes: 0,
            };
        }
        let aborts = self.aborts;
        let read = crate::units::read_decl(self, e);
        let saved = std::mem::take(&mut self.unit);
        let mut s = Stream::new();
        self.decl_stream(&mut s, read, e);
        self.unit = saved;
        let h = s.finish(UNIT_TAG);
        let h = self.hashed(h);
        self.content_in_progress.remove(&e);
        if self.aborts == aborts {
            self.content.insert(e, h);
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
        match n.ty() {
            Some(q) => {
                s.opt(true);
                let t = self.type_digest(q, function);
                s.digest(&t.digest).absorb(t.cause);
            }
            None => {
                s.opt(false);
            }
        }
        // ty-exception-spec: functions only.
        if function {
            self.own_exception_spec(s, d);
        }
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
            if (bits >> MISC_MODULE_SHIFT) & 0b111 > 4 {
                s.indeterminate(Cause::Unaccounted("DeclMisc/0-module-ownership"));
            }
        }
        let undeclared =
            redecls.iter().all(|&x| ast.decls.get(x).misc().friend() == FriendKind::Undeclared);
        let friend = if undeclared { FriendKind::Undeclared } else { FriendKind::None };
        let local_extern = redecls.iter().all(|&x| self.is_local_extern(x));
        let first = ast.decls.get(ast.first(e)).misc();
        let definition = ast.definition(e).is_some();
        s.tag(access::row(&misc.access()));
        s.tag(friend_kind::row(&friend));
        s.flags(FlagWord::new().bit(local_extern).bit(misc.name_independent()).bit(definition));
        s.tag(module_ownership::row(&first.module_ownership()));
    }

    /// `DeclNode` `ty-exception-spec` (§5.3, §11.2): `enum ExcSpec?`,
    /// absent for a function of the deferrable class.
    fn own_exception_spec(&mut self, s: &mut Stream, d: DeclId) {
        let ast = self.ast;
        match crate::demand::deferrable(self, d) {
            Some(true) => {
                s.opt(false);
            }
            Some(false) => {
                let proto = ast.decls.get(d).ty().and_then(|q| ast.types.proto(q)).copied();
                match proto {
                    Some(p) => {
                        s.opt(true);
                        let e = ast.types.exc_spec(&p);
                        self.write_exc_spec(s, e, true);
                    }
                    None => {
                        s.opt(false);
                    }
                }
            }
            None => {
                s.opt(true).pending(Area::Demand);
            }
        }
    }

    /// The `data` row of `d`'s kind (§5.3).
    #[allow(clippy::match_same_arms, reason = "one arm per row of §5.3 `DeclNode` `<Kind>.data`")]
    fn decl_data(&mut self, s: &mut Stream, d: DeclId, kind: DeclKind) {
        let ast = self.ast;
        match kind {
            DeclKind::NamespaceAlias | DeclKind::UsingDirective | DeclKind::UsingEnum => {
                match ast.using_target(d) {
                    Some(t) => {
                        let t = if kind == DeclKind::UsingEnum { t } else { ast.resolve_alias(t) };
                        self.write_ref(s, t);
                    }
                    None => {
                        s.uv(crate::refs::REF_NONE)
                            .indeterminate(Cause::Unaccounted("DeclNode/data-target"));
                    }
                }
            }
            DeclKind::FileScopeAsm => crate::decl_data::file_scope_asm(self, s, d),
            DeclKind::Using
            | DeclKind::UsingShadow
            | DeclKind::ConstructorUsingShadow
            | DeclKind::UnresolvedUsingValue
            | DeclKind::UnresolvedUsingTypename => {}
            DeclKind::Typedef | DeclKind::TypeAlias => crate::decl_data::typedef_data(self, s, d),
            DeclKind::Enum => crate::decl_data::enum_data(self, s, d),
            DeclKind::EnumConstant => crate::decl_data::enumerator_data(self, s, d),
            DeclKind::CXXRecord
            | DeclKind::ClassTemplateSpecialization
            | DeclKind::ClassTemplatePartialSpecialization => {
                crate::decl_data::record_data(self, s, d);
            }
            DeclKind::Field => crate::decl_data::field_data(self, s, d),
            DeclKind::IndirectField => {
                let chain: Vec<DeclId> =
                    ast.lists.iter::<DeclId>(ast.indirect_field_chain(d)).collect();
                self.write_refs(s, &chain);
            }
            DeclKind::Friend => crate::decl_data::friend_target(self, s, d),
            DeclKind::Function
            | DeclKind::CXXMethod
            | DeclKind::CXXConstructor
            | DeclKind::CXXDestructor
            | DeclKind::CXXConversion
            | DeclKind::CXXDeductionGuide => crate::decl_data::function_data(self, s, d),
            DeclKind::Var
            | DeclKind::VarTemplateSpecialization
            | DeclKind::VarTemplatePartialSpecialization => crate::decl_data::var_data(self, s, d),
            DeclKind::ParmVar => crate::decl_data::param_data(self, s, d),
            DeclKind::ClassTemplate
            | DeclKind::FunctionTemplate
            | DeclKind::VarTemplate
            | DeclKind::TypeAliasTemplate
            | DeclKind::Concept => crate::templ::template_info(self, s, d),
            DeclKind::TemplateTypeParm
            | DeclKind::NonTypeTemplateParm
            | DeclKind::TemplateTemplateParm => crate::templ::template_parm_info(self, s, d),
            DeclKind::Namespace => crate::decl_data::namespace_data(self, s, d),
            DeclKind::StaticAssert | DeclKind::ConstevalBlock => {
                crate::decl_data::static_assert_data(self, s, d);
            }
            DeclKind::LifetimeExtendedTemporary => crate::decl_data::let_data(self, s, d),
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
