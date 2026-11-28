//! Layer S (WP-156 fix round 3): the one place that destructures what Kiln
//! stores of a written type, an argument list and a nested-name-specifier.
//!
//! Every variant of `TypeView`, `ExcSpec`, `ArgView`, `Nns` and
//! `TypeOfOperand` is matched with every field bound to a name (no `..`, no
//! `field: _`), and each bound field is passed to exactly one [`Slot`]. A
//! field Kiln adds does not compile until it has one. The consumer is
//! `operands::scan`, `args` and `scan_nns`; nothing else recurses over a
//! written type.

#![deny(unused_variables)]

use slip_kiln::kiln::ast::types::{
    ArgView, ExcSpec, ExprOperand, Nns, TypeAttrKind, TypeContext, TypeOfOperand, TypeView,
};
use slip_kiln::kiln::ast::{ArgListId, ExprId, NnsId, QualType};

use super::{BIT_INT, DECLTYPE, EXPR_ARG, NOEXCEPT, TYPEOF};
use crate::Cause;

/// Where a [`Slot::Type`] sits in its type: what `operands::function_type`
/// tells apart (a declaration's parameters are read from the parameters).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Part {
    /// Written inside: a pointee, an element, sugar's inner type, a
    /// template argument, a qualifier's type.
    Inner,
    /// A function type's result.
    Result,
    /// A function type's parameter.
    Param,
    /// A type of a dynamic exception specification (`throw(A, B)`).
    Thrown,
}

