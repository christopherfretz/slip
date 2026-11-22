//! The type digest (`spec/digest.md` §4.2, `spec/digest-kinds.md` §4.1 to
//! §4.3, §4.5, §4.6): `Quals`, `TypeView`, function types, `Nns`,
//! `TemplateName` and `ArgView`.

use slip_kiln::kiln::ast::types::{
    ArgView, ExcSpec, ExprOperand, FuncProto, Nns, NoexceptValue, ParamAbi, Quals, TemplateName,
    TypeFlags, TypeOfOperand, TypeView,
};
use slip_kiln::kiln::ast::{ArgListId, NnsId, QualType, TemplateNameId};
use slip_model::Digest;

use crate::Cause;
use crate::stream::{FlagWord, Stream, TYPE_TAG};
use crate::tables::{
    arg_view, auto_keyword, builtin_type, call_conv, exc_spec, expr_operand, nns, param_abi,
    ref_qual, tag_keyword, template_name, type_of_operand, type_view, unary_transform_kind,
    vector_kind,
};
use crate::walk::{TOO_DEEP, Walk};

/// A type digest and the cause it carries (`spec/digest.md` §10).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TypeRef {
    pub digest: Digest,
    pub cause: Option<Cause>,
}

/// The cause of a variant or field digest-kinds §4.1 says never appears
/// in a canonical type.
const fn sugar(what: &'static str) -> Cause {
    Cause::Unaccounted(what)
}

