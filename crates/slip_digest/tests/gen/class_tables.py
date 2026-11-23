#!/usr/bin/env python3
"""Generates the tests of WP-107 (the tables Kiln computes for classes,
`spec/digest-kinds.md` §4.7, §10.6, §10.9, §11.7) under
`crates/slip_digest/tests/`.

Run from anywhere: `python3 crates/slip_digest/tests/gen/class_tables.py`.
It writes every directory of the lists below (`t.cpp`, `expect.txt` when
it has a line, and the header `compare.h` where a test asks for it) and
removes nothing; the generated directories are committed, and this script
is not part of the gate.

A `demand` test is a source and the same source with a use added on side
B inside a function `y` (spec/digest-kinds.md §1.5): the use is one of the
constructs that make Kiln fill the table (`CONSTRUCTS`, with the Kiln
site that fills it; `spec/landed/digest/WP-107.md` has the table). Side A
holds nothing that fills it. The other tests are item 1 to 3's pairs.
"""

import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)

# A minimal <compare>: the three category classes Kiln validates
# (`kiln_sema/src/class/compare/mod.rs:96`, `cmp_category_type`): one
# integral data member, each value a constexpr static data member.
COMPARE_H = """namespace std {
struct partial_ordering { int v; static const partial_ordering less, equivalent, greater, unordered; };
inline constexpr partial_ordering partial_ordering::less{-1}, partial_ordering::equivalent{0},
  partial_ordering::greater{1}, partial_ordering::unordered{2};
struct weak_ordering { int v; static const weak_ordering less, equivalent, greater;
  friend constexpr bool operator==(weak_ordering a, int) { return a.v == 0; } };
inline constexpr weak_ordering weak_ordering::less{-1}, weak_ordering::equivalent{0},
  weak_ordering::greater{1};
struct strong_ordering { int v; static const strong_ordering less, equal, equivalent, greater;
  constexpr operator weak_ordering() const { return weak_ordering{v}; } };
inline constexpr strong_ordering strong_ordering::less{-1}, strong_ordering::equal{0},
  strong_ordering::equivalent{0}, strong_ordering::greater{1};
}
"""

SOURCE_LOCATION = (
    "namespace std { struct source_location { struct __impl { const char *_M_file_name;"
    " const char *_M_function_name; unsigned _M_line; unsigned _M_column; }; }; }")

# The constructs that fill each table: (name, pre, use, expect, Kiln site).
# `pre` is on both sides, `use` is side B's body of `y`.
LAYOUT = {
    "offsetof": (
        "struct S { char c; int i; };",
        "unsigned long y() { return __builtin_offsetof(S, i); }",
        "`offsetof` of a field: `field_bit_offset`, kiln_sema `gnu.rs:471`, `:528`"),
    "offsetof-base": (
        "struct A { char a; }; struct S : A { int i; };",
        "unsigned long y() { return __builtin_offsetof(S, a); }",
        "`offsetof` of a base's field: `offsetof_base_steps`, kiln_sema `gnu.rs:507`"),
    "offsetof-vbase": (
        "struct A { char a; }; struct S : virtual A { int i; };",
        "unsigned long y() { return __builtin_offsetof(S, i); }",
        "`offsetof` in a class with a virtual base (conditionally supported): kiln_sema `gnu.rs:528`"),
    "layout-compatible": (
        "struct S { char c; int i; }; struct T { char d; int j; };",
        "bool y() { return __is_layout_compatible(S, T); }",
        "`__is_layout_compatible`: `ensure_record_layout`, kiln_sema `templ/builtins/traits/special.rs:391`, `:405`"),
    "layout-compatible-polymorphic": (
        "struct A { virtual void f() {} int a; }; struct S : A { int i; };",
        "bool y() { return __is_layout_compatible(S, S); }",
        "`__is_layout_compatible` of a class with a primary base: kiln_sema `traits/special.rs:405`"),
    "pointer-interconvertible": (
        "struct S { int i; char c; };",
        "bool y() { return __builtin_is_pointer_interconvertible_with_class(&S::c); }",
        "`__builtin_is_pointer_interconvertible_with_class`: kiln_sema `builtins.rs:2672`"),
    "packed-member-address": (
        "struct __attribute__((packed)) S { char c; int i; };",
        "int *y() { static S s; return &s.i; }",
        "the address of a packed member (`-Waddress-of-packed-member`): kiln_sema `packed.rs:93`, `:117`"),
    "constant-object": (
        "struct S { char c; int i; };",
        "int y() { constexpr S s{1, 2}; return s.i; }",
        "a class object in constant evaluation: the Oven's object type, kiln_oven `ty.rs:1165` through kiln_sema `oven_bridge.rs:125`"),
    "constant-object-polymorphic": (
        "struct S { constexpr S() {} virtual constexpr int f() const { return 1; } int i = 2; };",
        "int y() { constexpr S s; return s.i; }",
        "a polymorphic object in constant evaluation: kiln_oven `ty.rs:1165`"),
    "source-location": (
        SOURCE_LOCATION,
        "const void *y() { return __builtin_source_location(); }",
        "`__builtin_source_location()`: the `__impl` object, kiln_sema `builtins.rs:3766`"),
}

