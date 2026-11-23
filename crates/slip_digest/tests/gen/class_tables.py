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
     "`packed` on a redeclaration after the definition is dropped (kiln_sema `after_def.rs:153`):"
     " neither the layout nor a unit changes.", pre="struct S { char c; int i; };")
pair("differ", "layout-unnamed-bit-field", "struct S { char a; char b; };",
     "struct S { char a; int : 0; char b; };", ["changed S", "added S::(unnamed 0)"],
     "an unnamed bit-field moves the next field; it is a member of the class.")
pair("differ", "layout-tail-padding-of-base", "struct P { int i; char c; P() {} };",
     "struct P { int i; char c; P() = default; };", ["changed P", "changed P::P()", "changed S"],
     "a defaulted constructor makes the base POD for layout, whose tail padding a derived class"
     " does not reuse (kiln_abi `layout/props.rs:252`).", post="struct S : P { char d; };")

# The typedef-carried alignment (item 1's wrong equal): indeterminate
# where the class is written, pending where it is not.
TD = "typedef int I __attribute__((aligned(8)));"
pair("indeterminate", "layout-typedef-alignment-field", "", "struct S { char c; I i; };",
     ["indeterminate S"],
     "a field whose type is a typedef carrying `aligned`: the layout reads the typedef"
     " (kiln_abi `layout/mod.rs:443`), the type digest is canonical.", pre=TD)
pair("indeterminate", "layout-typedef-alignment-alias-template", "",
     "template <class T> using A = T;\nstruct S { char c; A<I> i; };", ["indeterminate S"],
     "an alias template keeps its argument's typedef for layout (kiln_ast `layout_sugar.rs:47`).", pre=TD)
pair("indeterminate", "layout-typedef-alignment-pattern", "",
     "template <class T> struct X { char c; I i; T t; };\nX<char> x;",
     ["indeterminate X", "indeterminate X<char>::<definition>"],
     "a class template's field typed by the typedef: the pattern and the instantiation's"
     " `definition` (whose fields are derived, so the walk reads Kiln's fields, not the member list).",
     pre=TD)
pair("indeterminate", "layout-typedef-alignment-alignas-class", "",
     "struct alignas(I) S { char c; };", ["indeterminate S"],
     "`alignas(type-id)` naming the typedef: the alignment is the typedef's"
     " (kiln_abi `layout/props.rs:88`), the attribute's type digest canonical.", pre=TD)
pair("indeterminate", "layout-typedef-alignment-alignas-field", "",
     "struct S { char c; alignas(I) char d; };", ["indeterminate S"],
     "`alignas(type-id)` on a field naming the typedef.", pre=TD)
TDL = "typedef long L __attribute__((aligned(4)));"
pair("indeterminate", "layout-typedef-alignment-array", "", "struct S { char c; L a[2]; };",
     ["indeterminate S"],
     "an array of a typedef that lowers alignment (an over-aligned element is an error): Kiln's"
     " layout reads the element's typedef.", pre=TDL)
pair("indeterminate", "layout-typedef-alignment-const-array", "",
     "typedef const L CL;\nstruct S { char c; CL a[2]; };", ["indeterminate S"],
     "an array of a cv-qualified typedef of the typedef.", pre=TDL)
pair("indeterminate", "layout-typedef-alignment-qualified-array-typedef", "",
     "typedef L LA[2];\nstruct S { char c; const LA a; };", ["indeterminate S"],
     "a cv-qualified array typedef (Kiln's `QualifiedArray` sugar, kiln_ast `types/canon.rs:100`)"
     " whose element is the typedef.", pre=TDL)
pair("indeterminate", "layout-typedef-alignment-flexible-array", "",
     "struct S { char c; L a[]; };", ["indeterminate S"],
     "a flexible array member of the typedef.", pre=TDL)
pair("indeterminate", "layout-typedef-alignment-redeclared", "",
     "typedef int I2;\nstruct S { char c; I2 i; };", ["indeterminate S"],
     "the field names a later redeclaration of a typedef whose first declaration carries"
     " `aligned` (Kiln reads every redeclaration, kiln_abi `layout/props.rs:85`).",
     pre="typedef int I2 __attribute__((aligned(8)));")
pair("equal", "layout-typedef-alignment-template-argument",
     "template <class T> struct X { char c; T t; };\nX<int> x;",
     "template <class T> struct X { char c; T t; };\nX<I> x;", [],
     "the typedef as a class template's argument is read canonical by Kiln's layout"
     " (`layout_desugar_once`, kiln_ast `layout_sugar.rs:34`): no layout and no unit changes.",
     pre=TD)
pair("differ", "alignof-typedef-alignment", "unsigned long f() { return alignof(int); }",
     "unsigned long f() { return alignof(I); }", ["changed f()"],
     "`alignof` of a typedef carrying `aligned` is the typedef's alignment; the operand's"
     " type digest is canonical, so `f` is equal while its value differs (wrong equal).",
     pre=TD)
pair("differ", "variable-typedef-alignment", "int v;", "I v;", ["changed v"],
     "a variable of a typedef carrying `aligned` has the typedef's alignment (kiln_abi"
     " `layout/mod.rs:518`, `decl_align`); its type digest is canonical (wrong equal).", pre=TD)

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
     " `class/compare/mod.rs:165`), and the static members are written sorted.",
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

PENDING = {
    "alignof-typedef-alignment": "the type digest is canonical and loses a typedef's `aligned`",
    "variable-typedef-alignment": "the type digest is canonical and loses a typedef's `aligned`",
}

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
        key = (root, "_", slug)
        if key in names:
            sys.exit(f"duplicate {key}")
        names.add(key)
        d = os.path.join(ROOT, root, "_", slug)
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