impl Walk<'_> {
    /// `type`: the type digest of `q`.
    pub(crate) fn write_type(&mut self, s: &mut Stream, q: QualType) {
        let t = self.type_digest(q, false);
        s.digest(&t.digest).absorb(t.cause);
    }

    /// `type?`.
    pub(crate) fn write_type_opt(&mut self, s: &mut Stream, q: Option<QualType>) {
        s.opt(q.is_some());
        if let Some(q) = q {
            self.write_type(s, q);
        }
    }

    /// `type*`, in stored order.
    pub(crate) fn write_types(&mut self, s: &mut Stream, qs: &[QualType]) {
        s.count(qs.len());
        for &q in qs {
            self.write_type(s, q);
        }
    }

    /// The type digest of `q` (digest-kinds §4.1); `no_exc` writes a
    /// function type with the normal form of "no specification" in place
    /// of its own (`DeclNode` `ty-type`). Memoised per parse, or per unit
    /// when the type holds a reference to one of the unit's local
    /// entities (digest §4.2).
    #[allow(
        clippy::many_single_char_names,
        reason = "the names of digest-kinds: s stream, d declaration, e entity, n node, q type"
    )]
    pub(crate) fn type_digest(&mut self, q: QualType, no_exc: bool) -> TypeRef {
        let c = self.ast.types.canonical(q);
        let raw = c.to_raw();
        let memo = if no_exc { &self.type_memo_no_exc } else { &self.type_memo };
        if let Some(t) = memo.get(&raw) {
            return *t;
        }
        let unit_key = u64::from(raw) | (u64::from(no_exc) << 32);
        if let Some(t) = self.unit.types.get(&unit_key) {
            self.unit.local_uses += 1;
            return *t;
        }
        if !self.enter() {
            return TypeRef { digest: Digest([0; 32]), cause: Some(TOO_DEEP) };
        }
        let aborts = self.aborts;
        let uses = self.unit.local_uses;
        let mut s = Stream::new();
        self.type_stream(&mut s, c, no_exc);
        self.leave();
        let h = s.finish(TYPE_TAG);
        let h = self.hashed(h);
        let t = TypeRef { digest: h.digest, cause: h.cause };
        if self.aborts == aborts {
            if self.unit.local_uses == uses {
                if no_exc {
                    self.type_memo_no_exc.insert(raw, t);
                } else {
                    self.type_memo.insert(raw, t);
                }
            } else {
                self.unit.types.insert(unit_key, t);
            }
        }
        t
    }

    /// The stream of canonical type `c`: `Quals`, then `enum TypeView`.
    fn type_stream(&mut self, s: &mut Stream, c: QualType, no_exc: bool) {
        let types = &self.ast.types;
        if types.flags(c.ty()).contains(TypeFlags::CONTAINS_ERROR) {
            s.indeterminate(Cause::Error);
        }
        let q = Quals::from_bits(c.quals_bits());
        s.flags(FlagWord::new().bit(q.is_const()).bit(q.is_volatile()).bit(q.is_restrict()));
        let view = types.view(c.ty());
        self.write_view(s, view, no_exc);
    }

    /// `enum TypeView` (digest-kinds §4.3), every variant destructured
    /// without a rest pattern.
    #[allow(clippy::too_many_lines, reason = "one arm per TypeView variant")]
    fn write_view(&mut self, s: &mut Stream, view: TypeView, no_exc: bool) {
        match view {
            TypeView::Invalid => {
                s.tag(type_view::INVALID).indeterminate(Cause::Unsupported("TypeView/Invalid"));
            }
            TypeView::Builtin(b) => {
                let row = builtin_type::row(&b);
                s.tag(type_view::BUILTIN).tag(row);
                if row == builtin_type::ERROR {
                    s.indeterminate(Cause::Unsupported("BuiltinType/Error"));
                } else if row.decision == crate::tables::Decision::Unsupported {
                    s.indeterminate(Cause::Unsupported("BuiltinType"));
                }
            }
            TypeView::BitInt { signed, width } => {
                s.tag(type_view::BIT_INT).bool(signed).uv(u64::from(width));
            }
            TypeView::DependentBitInt { signed, width } => {
                s.tag(type_view::DEPENDENT_BIT_INT).bool(signed);
                crate::expr_dep::profile(self, s, width);
            }
            TypeView::Pointer(x) => {
                s.tag(type_view::POINTER);
                self.write_type(s, x);
            }
            TypeView::LValueRef(x) => {
                s.tag(type_view::L_VALUE_REF);
                self.write_type(s, x);
            }
            TypeView::RValueRef(x) => {
                s.tag(type_view::R_VALUE_REF);
                self.write_type(s, x);
            }
            TypeView::MemberPointer { pointee, class } => {
                s.tag(type_view::MEMBER_POINTER);
                self.write_type(s, pointee);
                self.write_type(s, QualType::unqual(class));
            }
            TypeView::ConstantArray(a) => {
                let slip_kiln::kiln::ast::types::ConstArray { elem, size, size_expr } = a;
                s.tag(type_view::CONSTANT_ARRAY);
                self.write_type(s, elem);
                s.uv(size);
                if size_expr.is_some() {
                    s.indeterminate(sugar("ConstArray/size_expr"));
                }
            }
            TypeView::IncompleteArray(x) => {
                s.tag(type_view::INCOMPLETE_ARRAY);
                self.write_type(s, x);
            }
            TypeView::VariableArray { elem: _, size: _ } => {
                s.tag(type_view::VARIABLE_ARRAY)
                    .indeterminate(Cause::Unsupported("TypeView/VariableArray"));
            }
            TypeView::DependentSizedArray { elem, size } => {
                s.tag(type_view::DEPENDENT_SIZED_ARRAY);
                self.write_type(s, elem);
                self.write_expr_operand(s, size);
            }
            TypeView::Function(p) => {
                s.tag(type_view::FUNCTION);
                self.write_func_proto(s, &p, no_exc);
            }
            TypeView::Record(d) => {
                s.tag(type_view::RECORD);
                self.write_ref(s, d);
            }
            TypeView::Enum(d) => {
                s.tag(type_view::ENUM);
                self.write_ref(s, d);
            }
            TypeView::UnresolvedUsing(d) => {
                s.tag(type_view::UNRESOLVED_USING);
                self.write_ref(s, d);
            }
            TypeView::Paren(_) => {
                s.tag(type_view::PAREN).indeterminate(sugar("TypeView/Paren"));
            }
            TypeView::Complex(x) => {
                s.tag(type_view::COMPLEX);
                self.write_type(s, x);
            }
            TypeView::Atomic(x) => {
                s.tag(type_view::ATOMIC);
                self.write_type(s, x);
            }
            TypeView::Vector { elem, count, kind } => {
                s.tag(type_view::VECTOR);
                self.write_type(s, elem);
                s.uv(u64::from(count)).tag(vector_kind::row(&kind));
            }
            TypeView::DependentVector { elem, size, kind } => {
                s.tag(type_view::DEPENDENT_VECTOR);
                self.write_type(s, elem);
                self.write_expr_operand(s, size);
                s.tag(vector_kind::row(&kind));
            }
            TypeView::ExtQual { base, word } => {
                s.tag(type_view::EXT_QUAL);
                self.write_type(s, base);
                s.uv(u64::from(word));
            }
            TypeView::DependentAddrSpace { pointee, space } => {
                s.tag(type_view::DEPENDENT_ADDR_SPACE);
                self.write_type(s, pointee);
                self.write_expr_operand(s, space);
            }
            TypeView::Typedef { decl: _, underlying: _ } => {
                s.tag(type_view::TYPEDEF).indeterminate(sugar("TypeView/Typedef"));
            }
            TypeView::Using { shadow: _, underlying: _ } => {
                s.tag(type_view::USING).indeterminate(sugar("TypeView/Using"));
            }
            TypeView::Elaborated { keyword: _, nns: _, named: _ } => {
                s.tag(type_view::ELABORATED).indeterminate(sugar("TypeView/Elaborated"));
            }
            TypeView::QualifiedArray { quals: _, base: _ } => {
                s.tag(type_view::QUALIFIED_ARRAY).indeterminate(sugar("TypeView/QualifiedArray"));
            }
            TypeView::TemplateTypeParm(_) => {
                s.tag(type_view::TEMPLATE_TYPE_PARM)
                    .indeterminate(sugar("TypeView/TemplateTypeParm"));
            }
            TypeView::CanonTemplateTypeParm { depth, index, pack } => {
                s.tag(type_view::CANON_TEMPLATE_TYPE_PARM)
                    .uv(u64::from(depth))
                    .uv(u64::from(index))
                    .bool(pack);
            }
            TypeView::SubstTemplateTypeParm {
                replacement: _,
                associated: _,
                index: _,
                pack_index: _,
            } => {
                s.tag(type_view::SUBST_TEMPLATE_TYPE_PARM)
                    .indeterminate(sugar("TypeView/SubstTemplateTypeParm"));
            }
            TypeView::SubstTemplateTypeParmPack { associated, index, pack } => {
                s.tag(type_view::SUBST_TEMPLATE_TYPE_PARM_PACK);
                self.write_ref(s, associated);
                s.uv(u64::from(index));
                self.write_args(s, pack);
            }
            TypeView::TemplateSpecialization { name, args, underlying, alias } => {
                s.tag(type_view::TEMPLATE_SPECIALIZATION);
                self.write_template_name(s, name);
                self.write_args(s, args);
                if underlying.is_some() {
                    s.indeterminate(sugar("TypeView/TemplateSpecialization.underlying"));
                }
                if alias {
                    s.indeterminate(sugar("TypeView/TemplateSpecialization.alias"));
                }
            }
            TypeView::InjectedClassName { pattern: _, injected: _ } => {
                s.tag(type_view::INJECTED_CLASS_NAME)
                    .indeterminate(sugar("TypeView/InjectedClassName"));
            }
            TypeView::DependentName { keyword, nns, name } => {
                s.tag(type_view::DEPENDENT_NAME).tag(tag_keyword::row(&keyword));
                self.write_nns(s, nns);
                self.write_sym(s, name);
            }
            TypeView::DependentTemplateSpecialization { keyword, nns, name, args } => {
                s.tag(type_view::DEPENDENT_TEMPLATE_SPECIALIZATION).tag(tag_keyword::row(&keyword));
                self.write_nns(s, nns);
                self.write_sym(s, name);
                self.write_args(s, args);
            }
            TypeView::PackExpansion { pattern, num_expansions } => {
                s.tag(type_view::PACK_EXPANSION);
                self.write_type(s, pattern);
                s.opt(num_expansions.is_some());
                if let Some(n) = num_expansions {
                    s.uv(u64::from(n));
                }
            }
            TypeView::PackIndexing { pattern, index, selected } => {
                s.tag(type_view::PACK_INDEXING);
                self.write_type(s, pattern);
                self.write_expr_operand(s, index);
                if selected.is_some() {
                    s.indeterminate(sugar("TypeView/PackIndexing.selected"));
                }
            }
            TypeView::Decltype { expr: _, underlying: _ } => {
                s.tag(type_view::DECLTYPE).indeterminate(sugar("TypeView/Decltype"));
            }
            TypeView::DependentDecltype(p) => {
                s.tag(type_view::DEPENDENT_DECLTYPE);
                crate::expr_dep::profile(self, s, p);
            }
            TypeView::TypeOf { operand, underlying, unqual } => {
                s.tag(type_view::TYPE_OF);
                self.write_type_of_operand(s, operand);
                s.bool(unqual);
                if underlying.is_some() {
                    s.indeterminate(sugar("TypeView/TypeOf.underlying"));
                }
            }
            TypeView::UnaryTransform { kind, base, result } => {
                s.tag(type_view::UNARY_TRANSFORM).tag(unary_transform_kind::row(&kind));
                self.write_type(s, base);
                if result.is_some() {
                    s.indeterminate(sugar("TypeView/UnaryTransform.result"));
                }
            }
            TypeView::Auto { keyword, deduced, concept, args, pack } => {
                s.tag(type_view::AUTO).tag(auto_keyword::row(&keyword));
                self.write_ref_opt(s, concept);
                s.opt(args.is_some());
                if let Some(l) = args {
                    self.write_args(s, l);
                }
                s.bool(pack);
                if deduced.is_some() {
                    s.indeterminate(sugar("TypeView/Auto.deduced"));
                }
            }
            TypeView::DeducedTemplateSpecialization { name, deduced } => {
                s.tag(type_view::DEDUCED_TEMPLATE_SPECIALIZATION);
                self.write_template_name(s, name);
                if deduced.is_some() {
                    s.indeterminate(sugar("TypeView/DeducedTemplateSpecialization.deduced"));
                }
            }
            TypeView::Attributed { kind: _, modified: _, equivalent: _ } => {
                s.tag(type_view::ATTRIBUTED).indeterminate(sugar("TypeView/Attributed"));
            }
            TypeView::Splice { operand: _, designated: _ } => {
                s.tag(type_view::SPLICE).indeterminate(Cause::Unsupported("TypeView/Splice"));
            }
        }
    }

    /// `enum ExprOperand`: a canonical type holds `Profile`.
    fn write_expr_operand(&mut self, s: &mut Stream, op: ExprOperand) {
        s.tag(expr_operand::row(&op));
        match op {
            ExprOperand::Expr(_) => {
                s.indeterminate(sugar("ExprOperand/Expr"));
            }
            ExprOperand::Profile(p) => crate::expr_dep::profile(self, s, p),
        }
    }

    /// `enum TypeOfOperand`: a canonical type holds `Profile`.
    fn write_type_of_operand(&mut self, s: &mut Stream, op: TypeOfOperand) {
        s.tag(type_of_operand::row(&op));
        match op {
            TypeOfOperand::Expr(_) => {
                s.indeterminate(sugar("TypeOfOperand/Expr"));
            }
            TypeOfOperand::Type(_) => {
                s.indeterminate(sugar("TypeOfOperand/Type"));
            }
            TypeOfOperand::Profile(p) => crate::expr_dep::profile(self, s, p),
        }
    }

    /// `record FuncProto` (digest-kinds §4.5); `no_exc` writes the normal
    /// form `None` for the exception specification.
    pub(crate) fn write_func_proto(&mut self, s: &mut Stream, p: &FuncProto, no_exc: bool) {
        let ast = self.ast;
        self.write_type(s, p.ret());
        let params = ast.types.params(p).to_vec();
        self.write_types(s, &params);
        let fi = p.info();
        if fi.no_proto() {
            s.indeterminate(Cause::Unsupported("FuncInfo/0-no-proto"));
        }
        if fi.bits() >> 25 != 0 {
            s.indeterminate(Cause::Unaccounted("FuncInfo/0"));
        }
        let q = fi.this_quals();
        s.flags(
            FlagWord::new()
                .bit(fi.variadic())
                .bit(q.is_const())
                .bit(q.is_volatile())
                .bit(q.is_restrict())
                .bit(fi.noreturn())
                .bit(fi.no_caller_saved_regs()),
        );
        s.tag(ref_qual::row(&fi.ref_qual()));
        s.tag(call_conv::row(&fi.cc()));
        s.uv(fi.regparm().map_or(0, |v| u64::from(v) + 1));
        s.uv(u64::from(fi.sme()));
        if no_exc {
            s.tag(exc_spec::NONE);
        } else {
            let e = ast.types.exc_spec(p);
            self.write_exc_spec(s, e, false);
        }
        let abis = ast.types.param_abi_list(fi);
        s.count(abis.len());
        for abi in abis {
            s.tag(match abi {
                ParamAbi::Ordinary => param_abi::ORDINARY,
                ParamAbi::SwiftContext => param_abi::SWIFT_CONTEXT,
                ParamAbi::SwiftAsyncContext => param_abi::SWIFT_ASYNC_CONTEXT,
                ParamAbi::SwiftErrorResult => param_abi::SWIFT_ERROR_RESULT,
                ParamAbi::SwiftIndirectResult => param_abi::SWIFT_INDIRECT_RESULT,
            });
        }
    }

    /// `enum ExcSpec` in normal form (digest-kinds §4.5). `own` is a
    /// function declaration's own specification (`DeclNode`
    /// `ty-exception-spec`), where `Deferred` makes the unit
    /// indeterminate (§11.2).
    pub(crate) fn write_exc_spec(&mut self, s: &mut Stream, e: ExcSpec, own: bool) {
        match e {
            ExcSpec::None => {
                s.tag(exc_spec::NONE);
            }
            ExcSpec::Nothrow => {
                s.tag(exc_spec::NOTHROW);
            }
            ExcSpec::DynamicNone => {
                s.tag(exc_spec::DYNAMIC_NONE);
            }
            ExcSpec::NothrowAttr => {
                s.tag(exc_spec::NOTHROW_ATTR);
            }
            ExcSpec::BasicNoexcept => {
                s.tag(exc_spec::BASIC_NOEXCEPT);
            }
            ExcSpec::NoexceptExpr { expr: _, value } => match value {
                NoexceptValue::False => {
                    s.tag(exc_spec::NONE);
                }
                NoexceptValue::True => {
                    s.tag(exc_spec::NOTHROW);
                }
                NoexceptValue::Dependent(p) => {
                    s.tag(exc_spec::DEPENDENT_NOEXCEPT);
                    crate::expr_dep::profile(self, s, p);
                }
            },
            ExcSpec::Dynamic(list) => {
                s.tag(exc_spec::DYNAMIC);
                let types = self.ast.types.type_list_types(list).to_vec();
                self.write_types(s, &types);
            }
            ExcSpec::DependentNoexcept(p) => {
                s.tag(exc_spec::DEPENDENT_NOEXCEPT);
                crate::expr_dep::profile(self, s, p);
            }
            ExcSpec::Deferred(d) => {
                s.tag(exc_spec::DEFERRED);
                self.write_ref(s, d);
                if own {
                    s.indeterminate(Cause::Unaccounted("ExcSpec/Deferred"));
                }
            }
        }
    }

    /// `enum Nns`, canonical (digest-kinds §4.6).
    pub(crate) fn write_nns(&mut self, s: &mut Stream, n: NnsId) {
        if !self.enter() {
            s.uv(0).indeterminate(TOO_DEEP);
            return;
        }
        let types = &self.ast.types;
        let view = types.nns_view(types.canonical_nns(n));
        s.tag(nns::row(&view));
        match view {
            Nns::Global => {}
            Nns::Namespace { prefix: _, ns } | Nns::NamespaceAlias { prefix: _, alias: _, ns } => {
                let ns = self.ast.resolve_alias(ns);
                self.write_ref(s, ns);
            }
            Nns::Type { prefix, ty, template_kw: _ } => {
                self.write_nns_opt(s, prefix);
                self.write_type(s, ty);
            }
            Nns::Identifier { prefix, name } => {
                self.write_nns_opt(s, prefix);
                self.write_sym(s, name);
            }
            Nns::Splice(_) => {
                s.indeterminate(Cause::Unsupported("Nns/Splice"));
            }
        }
        self.leave();
    }

    fn write_nns_opt(&mut self, s: &mut Stream, n: Option<NnsId>) {
        s.opt(n.is_some());
        if let Some(n) = n {
            self.write_nns(s, n);
        }
    }

    /// `enum TemplateName`, canonical (digest-kinds §4.6).
    pub(crate) fn write_template_name(&mut self, s: &mut Stream, n: TemplateNameId) {
        if !self.enter() {
            s.uv(0).indeterminate(TOO_DEEP);
            return;
        }
        let types = &self.ast.types;
        let view = types.template_name_view(types.canonical_template_name(n));
        s.tag(template_name::row(&view));
        match view {
            TemplateName::Template(d) => self.write_ref(s, d),
            TemplateName::Qualified { nns: _, template_kw: _, underlying: _ } => {
                s.indeterminate(sugar("TemplateName/Qualified"));
            }
            TemplateName::DependentIdentifier { nns, name } => {
                self.write_nns(s, nns);
                self.write_sym(s, name);
            }
            TemplateName::DependentOperator { nns: _, op: _ } => {
                s.indeterminate(Cause::Unsupported("TemplateName/DependentOperator"));
            }
            TemplateName::TemplateTemplateParm { decl: _, depth: _, index: _, pack: _ } => {
                s.indeterminate(sugar("TemplateName/TemplateTemplateParm"));
            }
            TemplateName::CanonTemplateTemplateParm { depth, index, pack } => {
                s.uv(u64::from(depth)).uv(u64::from(index)).bool(pack);
            }
            TemplateName::SubstTemplateTemplateParm { replacement: _, associated: _, index: _ } => {
                s.indeterminate(sugar("TemplateName/SubstTemplateTemplateParm"));
            }
            TemplateName::SubstTemplateTemplateParmPack { associated, index, args } => {
                self.write_ref(s, associated);
                s.uv(u64::from(index));
                self.write_args(s, args);
            }
            TemplateName::Using { shadow: _, underlying: _ } => {
                s.indeterminate(sugar("TemplateName/Using"));
            }
            TemplateName::Overloaded(set) => {
                let decls = self.ast.types.name_decls(set).to_vec();
                self.write_refs_sorted(s, &decls);
            }
            TemplateName::Splice { expr: _, profile: _ } => {
                s.indeterminate(Cause::Unsupported("TemplateName/Splice"));
            }
            TemplateName::SpliceProfile(_) => {
                s.indeterminate(Cause::Unsupported("TemplateName/SpliceProfile"));
            }
        }
        self.leave();
    }

    /// `enum ArgView*` of the canonical form of list `l` (digest-kinds
    /// §4.6).
    pub(crate) fn write_args(&mut self, s: &mut Stream, l: ArgListId) {
        if !self.enter() {
            s.count(0).indeterminate(TOO_DEEP);
            return;
        }
        let types = &self.ast.types;
        let views: Vec<ArgView> = types
            .args(types.canonical_args(l))
            .iter()
            .map(slip_kiln::kiln::ast::types::TemplateArg::view)
            .collect();
        s.count(views.len());
        for v in views {
            self.write_arg(s, v);
        }
        self.leave();
    }

    /// `enum ArgView` (digest-kinds §4.6) of one argument.
    pub(crate) fn write_arg(&mut self, s: &mut Stream, v: ArgView) {
        match v {
            ArgView::Null => {
                s.tag(arg_view::NULL);
            }
            ArgView::Type(q) => {
                s.tag(arg_view::TYPE);
                self.write_type(s, q);
            }
            ArgView::Value { ty, value } => {
                s.tag(arg_view::VALUE);
                self.write_type(s, ty);
                crate::value::value(self, s, value);
            }
            ArgView::Template(n) => {
                s.tag(arg_view::TEMPLATE);
                self.write_template_name(s, n);
            }
            ArgView::TemplateExpansion { name, num_expansions } => {
                s.tag(arg_view::TEMPLATE_EXPANSION);
                self.write_template_name(s, name);
                s.opt(num_expansions.is_some());
                if let Some(n) = num_expansions {
                    s.uv(u64::from(n));
                }
            }
            ArgView::Expr { expr: _, profile: _ } => {
                s.tag(arg_view::EXPR).indeterminate(sugar("ArgView/Expr"));
            }
            ArgView::Profile(p) => {
                s.tag(arg_view::PROFILE);
                crate::expr_dep::profile(self, s, p);
            }
            ArgView::Pack(l) => {
                s.tag(arg_view::PACK);
                self.write_args(s, l);
            }
        }
    }
}