LAYOUT["static-assert-sizeof"] = (
    "struct S { char c; int i; };",
    "void y() { static_assert(sizeof(S) == 8); }",
    "`sizeof` in a constant expression the Oven evaluates (`static_assert`): kiln_oven `compile.rs:4479`")

VIRTUALS = {
    "vtable-use": (
        "struct A { virtual void f() {} int a; }; struct S : A { void f() override {} };",
        "void y() { S s; }",
        "a construction that uses the vtable: `vtable_finals`, kiln_sema `class/virtual/used.rs:147`"),
    "constant-virtual-call": (
        "struct A { virtual constexpr int f() const { return 1; } };"
        " struct S : A { constexpr int f() const override { return 2; } };",
        "int y() { static constexpr S s; constexpr const A &a = s; constexpr int r = a.f(); return r; }",
        "a virtual call in constant evaluation: `final_overrider`, kiln_sema `oven_bridge.rs:174`"),
}

VIRTUALS["constant-dynamic-cast"] = (
    "struct A { virtual constexpr int f() const { return 1; } }; struct S : A { };",
    "bool y() { static constexpr S s; constexpr const A &a = s;"
    " constexpr bool r = dynamic_cast<const S *>(&a) != nullptr; return r; }",
    "a `dynamic_cast` in constant evaluation: the subobject graph, kiln_sema `oven_bridge.rs:178`")
VIRTUALS["constant-member-pointer-call"] = (
    "struct A { virtual constexpr int f() const { return 1; } };"
    " struct S : A { constexpr int f() const override { return 2; } };",
    "int y() { static constexpr S s; constexpr int (A::*p)() const = &A::f;"
    " constexpr int r = (s.*p)(); return r; }",
    "a call through a pointer to a virtual member in constant evaluation: kiln_sema `oven_bridge.rs:174`")

CMP = {
    "builtin-int": ("", "auto y() { return 1 <=> 2; }",
                    "a built-in `<=>` on integers (`strong_ordering`): kiln_sema `class/compare/mod.rs:378`"),
    "builtin-pointer": ("int g[2];", "auto y() { return &g[0] <=> &g[1]; }",
                        "a built-in `<=>` on pointers (`strong_ordering`): kiln_sema `class/compare/mod.rs:307`"),
    "builtin-double": ("", "auto y() { return 1.0 <=> 2.0; }",
                       "a built-in `<=>` on floating values (`partial_ordering`): kiln_sema `class/compare/mod.rs:378`"),
    "synthesized-weak": (
        "struct M { bool operator==(const M &) const; bool operator<(const M &) const; };",
        "auto y() { struct X { M m; std::weak_ordering operator<=>(const X &) const = default; };"
        " return X{} <=> X{}; }",
        "a defaulted `<=>` declared `weak_ordering` over a member without `<=>`: kiln_sema"
        " `class/compare/default.rs:433`, the synthesized comparison `three_way.rs:37`, `:61`"),
    "defaulted-empty": (
        "",
        "auto y() { struct X { auto operator<=>(const X &) const = default; }; return X{} <=> X{}; }",
        "a defaulted `<=>` of a class with no subobject (`strong_ordering::equal`): kiln_sema"
        " `class/compare/synth.rs:214`, `:217`"),
}

TESTS = []


def demand(short, row, kind, construct, extra=None):
    TESTS.append(("demand", short, row, kind, construct, extra))


# §4.7 RecordLayout and LayoutCache, §10.9 Ast `layouts`.
for row, c in [
    ("size", "constant-object"),
    ("align", "layout-compatible"),
    ("preferred_align", "offsetof"),
    ("data_size", "offsetof-base"),
    ("nv_size", "pointer-interconvertible"),
    ("nv_align", "packed-member-address"),
    ("unadjusted_align", "source-location"),
    ("unadjusted_field_align", "constant-object"),
    ("fields", "offsetof"),
    ("bases", "offsetof-base"),
    ("vbases", "offsetof-vbase"),
    ("vptr", "constant-object-polymorphic"),
    ("primary_base", "layout-compatible-polymorphic"),
]:
    demand("RecordLayout", row, LAYOUT, c)
demand("LayoutCache", "map", LAYOUT, "layout-compatible")
demand("Ast", "layouts", LAYOUT, "offsetof")

# §10.6 the subobject graphs and final overriders, §10.9 Ast `virtuals`.
for short, row, c in [
    ("ClassCache", "map", "vtable-use"),
    ("SubobjectId", "0", "constant-virtual-call"),
    ("Subobject", "class", "vtable-use"),
    ("Subobject", "parent", "constant-virtual-call"),
    ("Subobject", "base_index", "vtable-use"),
    ("Subobject", "is_virtual", "constant-virtual-call"),
    ("ClassVirtuals", "subobjects", "vtable-use"),
    ("ClassVirtuals", "overriders", "constant-virtual-call"),
    ("Ast", "virtuals", "vtable-use"),
]:
    demand(short, row, VIRTUALS, c)