/// What one field of a written type is (the table of the design's Layer S).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Slot {
    /// A type written inside: descend.
    Type(QualType, Part),
    /// An operand of a site of `spec/digest.md` §2.4: `decltype(e)` (1),
    /// `typeof(e)` (2), an expression template argument (4), `noexcept(e)`
    /// (6).
    Operand(u8, ExprId),
    /// An expression beside a type that is no site of §2.4: an array bound,
    /// a vector size, an address space, a pack index (sites 3 and 5 stay
    /// reserved).
    Bound(ExprOperand),
    /// A written template argument list.
    Args(ArgListId),
    /// The argument list as written of an alias specialisation (site 8).
    AliasArgs(ArgListId),
    /// A written nested-name-specifier.
    Nns(NnsId),
    /// A value Kiln computed from an expression it does not keep.
    Folded(&'static str),
    /// A splice's operand (`[: e :]`).
    Splice,
    /// What Kiln keeps of a pattern's `_BitInt(e)`: a profile, no
    /// expression.
    Unkept(Cause),
    /// What a name denotes or Kiln computed: never descended.
    Denotes(&'static str),
    /// A declaration, keyword, flag or index that is no written syntax.
    Plain(&'static str),
}

/// Passes `_x` as [`Slot::Plain`].
fn plain<T>(f: &mut impl FnMut(Slot), what: &'static str, _x: T) {
    f(Slot::Plain(what));
}

/// Passes `_x` as [`Slot::Denotes`].
fn denotes<T>(f: &mut impl FnMut(Slot), what: &'static str, _x: T) {
    f(Slot::Denotes(what));
}

/// Passes an `Option` of a written type as [`Slot::Type`].
fn ty_opt(f: &mut impl FnMut(Slot), q: Option<QualType>) {
    if let Some(q) = q {
        f(Slot::Type(q, Part::Inner));
    }
}

/// The slots of written type `q`'s own node, in the order a written type
/// is read (pre-order, as `operands::scan` reads them).
#[allow(clippy::too_many_lines, reason = "one arm per TypeView variant, every field bound")]
pub(crate) fn slots(types: &TypeContext, q: QualType, f: &mut impl FnMut(Slot)) {
    use Part::Inner;
    let inner = |f: &mut dyn FnMut(Slot), x: QualType| f(Slot::Type(x, Inner));
    match types.view(q.ty()) {
        TypeView::Invalid => {}
        TypeView::Builtin(b) => plain(f, "Builtin", b),
        TypeView::BitInt { signed, width } => {
            plain(f, "BitInt.signed", signed);
            let _ = width;
            f(Slot::Folded("BitInt.width"));
        }
        TypeView::DependentBitInt { signed, width } => {
            plain(f, "DependentBitInt.signed", signed);
            let _ = width;
            f(Slot::Unkept(BIT_INT));
        }
        TypeView::Pointer(x)
        | TypeView::LValueRef(x)
        | TypeView::RValueRef(x)
        | TypeView::IncompleteArray(x)
        | TypeView::Paren(x)
        | TypeView::Complex(x)
        | TypeView::Atomic(x) => inner(f, x),
        TypeView::MemberPointer { pointee, class } => {
            inner(f, pointee);
            inner(f, QualType::unqual(class));
        }
        TypeView::ConstantArray(a) => {
            let slip_kiln::kiln::ast::types::ConstArray { elem, size, size_expr } = a;
            inner(f, elem);
            denotes(f, "ConstantArray.size", size);
            if let Some(e) = size_expr {
                f(Slot::Bound(ExprOperand::Expr(e)));
            }
        }
        TypeView::VariableArray { elem, size } => {
            inner(f, elem);
            f(Slot::Bound(ExprOperand::Expr(size)));
        }
        TypeView::DependentSizedArray { elem, size } => {
            inner(f, elem);
            f(Slot::Bound(size));
        }
        TypeView::Function(p) => {
            f(Slot::Type(p.ret(), Part::Result));
            for &t in types.params(&p) {
                f(Slot::Type(t, Part::Param));
            }
            plain(f, "Function.info", p.info());
            exc_spec_slots(types, types.exc_spec(&p), f);
        }
        TypeView::Record(d) => plain(f, "Record", d),
        TypeView::Enum(d) => plain(f, "Enum", d),
        TypeView::UnresolvedUsing(d) => plain(f, "UnresolvedUsing", d),
        TypeView::Vector { elem, count, kind } => {
            inner(f, elem);
            let _ = count;
            f(Slot::Folded("Vector.count"));
            plain(f, "Vector.kind", kind);
        }
        TypeView::DependentVector { elem, size, kind } => {
            inner(f, elem);
            f(Slot::Bound(size));
            plain(f, "DependentVector.kind", kind);
        }
        TypeView::ExtQual { base, word } => {
            inner(f, base);
            let _ = word;
            f(Slot::Folded("ExtQual.word"));
        }
        TypeView::DependentAddrSpace { pointee, space } => {
            inner(f, pointee);
            f(Slot::Bound(space));
        }
        TypeView::Typedef { decl, underlying } => {
            plain(f, "Typedef.decl", decl);
            denotes(f, "Typedef.underlying", underlying);
        }
        TypeView::Using { shadow, underlying } => {
            plain(f, "Using.shadow", shadow);
            denotes(f, "Using.underlying", underlying);
        }
        TypeView::Elaborated { keyword, nns, named } => {
            plain(f, "Elaborated.keyword", keyword);
            if let Some(n) = nns {
                f(Slot::Nns(n));
            }
            inner(f, named);
        }
        TypeView::QualifiedArray { quals, base } => {
            plain(f, "QualifiedArray.quals", quals);
            inner(f, QualType::unqual(base));
        }
        TypeView::TemplateTypeParm(d) => plain(f, "TemplateTypeParm", d),
        TypeView::CanonTemplateTypeParm { depth, index, pack } => {
            plain(f, "CanonTemplateTypeParm", (depth, index, pack));
        }
        TypeView::SubstTemplateTypeParm { replacement, associated, index, pack_index } => {
            denotes(f, "SubstTemplateTypeParm.replacement", replacement);
            plain(f, "SubstTemplateTypeParm", (associated, index, pack_index));
        }
        TypeView::SubstTemplateTypeParmPack { associated, index, pack } => {
            plain(f, "SubstTemplateTypeParmPack", (associated, index));
            denotes(f, "SubstTemplateTypeParmPack.pack", pack);
        }
        TypeView::TemplateSpecialization { name, args, underlying, alias } => {
            plain(f, "TemplateSpecialization.name", name);
            f(if alias { Slot::AliasArgs(args) } else { Slot::Args(args) });
            denotes(f, "TemplateSpecialization.underlying", underlying);
        }
        TypeView::InjectedClassName { pattern, injected } => {
            plain(f, "InjectedClassName.pattern", pattern);
            denotes(f, "InjectedClassName.injected", injected);
        }
        TypeView::DependentName { keyword, nns, name } => {
            plain(f, "DependentName", (keyword, name));
            f(Slot::Nns(nns));
        }
        TypeView::DependentTemplateSpecialization { keyword, nns, name, args } => {
            plain(f, "DependentTemplateSpecialization", (keyword, name));
            f(Slot::Nns(nns));
            f(Slot::Args(args));
        }
        TypeView::PackExpansion { pattern, num_expansions } => {
            inner(f, pattern);
            plain(f, "PackExpansion.num_expansions", num_expansions);
        }
        TypeView::PackIndexing { pattern, index, selected } => {
            inner(f, pattern);
            f(Slot::Bound(index));
            denotes(f, "PackIndexing.selected", selected);
        }
        TypeView::Decltype { expr, underlying } => {
            f(Slot::Operand(DECLTYPE, expr));
            denotes(f, "Decltype.underlying", underlying);
        }
        TypeView::DependentDecltype(p) => denotes(f, "DependentDecltype", p),
        TypeView::TypeOf { operand, underlying, unqual } => {
            match operand {
                TypeOfOperand::Expr(e) => f(Slot::Operand(TYPEOF, e)),
                TypeOfOperand::Type(t) => inner(f, t),
                TypeOfOperand::Profile(p) => denotes(f, "TypeOf.operand", p),
            }
            denotes(f, "TypeOf.underlying", underlying);
            plain(f, "TypeOf.unqual", unqual);
        }
        TypeView::UnaryTransform { kind, base, result } => {
            plain(f, "UnaryTransform.kind", kind);
            inner(f, base);
            denotes(f, "UnaryTransform.result", result);
        }
        TypeView::Auto { keyword, deduced, concept, args, pack } => {
            plain(f, "Auto", (keyword, concept, pack));
            denotes(f, "Auto.deduced", deduced);
            if let Some(l) = args {
                f(Slot::Args(l));
            }
        }
        TypeView::DeducedTemplateSpecialization { name, deduced } => {
            plain(f, "DeducedTemplateSpecialization.name", name);
            denotes(f, "DeducedTemplateSpecialization.deduced", deduced);
        }
        TypeView::Attributed { kind, modified, equivalent } => {
            attr_kind_slots(kind, f);
            inner(f, modified);
            denotes(f, "Attributed.equivalent", equivalent);
        }
        TypeView::Splice { operand, designated } => {
            let _ = operand;
            f(Slot::Splice);
            denotes(f, "Splice.designated", designated);
        }
        TypeView::Decayed { original, adjusted } => {
            inner(f, original);
            denotes(f, "Decayed.adjusted", adjusted);
        }
    }
}

/// The slots of a type attribute's kind: an attribute that takes an
/// argument Kiln folds would be [`Slot::Folded`]; none does at the pin.
fn attr_kind_slots(kind: TypeAttrKind, f: &mut impl FnMut(Slot)) {
    match kind {
        TypeAttrKind::Nonnull
        | TypeAttrKind::Nullable
        | TypeAttrKind::NullUnspecified
        | TypeAttrKind::NullableResult
        | TypeAttrKind::NoDeref
        | TypeAttrKind::CallingConv
        | TypeAttrKind::NoReturn
        | TypeAttrKind::LifetimeBound
        | TypeAttrKind::Other => f(Slot::Plain("Attributed.kind")),
    }
}

/// The slots of an exception specification of a written function type.
pub(crate) fn exc_spec_slots(types: &TypeContext, spec: ExcSpec, f: &mut impl FnMut(Slot)) {
    match spec {
        ExcSpec::None
        | ExcSpec::Nothrow
        | ExcSpec::DynamicNone
        | ExcSpec::NothrowAttr
        | ExcSpec::BasicNoexcept => f(Slot::Plain("ExcSpec")),
        ExcSpec::NoexceptExpr { expr, value } => {
            f(Slot::Operand(NOEXCEPT, expr));
            denotes(f, "ExcSpec::NoexceptExpr.value", value);
        }
        ExcSpec::Dynamic(l) => {
            for &t in types.type_list_types(l) {
                f(Slot::Type(t, Part::Thrown));
            }
        }
        ExcSpec::DependentNoexcept(p) => denotes(f, "ExcSpec::DependentNoexcept", p),
        ExcSpec::Deferred(d) => denotes(f, "ExcSpec::Deferred", d),
    }
}

/// The slots of the written template argument list `l`, argument by
/// argument.
pub(crate) fn arg_slots(types: &TypeContext, l: ArgListId, f: &mut impl FnMut(Slot)) {
    for a in types.args(l) {
        match a.view() {
            ArgView::Null => f(Slot::Plain("ArgView::Null")),
            ArgView::Type(t) => ty_opt(f, Some(t)),
            ArgView::Value { ty, value } => denotes(f, "ArgView::Value", (ty, value)),
            ArgView::Template(n) => plain(f, "ArgView::Template", n),
            ArgView::TemplateExpansion { name, num_expansions } => {
                plain(f, "ArgView::TemplateExpansion", (name, num_expansions));
            }
            ArgView::Expr { expr, profile } => {
                f(Slot::Operand(EXPR_ARG, expr));
                denotes(f, "ArgView::Expr.profile", profile);
            }
            ArgView::Profile(p) => denotes(f, "ArgView::Profile", p),
            // R21: a written list holds no pack (Kiln groups a pack's
            // arguments in a converted list only); kept for a list that
            // would. Unreachable at the pin: emptying this arm changes no
            // unit of `differ/_/operands-scan-arms`, whose `vp` (`VT<int,
            // Q<…>>`, an alias pack) and `tt` (`Tup<Ts...>` substituted)
            // are the shapes that could make one.
            ArgView::Pack(p) => f(Slot::Args(p)),
        }
    }
}

/// The slots of nested-name-specifier `n`'s own component.
pub(crate) fn nns_slots(types: &TypeContext, n: NnsId, f: &mut impl FnMut(Slot)) {
    let prefix = |f: &mut dyn FnMut(Slot), p: Option<NnsId>| {
        if let Some(p) = p {
            f(Slot::Nns(p));
        }
    };
    match types.nns_view(n) {
        Nns::Global => f(Slot::Plain("Nns::Global")),
        Nns::Namespace { prefix: p, ns } => {
            prefix(f, p);
            plain(f, "Nns::Namespace.ns", ns);
        }
        Nns::NamespaceAlias { prefix: p, alias, ns } => {
            prefix(f, p);
            plain(f, "Nns::NamespaceAlias", (alias, ns));
        }
        Nns::Type { prefix: p, ty, template_kw } => {
            prefix(f, p);
            ty_opt(f, Some(ty));
            plain(f, "Nns::Type.template_kw", template_kw);
        }
        Nns::Identifier { prefix: p, name } => {
            prefix(f, p);
            plain(f, "Nns::Identifier.name", name);
        }
        Nns::Splice(e) => {
            let _ = e;
            f(Slot::Splice);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use slip_kiln::kiln::ast::{Ast, QualType};

    use super::{Part, Slot, arg_slots, nns_slots, slots};

    /// A slot as the table below spells it.
    fn spell(s: Slot) -> String {
        match s {
            Slot::Type(_, Part::Inner) => "type".to_owned(),
            Slot::Type(_, p) => format!("type:{p:?}"),
            Slot::Operand(site, _) => format!("operand{site}"),
            Slot::Bound(_) => "bound".to_owned(),
            Slot::Args(_) => "args".to_owned(),
            Slot::AliasArgs(_) => "alias-args".to_owned(),
            Slot::Nns(_) => "nns".to_owned(),
            Slot::Folded(w) => format!("folded:{w}"),
            Slot::Splice => "splice".to_owned(),
            Slot::Unkept(c) => format!("unkept:{c:?}"),
            Slot::Denotes(w) => format!("denotes:{w}"),
            Slot::Plain(w) => format!("plain:{w}"),
        }
    }

    /// The variant's name of `t`'s node.
    fn variant(ast: &Ast, q: QualType) -> String {
        let v = format!("{:?}", ast.types.view(q.ty()));
        v.split(['(', ' ', '{']).next().unwrap_or_default().to_owned()
    }

    /// Every written type reachable through [`slots`] from `q`, by variant:
    /// the slots its nodes give, each once.
    fn reach(ast: &Ast, q: QualType, out: &mut BTreeMap<String, BTreeSet<String>>, depth: u32) {
        if depth > 64 {
            return;
        }
        let mut got = Vec::new();
        slots(&ast.types, q, &mut |s| got.push(s));
        out.entry(variant(ast, q)).or_default().extend(got.iter().map(|&s| spell(s)));
        for s in got {
            match s {
                Slot::Type(x, _) => reach(ast, x, out, depth + 1),
                Slot::Args(l) | Slot::AliasArgs(l) => {
                    let mut inner = Vec::new();
                    arg_slots(&ast.types, l, &mut |s| inner.push(s));
                    for s in inner {
                        if let Slot::Type(x, _) = s {
                            reach(ast, x, out, depth + 1);
                        }
                    }
                }
                Slot::Nns(n) => {
                    let mut inner = Vec::new();
                    nns_slots(&ast.types, n, &mut |s| inner.push(s));
                    for s in inner {
                        if let Slot::Type(x, _) = s {
                            reach(ast, x, out, depth + 1);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    const SRC: &str = r"
int make(int);
template <class T> struct W { using t = T; template <class U> struct X {}; };
template <class T> using AW = W<T>;
template <int N> struct I {};
template <class T, class U> concept C = true;
struct S { int m; };
enum E { e0 };
int x = 0;
typedef float V4 __attribute__((vector_size(16)));
decltype(make(0)) v1 = 0;
__typeof__(make(0)) v2 = 0;
__typeof__(int) v3 = 0;
int &v5 = x;
int &&v6 = 0;
int S::*v7 = nullptr;
int v8[sizeof(make(0))];
extern int v9[];
void (*v10)(int) noexcept(sizeof(make(0)) > 0) = nullptr;
struct S v11;
E v12 = e0;
int (*(v13))[2] = nullptr;
_Complex double v14 = 0;
V4 v16;
int __attribute__((address_space(1))) *v17 = nullptr;
W<int>::t v18 = 0;
I<sizeof(make(0))> v20;
AW<int> v21;
_BitInt(8) v22 = 0;
int *_Nonnull v23 = nullptr;
_Atomic(int) v24 = 0;
namespace ns { struct U {}; }
using ns::U;
U v25;
template <class T, int N, class... Ts> struct P {
  T a1;
  typename T::type a2;
  typename T::template X<int> a3;
  char a4[N];
  int a5 __attribute__((vector_size(N * 4)));
  _BitInt(N) a6;
  P *a8;
  __underlying_type(T) a10;
  int __attribute__((address_space(N))) *a11;
  void f(Ts... ps);
  template <C<T> auto M> void g();
};
template <class T> void h(T a[sizeof(T)]);
";

    /// Layer S: `slots` reaches every field of each variant a written type
    /// of [`SRC`] has, and gives each its slot (the design's table).
    #[test]
    fn slots_reaches_every_field() {
        let files = [("main.cpp", SRC)];
        let snippet = slip_kiln::snippet::Snippet {
            files: &files,
            main: "main.cpp",
            include: &[""],
            std: "c++20",
        };
        let got = slip_kiln::snippet::parse_with_tables(
            &snippet,
            &slip_kiln::snippet::Extra::default(),
            |p| {
                assert_eq!(p.errors, 0, "{}", p.diagnostics);
                let ast = p.ast;
                let mut out = BTreeMap::new();
                for d in ast.decls.ids() {
                    let n = ast.decls.get(d);
                    let mut qs: Vec<QualType> = n.ty().into_iter().collect();
                    if let Some(t) = ast.decls.typedef(d) {
                        qs.push(t.underlying);
                    }
                    if let Some(p) = ast.decls.param(d).and_then(|p| p.original_ty) {
                        qs.push(p);
                    }
                    for q in qs {
                        reach(ast, q, &mut out, 0);
                    }
                }
                out
            },
        )
        .expect("parses");
        let table: Vec<String> = got
            .iter()
            .map(|(v, s)| format!("{v}: {}", s.iter().cloned().collect::<Vec<_>>().join(" ")))
            .collect();
        let text = table.join("\n");
        assert_eq!(text, EXPECTED.trim(), "the slots of each variant");
    }

    /// Reviewed against the design's table of Layer S (WP-156 fix round 3).
    const EXPECTED: &str = r#"
Atomic: type
Attributed: denotes:Attributed.equivalent plain:Attributed.kind type
Auto: args denotes:Auto.deduced plain:Auto
BitInt: folded:BitInt.width plain:BitInt.signed
Builtin: plain:Builtin
Complex: type
ConstantArray: bound denotes:ConstantArray.size type
Decayed: denotes:Decayed.adjusted type
Decltype: denotes:Decltype.underlying operand1
DependentAddrSpace: bound type
DependentBitInt: plain:DependentBitInt.signed unkept:Unsupported("kiln/bit-int-width")
DependentName: nns plain:DependentName
DependentSizedArray: bound type
DependentTemplateSpecialization: args nns plain:DependentTemplateSpecialization
DependentVector: bound plain:DependentVector.kind type
Elaborated: nns plain:Elaborated.keyword type
Enum: plain:Enum
ExtQual: folded:ExtQual.word type
Function: denotes:ExcSpec::Deferred denotes:ExcSpec::NoexceptExpr.value operand6 plain:ExcSpec plain:Function.info type:Param type:Result
IncompleteArray: type
InjectedClassName: denotes:InjectedClassName.injected plain:InjectedClassName.pattern
LValueRef: type
MemberPointer: type
PackExpansion: plain:PackExpansion.num_expansions type
Pointer: type
RValueRef: type
Record: plain:Record
SubstTemplateTypeParm: denotes:SubstTemplateTypeParm.replacement plain:SubstTemplateTypeParm
TemplateSpecialization: alias-args args denotes:TemplateSpecialization.underlying plain:TemplateSpecialization.name
TemplateTypeParm: plain:TemplateTypeParm
TypeOf: denotes:TypeOf.underlying operand2 plain:TypeOf.unqual type
Typedef: denotes:Typedef.underlying plain:Typedef.decl
UnaryTransform: denotes:UnaryTransform.result plain:UnaryTransform.kind type
Vector: folded:Vector.count plain:Vector.kind type
"#;
}