# §10.6 the comparison categories, §10.9 Ast `cmp_categories`.
for short, row, c in [
    ("CmpCategories", "infos", "builtin-int"),
    ("CmpCategoryInfo", "kind", "builtin-int"),
    ("CmpCategoryInfo", "record", "builtin-pointer"),
    ("CmpCategoryInfo", "field", "defaulted-empty"),
    ("CmpCategoryInfo", "values", "synthesized-weak"),
    ("CmpCategoryInfo", "ints", "builtin-double"),
    ("CmpCategory", "Partial", "builtin-double"),
    ("CmpCategory", "Weak", "synthesized-weak"),
    ("CmpCategory", "Strong", "builtin-int"),
    ("CmpResult", "Equal", "defaulted-empty"),
    ("CmpResult", "Equivalent", "synthesized-weak"),
    ("CmpResult", "Less", "builtin-double"),
    ("CmpResult", "Greater", "builtin-pointer"),
    ("CmpResult", "Unordered", "builtin-double"),
    ("Ast", "cmp_categories", "builtin-pointer"),
]:
    demand(short, row, CMP, c)


# Constructs no row above needs: `tests/demand/_/<construct>/`.
demand("_", "layout-static-assert-sizeof", LAYOUT, "static-assert-sizeof")
demand("_", "virtuals-constant-dynamic-cast", VIRTUALS, "constant-dynamic-cast")
demand("_", "virtuals-constant-member-pointer-call", VIRTUALS, "constant-member-pointer-call")


# Item 1 to 3's pairs: (root, slug, a, b, expect, pre, post, note).
PAIRS = []


def pair(root, slug, a, b, expect, note, pre="", post="", header=False):
    PAIRS.append((root, slug, a, b, expect, note, pre, post, header))


PACK = ("#pragma pack(push, 1)\n", "#pragma pack(pop)\n")

pair("differ", "layout-pragma-pack", "struct S { char c; int i; };",
     PACK[0] + "struct S { char c; int i; };\n" + PACK[1], ["changed S"],
     "`#pragma pack` around a class changes its layout; the class holds it (`ColdDeclData` `record_pack`).")
pair("differ", "layout-pragma-pack-class-template",
     "template <class T> struct X { char c; T t; };",
     PACK[0] + "template <class T> struct X { char c; T t; };\n" + PACK[1],
     ["changed X", "derived-changed X<int>::<definition>"],
     "`#pragma pack` around a class template's definition reaches its instantiation (Kiln copies"
     " the pattern's, kiln_sema `templ/class.rs:2561`).", post="X<int> x;")
pair("equal", "layout-pragma-pack-at-instantiation", "X<int> x;",
     PACK[0] + "X<int> x;\n" + PACK[1], [],
     "`#pragma pack` where a class template is instantiated does nothing: the pattern's applies"
     " (kiln_sema `templ/class.rs:2557-2561`).", pre="template <class T> struct X { char c; T t; };")
pair("differ", "layout-pragma-pack-member-class-out-of-line", "struct O::S { char c; int i; };",
     PACK[0] + "struct O::S { char c; int i; };\n" + PACK[1], ["changed O::S"],
     "`#pragma pack` at a member class defined out of line.", pre="struct O { struct S; };")
pair("differ", "layout-pragma-ms-struct", "struct S { char a : 4; int b : 4; };",
     "#pragma ms_struct on\nstruct S { char a : 4; int b : 4; };\n#pragma ms_struct off",
     ["changed S"],
     "`#pragma ms_struct on` gives the class an implicit `ms_struct` (kiln_sema `ms_struct.rs:30`).")
pair("differ", "layout-packed-on-earlier-declaration", "",
     "struct __attribute__((packed)) S;", ["changed S"],
     "`packed` on a declaration before the definition applies to the layout"
     " (kiln_abi `layout/props.rs:28`, every redeclaration).",
     post="struct S { char c; int i; };")
pair("differ", "layout-packed-on-friend-in-template",
     "template <class T> struct Y { friend struct S; };",
     "template <class T> struct Y { friend struct __attribute__((packed)) S; };", ["changed S"],
     "`packed` on a friend redeclaration in a class template's pattern applies to the class.",
     pre="struct S;", post="struct S { char c; int i; };")
pair("equal", "layout-packed-after-definition", "",
     "struct __attribute__((packed)) S;", [],
     "`packed` on a redeclaration after the definition is dropped (kiln_sema `after_def.rs:57`):"
     " neither the layout nor a unit changes.", pre="struct S { char c; int i; };")
pair("differ", "layout-unnamed-bit-field", "struct S { char a; char b; };",
     "struct S { char a; int : 0; char b; };", ["changed S", "added S::(unnamed 0)"],
     "an unnamed bit-field moves the next field; it is a member of the class.")
pair("differ", "layout-tail-padding-of-base", "struct P { int i; char c; P() {} };",
     "struct P { int i; char c; P() = default; };", ["changed P", "changed P::P()", "changed S"],
     "a defaulted constructor makes the base POD for layout, whose tail padding a derived class"
     " does not reuse (kiln_abi `layout/props.rs:252`).", post="struct S : P { char d; };")

# The typedef-carried alignment (item 1's wrong equal), written by
# digest-kinds §4.2 rule 9. `static_assert`s hold on both sides in Kiln and
# in clang 19 (x86_64 and aarch64 Linux) unless a note says otherwise.
TD = "typedef int I __attribute__((aligned(8)));"
TDJ = "typedef int J __attribute__((aligned(16)));"
TDK = "typedef int K __attribute__((aligned(8)));"
TDL = "typedef long L __attribute__((aligned(4)));"
R9 = "digest-kinds §4.2 rule 9"

# The rows of `TypeView/Typedef` (§1.5: one test per row).
pair("differ", "TypeView/Typedef", "struct S { char c; int i; };", "struct S { char c; I i; };",
     ["changed S::i"],
     f"a field of an aligned typedef against the type it names ({R9}): the field's type"
     " digest holds the typedef.", pre=TD)
pair("differ", "TypeView/Typedef.decl", "struct S { char c; I i; };", "struct S { char c; K i; };",
     ["changed S::i"],
     f"another aligned typedef of the same alignment ({R9}: the typedef's entity is written, so"
     " a rename maps it by a rule; without one the field differs, a false difference).",
     pre=TD + "\n" + TDK, post="static_assert(alignof(S) == 8 && sizeof(S) == 16);")
pair("differ", "TypeView/Typedef.underlying", "struct S { char c; L a; };",
     "struct S { char c; L a[2]; };", ["changed S::a"],
     f"the same aligned typedef around another canonical type ({R9}).", pre=TDL)

# The ten class pairs that were indeterminate under the class cause.
pair("differ", "typedef-alignment-field", "struct S { char c; I i; };",
     "struct S { char c; J i; };", ["changed S::i"],
     "another aligned typedef of another alignment.", pre=TD + "\n" + TDJ)
pair("differ", "typedef-alignment-alias-template", "struct S { char c; A<int> i; };",
     "struct S { char c; A<I> i; };", ["changed S::i"],
     "an alias template keeps its argument's typedef for layout in Kiln (clang 22; clang 19"
     " makes it canonical, `kiln_ast/src/layout_sugar.rs:47`).",
     pre=TD + "\ntemplate <class T> using A = T;")
pair("differ", "typedef-alignment-pattern", "template <class T> struct X { char c; int i; T t; };",
     "template <class T> struct X { char c; I i; T t; };",
     ["changed X::i"],
     "a class template's field of the typedef. `X<char>::<definition>` is equal: its layout"
     " follows from the pattern's field `X::i`, as a class's layout from its fields.",
     pre=TD, post="X<char> x;")
pair("differ", "typedef-alignment-alignas-class", "struct alignas(int) S { char c; };",
     "struct alignas(I) S { char c; };", ["changed S"],
     "`alignas(type-id)` naming the typedef: the attribute's type holds it.", pre=TD,
     post="static_assert(alignof(S) == (sizeof(S) == 8 ? 8 : 4));")
pair("differ", "typedef-alignment-alignas-field", "struct S { char c; alignas(int) char d; };",
     "struct S { char c; alignas(I) char d; };", ["changed S::d"],
     "`alignas(type-id)` on a field naming the typedef.", pre=TD)
pair("differ", "typedef-alignment-array", "struct S { char c; long a[2]; };",
     "struct S { char c; L a[2]; };", ["changed S::a"],
     "an array of a typedef that lowers alignment (an over-aligned element is an error).",
     pre=TDL, post="static_assert(sizeof(S) == (alignof(S) == 4 ? 20 : 24));")
pair("differ", "typedef-alignment-const-array", "typedef const long CL;\nstruct S { char c; CL a[2]; };",
     "typedef const L CL;\nstruct S { char c; CL a[2]; };", ["changed CL", "changed S::a"],
     "an array of a cv-qualified typedef of the typedef; the plain typedef `CL` holds it too.",
     pre=TDL)
pair("differ", "typedef-alignment-qualified-array-typedef", "typedef long LA[2];",
     "typedef L LA[2];", ["changed LA", "changed S::a"],
     "a cv-qualified array typedef (Kiln's `QualifiedArray` sugar, kiln_ast `types/canon.rs:100`).",
     pre=TDL, post="struct S { char c; const LA a; };\nstatic_assert(sizeof(S) == (alignof(S) == 4 ? 20 : 24));")
pair("differ", "typedef-alignment-flexible-array", "struct S { char c; long a[]; };",
     "struct S { char c; L a[]; };", ["changed S::a"], "a flexible array member of the typedef.",
     pre=TDL)
pair("equal", "typedef-alignment-template-argument", "X<int> x;", "X<I> x;", [],
     "the typedef as a class template's argument: the specialization is `X<int>` and its layout"
     " reads the canonical argument (`layout_desugar_once`, kiln_ast `layout_sugar.rs:34`; clang 19"
     " agrees).",
     pre=TD + "\ntemplate <class T> struct X { char c; T t; };",
     post="static_assert(alignof(X<I>) == 4 && sizeof(X<I>) == 8);")

# Outside classes: the two that were pending, and the coordinator's list.
pair("differ", "alignof-typedef-alignment", "unsigned long f() { return alignof(int); }",
     "unsigned long f() { return alignof(I); }", ["changed f()"],
     "`alignof` of the typedef: the operand's type digest holds it.", pre=TD)
pair("differ", "variable-typedef-alignment", "int v;", "I v;", ["changed v"],
     "a variable of the typedef (kiln_abi `layout/mod.rs:518`, `decl_align`).", pre=TD)
pair("differ", "typedef-alignment-parameter", "void f(int p) { }", "void f(I p) { }",
     ["changed f(int)"], "a parameter of the typedef (its `__alignof__` reads it).", pre=TD)
pair("equal", "typedef-alignment-return-type", "int fr();", "I fr();", [],
     "a return type of the typedef: no alignment is read from a function's type; a use of"
     " the call's type (`alignof(decltype(fr()))`) writes it where it is used.", pre=TD)
pair("differ", "typedef-alignment-new", "int *f() { return new int; }",
     "I *f() { return new I; }", ["changed f()"],
     "`new` of the typedef: the allocated type holds it.", pre=TD)
pair("differ", "typedef-alignment-alignof-array", "unsigned long f() { return alignof(long[4]); }",
     "unsigned long f() { return alignof(L[4]); }", ["changed f()"],
     "`alignof` of an array of the typedef.", pre=TDL)
pair("differ", "typedef-alignment-sizeof-class", "struct S { char c; int i; };",
     "struct S { char c; I i; };", ["changed S::i"],
     "`sizeof` of a class holding one: the field holds the typedef and `f` holds `sizeof(S)`,"
     " which the class decides (item 2).",
     pre=TD, post="unsigned long f() { return sizeof(S); }")
pair("differ", "typedef-alignment-deduced", "int v;", "I v;", ["changed v", "changed av"],
     "a deduced type keeps the typedef in Kiln and clang 19 (`alignof(decltype(av))` is 8).",
     pre=TD, post="auto av = v;")
pair("differ", "typedef-alignment-decltype", "int v;", "I v;", ["changed v", "changed S::d"],
     "`decltype` of a variable of the typedef.", pre=TD,
     post="struct S { char c; decltype(v) d; };\nstatic_assert(sizeof(S) == (alignof(S) == 8 ? 16 : 8));")
pair("differ", "typedef-alignment-typedef-of-typedef", "typedef int T2;", "typedef I T2;",
     ["changed T2", "changed S::i"], "a plain typedef of the typedef, and its use.", pre=TD,
     post="struct S { char c; T2 i; };")
pair("differ", "typedef-alignment-using", "using U = int;", "using U = I;",
     ["changed U", "changed S::i"], "an alias-declaration of the typedef, and its use.", pre=TD,
     post="struct S { char c; U i; };")
pair("differ", "typedef-alignment-alias-declaration", "using T16 = int;",
     "using T16 [[gnu::aligned(16)]] = int;", ["changed T16", "changed S::i"],
     "the attribute on an alias-declaration.", post="struct S { char c; T16 i; };\n"
     "static_assert(sizeof(S) == (alignof(S) == 16 ? 32 : 8));")
pair("differ", "typedef-alignment-attribute-first", "typedef int T16;",
     "[[gnu::aligned(16)]] typedef int T16;", ["changed T16", "changed S::i"],
     "the attribute before `typedef` (no `alignas` form applies to a typedef).",
     post="struct S { char c; T16 i; };\nstatic_assert(sizeof(S) == (alignof(S) == 16 ? 32 : 8));")
pair("differ", "typedef-alignment-reference", "unsigned long f() { return alignof(long &); }",
     "unsigned long f() { return alignof(L &); }", ["changed f()"],
     "`alignof` of a reference to the typedef is the typedef's ([expr.alignof]/3; clang 19: 4;"
     " Kiln: 8, \"For Kiln\").", pre=TDL)
pair("differ", "typedef-alignment-atomic", "struct S { char c; _Atomic(Big) a; };",
     "struct S { char c; _Atomic(BA) a; };", ["changed S::a"],
     "an atomic of the typedef, too wide to be promoted: its alignment is the typedef's.",
     pre="struct Big { char b[32]; };\ntypedef Big BA __attribute__((aligned(64)));",
     post="static_assert(sizeof(S) == (alignof(S) == 64 ? 128 : 33));")
pair("differ", "typedef-alignment-dependent-array",
     "template <int N> struct T { char c; long a[N]; };",
     "template <int N> struct T { char c; L a[N]; };", ["changed T::a"],
     "a dependent-sized array of the typedef in a class template.", pre=TDL,
     post="static_assert(sizeof(T<2>) == (alignof(T<2>) == 4 ? 20 : 24));")
pair("differ", "typedef-alignment-address-space", "__attribute__((address_space(1))) long v;",
     "__attribute__((address_space(1))) L v;", ["changed v"],
     "an address-space-qualified type of the typedef (Kiln's `ExtQual`).", pre=TDL)
pair("differ", "typedef-alignment-nested", "typedef I J __attribute__((aligned(16)));",
     "typedef K J __attribute__((aligned(16)));", ["changed J", "changed x"],
     "an aligned typedef of another aligned typedef, `I` or `K` (same alignment): every"
     " typedef met is listed (rule 9), not only the outermost, which decides the alignment.",
     pre="typedef int I __attribute__((aligned(8)));\ntypedef int K __attribute__((aligned(8)));",
     post="J x;")
# `v` and each typedef from T4194 on: T4194's underlying type `T4193` is
# 4,095 typedefs (T4193 down to T099) and then `int`, so its walk reaches no
# canonical type in 4,096 steps.
BOUND_EXPECT = ["indeterminate v"] + [f"indeterminate T{k}" for k in range(4194, 4300)]
# F1 of the review: declarations that disagree on carrying an alignment.
RD = "typedef int R __attribute__((aligned(8)));"
RP = "typedef int R;"
USE_R = "unsigned long g() { return alignof(R); }\nstruct S { char c; R r; };"
pair("indeterminate", "typedef-alignment-by-declaration-aligned-first", "", RD + "\n" + RP,
     ["indeterminate R", "indeterminate g()", "indeterminate S::r"],
     "an aligned declaration, then a plain one: Kiln's `alignof(R)` is 4 (it reads the"
     " declaration the use names), its layout 8; clang 19 gives 8 for both."
     " `typedef/alignment-by-declaration`.", post=USE_R)
pair("indeterminate", "typedef-alignment-by-declaration-plain-first", "", RP + "\n" + RD,
     ["indeterminate R", "indeterminate g()", "indeterminate S::r"],
     "a plain declaration, then an aligned one: Kiln's `alignof(R)` is 8; clang 19 gives 8.",
     post=USE_R)
pair("indeterminate", "typedef-alignment-by-declaration-use-between", "",
     RD + "\nunsigned long g() { return alignof(R); }\n" + RP,
     ["indeterminate R", "indeterminate g()"],
     "a use between an aligned declaration and a plain one: Kiln's `alignof(R)` is 8 there and 4"
     " after the plain one; clang 19 gives 8 for both.")
# Re-check R1: declarations that all carry an alignment, with different
# values. Kiln's layout reads every declaration present when a class is
# first laid out and caches it; clang reads the named declaration.
R16 = "typedef int R __attribute__((aligned(16)));"
CACHE = "struct S { char c; R r; };"
for slug, mid, end in [("laid-out-before", "static_assert(sizeof(S) > 0);", ""),
                       ("laid-out-after", "", "static_assert(sizeof(S) > 0);")]:
    pair("indeterminate", "typedef-alignment-by-value-" + slug, "",
         "\n".join(x for x in [RD, CACHE, mid, R16, end] if x),
         ["indeterminate R", "indeterminate S::r"],
         "aligned(8), a class laid out " + slug.split("-")[-1] + " the aligned(16) redeclaration:"
         " Kiln's `sizeof(S)` is 16 if S is laid out first between them and 32 after (cached);"
         " clang 19 gives 16. `g()` and `S` hold no type that meets `R` (`g` holds `sizeof(S)`,"
         " which the class's units decide), so only `R` and `S::r` are indeterminate.",
         post="unsigned long g() { return sizeof(S); }")
for slug, first, second in [("8-then-16", RD, R16), ("16-then-8", R16, RD)]:
    pair("indeterminate", "typedef-alignment-by-value-" + slug, "", first + "\n" + second,
         ["indeterminate R", "indeterminate g()", "indeterminate S::r"],
         "two declarations with different alignments: Kiln's `alignof(R)` reads the declaration"
         " a use names, its layout every one present when the class is first laid out.",
         post=USE_R)
pair("indeterminate", "typedef-alignment-by-value-third-declaration", "",
     R16 + "\n" + RD + "\n" + RD, ["indeterminate R", "indeterminate g()", "indeterminate S::r"],
     "aligned(16), then aligned(8) twice: the two latest declarations agree, the first does not"
     " (Kiln's `redecls_of` walks from the latest), so every declaration is compared.",
     post=USE_R)
pair("differ", "typedef-alignment-same-value", "int v;", "R v;", ["changed v"],
     "every declaration carries aligned(16): the declarations agree, determinate.",
     pre=R16 + "\n" + R16)
pair("differ", "typedef-alignment-every-declaration", "int v;", "R v;", ["changed v"],
     "every declaration carries the alignment: determinate.",
     pre=RD + "\n" + RD)
pair("equal", "typedef-alignment-no-declaration", "int v;", "R v;", [],
     "no declaration carries an alignment: transparent and determinate.", pre=RP + "\n" + RP)
# F2: a function template's specialization keeps its first use's argument
# sugar in Kiln, but layout reads the argument canonical.
pair("equal", "typedef-alignment-function-template-argument",
     "unsigned long a = f<int>();\nunsigned long b = f<I>();",
     "unsigned long a = f<I>();\nunsigned long b = f<int>();", [],
     "the first use of a function template names the typedef or `int`: Kiln keeps the first"
     " use's sugar in the specialization's arguments, but `layout_desugar_once` reads it"
     " canonical, so `alignof(decltype(x))` is 4 either way (Kiln's"
     " tests/conformance/expr.alignof/run-aligned-typedef-template-argument.cpp; clang 19: 4).",
     pre=TD + "\ntemplate <class T> unsigned long f() { T x = 0; return alignof(decltype(x)); }")
# F3: the bound of the walk, reached by a chain of 4,200 typedefs made by
# macros (T099, then T100 to T4299).
TEN = "#define TEN(p, q) typedef T##q##9 T##p##0; " + " ".join(
    f"typedef T##p##{k - 1} T##p##{k};" for k in range(1, 10))
HUNDRED = "#define HUNDRED(p, q) TEN(p##0, q##9) " + " ".join(
    f"TEN(p##{k}, p##{k - 1})" for k in range(1, 10))
CHAIN = "\n".join([
    "typedef int T099 __attribute__((aligned(8)));", TEN, HUNDRED,
    " ".join(f"HUNDRED({k}, {k - 1})" for k in range(1, 43)), "T4299 v;"])
pair("indeterminate", "typedef-alignment-bound", "", CHAIN,
     BOUND_EXPECT,
     "4,200 typedefs in a chain: past 4096 steps the walk stops and the type is"
     " indeterminate (`TypeView/Typedef-aligned-bound`); it fails closed.")
# The reviewer's answer 2: an expression's type is written with its sugar in
# the using unit, so pointers, return types and member pointers need no walk.
pair("differ", "typedef-alignment-dereference", "int g(int *q) { return *q; }",
     "int g(UI *q) { return *q; }", ["changed g(int *)"],
     "a load through a pointer to an under-aligned typedef (clang 19 loads with align 1): the"
     " pointer is not walked, the parameter's type and `*q`'s type are written with sugar.",
     pre="typedef int UI __attribute__((aligned(1)));")
pair("differ", "typedef-alignment-call-type", "int f();", "I f();", ["changed g()"],
     "`alignof(decltype(f()))` (clang 19: 8 / 4): the return type is not walked; the call's"
     " type is written in the user.", pre=TD,
     post="unsigned long g() { return alignof(decltype(f())); }")
pair("differ", "typedef-alignment-field-decltype", "int *p;", "I *p;", ["changed S::r"],
     "a field of type `decltype(*p)`: the pointer `p` is not walked, the field's type is.",
     pre=TD, post="struct S { decltype(*p) r; };")
pair("differ", "typedef-alignment-value", "typedef int I __attribute__((aligned(8)));",
     "typedef int I __attribute__((aligned(16)));", ["changed I"],
     "another alignment on the typedef: its own unit; a class with a field of it holds the"
     " typedef's entity and is decided by the typedef's unit, as by a field's class type.",
     post="struct S { char c; I i; };")

# Item 2: values computed from a layout, where they are used.
PACKED_S = ("#ifdef B\n#pragma pack(push, 1)\n#endif\nstruct S { char c; int i; };\n"
            "#ifdef B\n#pragma pack(pop)\n#endif")
pair("differ", "layout-sizeof-in-function", "", "", ["changed S"],
     "`sizeof` is not stored: the function holds `sizeof(S)` and the class decides it.",
     pre=PACKED_S, post="unsigned long f() { return sizeof(S); }")
pair("differ", "layout-sizeof-in-constant", "", "",
     ["changed S", "changed arr", "derived-changed n::<value>"],
     "`sizeof` in an array bound and in a constant: the values are written where they are used.",
     pre=PACKED_S, post="constexpr unsigned long n = sizeof(S);\nint arr[sizeof(S)];")
pair("differ", "layout-constant-object", "", "", ["changed S", "derived-changed s::<value>"],
     "a constant object's image follows the layout (digest §7).",
     pre=PACKED_S, post="constexpr S s{1, 2};")
pair("differ", "layout-offsetof", "", "", ["changed S", "changed f()"],
     "`offsetof` stores its result (`OffsetOfExpr.b`).",
     pre=PACKED_S, post="unsigned long f() { return __builtin_offsetof(S, i); }")
pair("differ", "layout-member-pointer", "", "", ["changed S"],
     "a pointer to member is the member and its path, no offset.",
     pre=PACKED_S, post="constexpr int S::*pm = &S::i;\nint g(S &s) { return s.*pm; }")
pair("differ", "layout-base-conversion", "struct C { char c; };", "struct C { char c; long l; };",
     ["changed C", "added C::l"],
     "a conversion between a class and its base writes the base path, not the offset.",
     pre="struct A { int a; };",
     post="struct D : C, A { };\nA *up(D *d) { return d; }\nD *down(A *a) { return static_cast<D *>(a); }")
pair("differ", "layout-dynamic-cast", "struct C { virtual ~C(); char c; };",
     "struct C { virtual ~C(); char c; long l; };", ["changed C", "added C::l"],
     "a `dynamic_cast` writes its type; the path is found at run time.",
     pre="struct A { virtual ~A(); int a; };",
     post="struct D : C, A { };\nD *down(A *a) { return dynamic_cast<D *>(a); }")
pair("differ", "vtable-virtual-call", "struct A { virtual void f(); virtual void g(); };",
     "struct A { virtual void g(); virtual void f(); };", ["changed A", "derived-changed A::<key_function>"],
     "a virtual call stores no vtable index: the class's order of virtual functions decides it.",
     post="void call(A *a) { a->f(); }")

# Item 3: two definitions of `std::strong_ordering`.
SO = "namespace std { struct strong_ordering { int v; static const strong_ordering less, equal, greater; };"
pair("differ", "cmp-category-value",
     SO + "\ninline constexpr strong_ordering strong_ordering::less{-1}, strong_ordering::equal{0},"
     " strong_ordering::greater{1}; }",
     SO + "\ninline constexpr strong_ordering strong_ordering::less{-2}, strong_ordering::equal{0},"
     " strong_ordering::greater{1}; }",
     ["changed std::strong_ordering::less", "derived-changed std::strong_ordering::less::<value>",
      "derived-changed w::<value>"],
     "another value for `less`: its unit, and a constant `<=>` (the Oven reads `ints`).",
     post="int a = 1, b = 2;\nauto v = a <=> b;\nconstexpr auto w = 1 <=> 2;")
pair("equal", "cmp-category-member-order",
     "namespace std { struct strong_ordering { int v; static const strong_ordering less, equal, greater; };",
     "namespace std { struct strong_ordering { int v; static const strong_ordering equal, less, greater; };",
     [],
     "the order of the category's values in its class: Kiln finds them by name (kiln_sema"
     " `class/compare/mod.rs:163`), and no unit differs.",
     post="inline constexpr strong_ordering strong_ordering::less{-1}, strong_ordering::equal{0},"
     " strong_ordering::greater{1}; }\nint a = 1, b = 2;\nauto v = a <=> b;")
pair("differ", "cmp-category-field-type",
     "namespace std { struct strong_ordering { int v;",
     "namespace std { struct strong_ordering { signed char v;",
     ["changed std::strong_ordering::v", "changed std::strong_ordering::less",
      "changed std::strong_ordering::equal", "changed std::strong_ordering::greater"],
     "another type for the category's data member: the member and every value.",
     post="static const strong_ordering less, equal, greater; };\ninline constexpr strong_ordering"
     " strong_ordering::less{-1}, strong_ordering::equal{0}, strong_ordering::greater{1}; }\n"
     "int a = 1, b = 2;\nauto v = a <=> b;")

PENDING = {}

SECTION = {
    "RecordLayout": "§4.7", "LayoutCache": "§4.7", "Ast": "§10.9",
}


def write_file(path, text):
    with open(path, "w", encoding="utf-8") as f:
        f.write(text)


def write():
    names = set()
    for root, short, row, kind, c, _extra in TESTS:
        key = (root, short, row)
        if key in names:
            sys.exit(f"duplicate {key}")
        names.add(key)
        pre, use, site = kind[c]
        d = os.path.join(ROOT, root, short, row)
        os.makedirs(d, exist_ok=True)
        sec = SECTION.get(short, "§10.6")
        what = f"{sec} {short} `{row}`" if short != "_" else "WP-107 construct"
        lines = ['#include "side.h"', f"// {what} (demand, §11.7): {site}; every unit stays equal."]
        if kind is CMP:
            lines.append('#include "compare.h"')
            write_file(os.path.join(d, "compare.h"), COMPARE_H)
        if pre:
            lines.append(pre)
        lines += ["#ifdef B", use, "#endif"]
        write_file(os.path.join(d, "t.cpp"), "\n".join(lines) + "\n")
        write_file(os.path.join(d, "expect.txt"), "added y()\n")
    for root, slug, a, b, expect, note, pre, post, header in PAIRS:
        sub = slug if "/" in slug else os.path.join("_", slug)
        key = (root, sub)
        if key in names:
            sys.exit(f"duplicate {key}")
        names.add(key)
        d = os.path.join(ROOT, root, sub)
        os.makedirs(d, exist_ok=True)
        lines = ['#include "side.h"', f"// WP-107 (spec/landed/digest/WP-107.md): {note}"]
        if pre:
            lines.append(pre)
        if root == "indeterminate":
            lines.append(b)
        elif a or b:
            lines += ["#ifdef B", b, "#else", a, "#endif"] if a else ["#ifdef B", b, "#endif"]
        if post:
            lines.append(post)
        write_file(os.path.join(d, "t.cpp"), "\n".join(lines) + "\n")
        out = []
        if slug in PENDING:
            out += ["# pending", f"# needs: {PENDING[slug]} (WP-107 item 1: a wrong equal)"]
        out += expect
        if out:
            write_file(os.path.join(d, "expect.txt"), "\n".join(out) + "\n")


if __name__ == "__main__":
    write()
