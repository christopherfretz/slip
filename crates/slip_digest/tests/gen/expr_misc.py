#!/usr/bin/env python3
"""Generates the pair tests of WP-037 (the expression kinds of
`expr_misc.rs`, `spec/digest-kinds.md` §7) under `crates/slip_digest/tests/`.

Run from anywhere: `python3 crates/slip_digest/tests/gen/expr_misc.py`. It
writes every directory of the tables below (`t.cpp` and `expect.txt`) and
removes nothing; the generated directories are committed, and this script
is not part of the gate (as `attrs.py`).

A test is `(root, short, row, a, b, expect, opts)`:
- `a`/`b`: the source of side A and side B (the `#ifdef B` branches);
- `expect`: the lines of `expect.txt` (default `changed v`);
- `opts`: `pre` (source before the branches), `std`, `needs` (marks the
  test `# pending` on that area), `note` (the comment after the section).
"""

import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)

TESTS = []

# The declarations most tests read.
VARS = "int a = 1, b = 2, c = 3; long l;"
VEC = ("typedef int v4 __attribute__((ext_vector_type(4)));\n"
       "typedef float f4 __attribute__((ext_vector_type(4)));\n"
       "v4 vv, vv2; v4 *pv;")
TYPEINFO = "namespace std { class type_info; }"
OFFSET = "struct S { int n; int m; int x[4]; int y[4]; };"
SOURCE_LOCATION = (
    "namespace std { struct source_location { struct __impl { const char *_M_file_name;"
    " const char *_M_function_name; unsigned _M_line; unsigned _M_column; }; }; }")
# A minimal coroutine library (`std::coroutine_traits`, a handle and an
# awaitable), for the coroutine kinds; their bodies need the statements.
CORO = """namespace std {
template <class R, class... A> struct coroutine_traits { using promise_type = typename R::promise_type; };
template <class P = void> struct coroutine_handle {
  static coroutine_handle from_address(void *) noexcept; void *address() const noexcept;
  template <class Q> coroutine_handle(coroutine_handle<Q>) noexcept; coroutine_handle() noexcept;
};
template <> struct coroutine_handle<void> {
  static coroutine_handle from_address(void *) noexcept; void *address() const noexcept;
  coroutine_handle() noexcept; template <class Q> coroutine_handle(coroutine_handle<Q>) noexcept;
};
}
struct Never { bool await_ready() noexcept; void await_suspend(std::coroutine_handle<>) noexcept; void await_resume() noexcept; };
struct Maybe { bool await_ready() noexcept; bool await_suspend(std::coroutine_handle<>) noexcept; int await_resume() noexcept; };
struct Hand { bool await_ready() noexcept; std::coroutine_handle<> await_suspend(std::coroutine_handle<>) noexcept; int await_resume() noexcept; };
struct Task { struct promise_type {
  Task get_return_object(); Never initial_suspend() noexcept; Never final_suspend() noexcept;
  void return_void(); void unhandled_exception(); Never yield_value(int);
}; };
Never never; Maybe maybe; Hand hand;"""

CMP = """namespace std { struct strong_ordering { int v;
  static const strong_ordering less, equal, greater;
  friend constexpr bool operator==(strong_ordering, int) { return true; }
  friend constexpr bool operator!=(strong_ordering, int) { return true; }
}; inline constexpr strong_ordering strong_ordering::less{-1}, strong_ordering::equal{0},
  strong_ordering::greater{1}; }
struct M { bool operator==(const M &) const; bool operator<(const M &) const; };"""


def t(root, short, row, a, b, expect=None, **opts):
    if expect is None:
        expect = [] if root == "equal" else ["changed v"]
    TESTS.append((root, short, row, a, b, expect, opts))


def differ(short, row, a, b, expect=None, **opts):
    t("differ", short, row, a, b, expect, **opts)


def equal(short, row, a, b, **opts):
    t("equal", short, row, a, b, [], **opts)


def node(row, a, b, expect=None, **opts):
    differ("ExprNode", row, a, b, expect, **opts)


def tmpl(body):
    """`body` as a member initialiser of a class template of `T`."""
    return f"template <class T> struct A {{ {body} }};"


# ---------------------------------------------------------------------------
# ExprKind: the kind on side A, another expression of the same type on B.

KIND_PRE = {
    "CXXTypeidExpr": TYPEINFO + "\nextern const std::type_info &r;",
    "ShuffleVectorExpr": VEC,
    "ConvertVectorExpr": VEC,
    "ExtVectorElementExpr": VEC,
    "OffsetOfExpr": OFFSET,
}
KINDS = [
    ("PredefinedExpr", "const char *f(const char *s = __func__);",
     'const char *f(const char *s = "");', "f(const char *)", {}),
    ("SourceLocExpr", "unsigned v = __builtin_LINE();", "unsigned v = 1u;", "v", {}),
    ("CXXNoexceptExpr", "bool v = noexcept(a);", "bool v = true;", "v", {}),
    ("CXXTypeidExpr", "const std::type_info &v = typeid(int);",
     "const std::type_info &v = r;", "v", {}),
    ("UnaryExprOrTypeTraitExpr", "unsigned long v = sizeof(int);", "unsigned long v = 4ul;",
     "v", {}),
    ("TypeTraitExpr", "bool v = __is_class(int);", "bool v = false;", "v", {}),
    ("ArrayTypeTraitExpr", "unsigned long v = __array_rank(int[1]);",
     "unsigned long v = 1ul;", "v", {}),
    ("ExpressionTraitExpr", "bool v = __is_lvalue_expr(a);", "bool v = true;", "v", {}),
    ("OffsetOfExpr", "unsigned long v = __builtin_offsetof(S, m);", "unsigned long v = 4ul;",
     "v", {}),
    ("ChooseExpr", "int v = __builtin_choose_expr(1, 1, 2);", "int v = 1;", "v", {}),
    ("VAArgExpr", "int v = __builtin_va_arg(ap, int);", "int v = 1;", "v",
     {"pre": "__builtin_va_list ap;"}),
    ("AtomicExpr", "int v = __atomic_load_n(&a, 0);", "int v = a;", "v", {}),
    ("ShuffleVectorExpr", "v4 v = __builtin_shufflevector(vv, vv, 0, 1, 2, 3);", "v4 v = vv;",
     "v", {}),
    ("ConvertVectorExpr", "v4 v = __builtin_convertvector(vv, v4);", "v4 v = vv;", "v", {}),
    ("ExtVectorElementExpr", "int v = vv.x;", "int v = vv[0];", "v", {}),
    ("GenericSelectionExpr", "int v = _Generic(a, int: 1, default: 2);", "int v = 1;", "v", {}),
    ("StmtExpr", "int v = ({ 1; });", "int v = 1;", "v", {"needs": "stmt"}),
    ("AddrLabelExpr", "void f() { l: void *p = &&l; }", "void f() { l: void *p = 0; }", "f()",
     {"needs": "stmt"}),
    ("CoawaitExpr", "Task f() { co_await never; }", "Task f() { co_return; }", "f()",
     {"needs": "stmt", "pre": CORO}),
    ("CoyieldExpr", "Task f() { co_yield 1; }", "Task f() { co_return; }", "f()",
     {"needs": "stmt", "pre": CORO}),
    ("DependentCoawaitExpr", "template <class T> Task f(T t) { co_await t; }",
     "template <class T> Task f(T t) { co_return; }", "f(T)", {"needs": "stmt", "pre": CORO}),
    ("PseudoObjectExpr",
     "struct X { M m; std::strong_ordering operator<=>(const X &) const = default; };",
     "struct X { M m; };", "X", {"needs": "stmt", "pre": CMP}),
]
for kind, a, b, name, opts in KINDS:
    pre = opts.pop("pre", KIND_PRE.get(kind, VARS))
    differ("ExprKind", kind, a, b, [f"changed {name}"], pre=pre,
           note=f"`{kind}`: the kind on side A only.", **opts)

# ---------------------------------------------------------------------------
# Names and builtins of a position.

FN = "const char *f(const char *s = {});"
node("PredefinedExpr.aux", FN.format("__func__"), FN.format("__FUNCTION__"),
     ["changed f(const char *)"])
differ("PredefinedKind", "Func", FN.format("__func__"), FN.format("__FUNCTION__"),
       ["changed f(const char *)"])
differ("PredefinedKind", "Function", FN.format("__FUNCTION__"), FN.format("__func__"),
       ["changed f(const char *)"])
differ("PredefinedKind", "PrettyFunction", FN.format("__PRETTY_FUNCTION__"), FN.format("__func__"),
       ["changed f(const char *)"])
equal("ExprNode", "PredefinedExpr.a", FN.format("__func__"), "\n" + FN.format("__func__"),
      note="`PredefinedExpr.a`: the string follows from where the use is (moved a line).")

node("SourceLocExpr.aux", "unsigned v = __builtin_LINE();", "unsigned v = __builtin_COLUMN();")
SL = [("File", "__builtin_FILE", "__builtin_FILE_NAME", "const char *"),
      ("FileName", "__builtin_FILE_NAME", "__builtin_FUNCTION", "const char *"),
      ("Function", "__builtin_FUNCTION", "__builtin_FILE", "const char *"),
      ("Line", "__builtin_LINE", "__builtin_COLUMN", "unsigned"),
      ("Column", "__builtin_COLUMN", "__builtin_LINE", "unsigned")]
for row, x, y, ty in SL:
    differ("SourceLocKind", row, f"{ty} v = {x}();", f"{ty} v = {y}();")
differ("SourceLocKind", "SourceLocStruct", "const void *v = __builtin_source_location();",
       "const void *v = 0;", pre=SOURCE_LOCATION)
equal("ExprNode", "SourceLocExpr.a", "const char *v = __builtin_FUNCTION();",
      "\nconst char *v = __builtin_FUNCTION();",
      note="`SourceLocExpr.a`: the context follows from where the use is (moved a line).")
# SourceLocValues: every value follows from the position.
equal("SourceLocValues", "file", '#line 10 "a.cpp"\nconst char *v = __builtin_FILE();',
      '#line 10 "b.cpp"\nconst char *v = __builtin_FILE();')
equal("SourceLocValues", "file_name", '#line 10 "a.cpp"\nconst char *v = __builtin_FILE_NAME();',
      '#line 10 "b.cpp"\nconst char *v = __builtin_FILE_NAME();')
equal("SourceLocValues", "function",
      "const char *f(const char *s = __builtin_FUNCTION());",
      "\nconst char *f(const char *s = __builtin_FUNCTION());")
equal("SourceLocValues", "line", "#line 10\nunsigned v = __builtin_LINE();",
      "#line 20\nunsigned v = __builtin_LINE();")
equal("SourceLocValues", "column", "unsigned v = __builtin_COLUMN();",
      "unsigned v =   __builtin_COLUMN();")
equal("SourceLocValues", "location", "#line 10\nconst void *v = __builtin_source_location();",
      "#line 20\nconst void *v = __builtin_source_location();", pre=SOURCE_LOCATION)
equal("SourceLocValues", "ctx", "const char *f(const char *s = __builtin_FUNCTION());",
      "\n\nconst char *f(const char *s = __builtin_FUNCTION());")

# ---------------------------------------------------------------------------
# noexcept and typeid.

node("CXXNoexceptExpr.a", "bool v = noexcept(a);", "bool v = noexcept(b);", pre=VARS)
# The constructor is named by its key: only the stored result differs at
# the use.
node("CXXNoexceptExpr.b", "struct S { S(); };\nbool v = noexcept(S());",
     "struct S { S() noexcept; };\nbool v = noexcept(S());", ["changed S::S()", "changed v"])

TI = "const std::type_info &v = typeid({});"
node("CXXTypeidExpr.aux-potentially-evaluated",
     "struct P { virtual void h(); };\nextern P &p;\n" + TI.format("p"),
     "struct P { void h(); };\nextern P &p;\n" + TI.format("p"),
     ["changed P", "changed P::h()", "changed v"], pre=TYPEINFO)
node("CXXTypeidExpr.a", TI.format("int"), TI.format("a"), pre=TYPEINFO + "\n" + VARS)
differ("TypeidOperand", "Type", TI.format("int"), TI.format("a"), pre=TYPEINFO + "\n" + VARS)
differ("TypeidOperand", "Type.0", TI.format("int"), TI.format("long"), pre=TYPEINFO)
differ("TypeidOperand", "Expr", TI.format("a"), TI.format("int"), pre=TYPEINFO + "\n" + VARS)
differ("TypeidOperand", "Expr.0", TI.format("a"), TI.format("b"), pre=TYPEINFO + "\n" + VARS)

# ---------------------------------------------------------------------------
# Traits.

UL = "unsigned long v = {};"
node("UnaryExprOrTypeTraitExpr.bits-arg-is-type", UL.format("sizeof(int)"), UL.format("sizeof(a)"),
     pre=VARS)
node("UnaryExprOrTypeTraitExpr.aux", UL.format("sizeof(int)"), UL.format("alignof(int)"))
node("UnaryExprOrTypeTraitExpr.a-type", UL.format("sizeof(int)"), UL.format("sizeof(long)"))
node("UnaryExprOrTypeTraitExpr.a-expr", UL.format("sizeof(a)"), UL.format("sizeof(b)"), pre=VARS)
TK = [("SizeOf", "sizeof", "alignof"), ("AlignOf", "alignof", "__alignof"),
      ("PreferredAlignOf", "__alignof", "sizeof"), ("DataSizeOf", "__datasizeof", "sizeof")]
for row, x, y in TK:
    differ("TraitKind", row, UL.format(f"{x}(int)"), UL.format(f"{y}(int)"))

# The type traits: each in a class template, where the result is value-
# dependent (0 on both sides) and the operands are the same, against the
# next trait of its group: only `aux` differs.
TRAITS = [line.split("=") for line in """HasNothrowAssign=__has_nothrow_assign
HasNothrowConstructor=__has_nothrow_constructor
HasNothrowCopy=__has_nothrow_copy
HasNothrowMoveAssign=__has_nothrow_move_assign
HasTrivialAssign=__has_trivial_assign
HasTrivialDefaultConstructor=__has_trivial_constructor
HasTrivialCopy=__has_trivial_copy
HasTrivialDestructor=__has_trivial_destructor
HasTrivialMoveAssign=__has_trivial_move_assign
HasTrivialMoveConstructor=__has_trivial_move_constructor
HasUniqueObjectRepresentations=__has_unique_object_representations
HasVirtualDestructor=__has_virtual_destructor
IsAbstract=__is_abstract
IsAggregate=__is_aggregate
IsArithmetic=__is_arithmetic
IsArray=__is_array
IsBoundedArray=__is_bounded_array
IsClass=__is_class
IsCompleteType=__is_complete_type
IsCompound=__is_compound
IsConst=__is_const
IsDestructible=__is_destructible
IsEmpty=__is_empty
IsEnum=__is_enum
IsFinal=__is_final
IsFloatingPoint=__is_floating_point
IsFunction=__is_function
IsFundamental=__is_fundamental
IsIntegral=__is_integral
IsLiteral=__is_literal
IsLvalueReference=__is_lvalue_reference
IsMemberFunctionPointer=__is_member_function_pointer
IsMemberObjectPointer=__is_member_object_pointer
IsMemberPointer=__is_member_pointer
IsNothrowDestructible=__is_nothrow_destructible
IsNullptr=__is_nullptr
IsObject=__is_object
IsPod=__is_pod
IsPointer=__is_pointer
IsPolymorphic=__is_polymorphic
IsReference=__is_reference
IsRvalueReference=__is_rvalue_reference
IsScalar=__is_scalar
IsScopedEnum=__is_scoped_enum
IsSigned=__is_signed
IsStandardLayout=__is_standard_layout
IsTrivial=__is_trivial
IsTriviallyCopyable=__is_trivially_copyable
IsTriviallyDestructible=__is_trivially_destructible
IsTriviallyEqualityComparable=__is_trivially_equality_comparable
IsTriviallyRelocatable=__is_trivially_relocatable
IsUnboundedArray=__is_unbounded_array
IsUnion=__is_union
IsUnsigned=__is_unsigned
IsVoid=__is_void
IsVolatile=__is_volatile
CanPassInRegs=__can_pass_in_regs
IsBitwiseCloneable=__is_bitwise_cloneable
IsImplicitLifetime=__builtin_is_implicit_lifetime
IsCppTriviallyRelocatable=__builtin_is_cpp_trivially_relocatable
IsStructural=__builtin_is_structural
StructuredBindingSize=__builtin_structured_binding_size""".split("\n")]
BINARY = [line.split("=") for line in """IsBaseOf=__is_base_of
IsVirtualBaseOf=__builtin_is_virtual_base_of
IsSame=__is_same
IsAssignable=__is_assignable
IsConvertible=__is_convertible
IsConvertibleTo=__is_convertible_to
IsLayoutCompatible=__is_layout_compatible
IsNothrowAssignable=__is_nothrow_assignable
IsNothrowConvertible=__is_nothrow_convertible
IsPointerInterconvertibleBaseOf=__is_pointer_interconvertible_base_of
IsTriviallyAssignable=__is_trivially_assignable
ReferenceBindsToTemporary=__reference_binds_to_temporary
ReferenceConstructsFromTemporary=__reference_constructs_from_temporary
ReferenceConvertsFromTemporary=__reference_converts_from_temporary
LtSynthesizesFromSpaceship=__builtin_lt_synthesizes_from_spaceship
LeSynthesizesFromSpaceship=__builtin_le_synthesizes_from_spaceship
GtSynthesizesFromSpaceship=__builtin_gt_synthesizes_from_spaceship
GeSynthesizesFromSpaceship=__builtin_ge_synthesizes_from_spaceship""".split("\n")]
VARIADIC = [line.split("=") for line in """IsConstructible=__is_constructible
IsNothrowConstructible=__is_nothrow_constructible
IsTriviallyConstructible=__is_trivially_constructible
IsInvocable=__is_invocable
IsNothrowInvocable=__is_nothrow_invocable""".split("\n")]


def trait_group(group, args):
    for i, (row, x) in enumerate(group):
        y = group[(i + 1) % len(group)][1]
        differ("TypeTrait", row, tmpl(f"unsigned long w = {x}({args});"),
               tmpl(f"unsigned long w = {y}({args});"), ["changed A::w"],
               note=f"`{row}`: against `{y}`, the result value-dependent on both sides.")


trait_group(TRAITS, "T")
trait_group(BINARY, "T, T")
trait_group(VARIADIC, "T, int")
differ("TypeTrait", "ArrayRank", tmpl("unsigned long w = __array_rank(T);"),
       tmpl("unsigned long w = __array_extent(T, 0);"), ["changed A::w"])
differ("TypeTrait", "ArrayExtent", tmpl("unsigned long w = __array_extent(T, 0);"),
       tmpl("unsigned long w = __array_rank(T);"), ["changed A::w"])
differ("TypeTrait", "IsLvalueExpr", tmpl("T t; bool w = __is_lvalue_expr(t);"),
       tmpl("T t; bool w = __is_rvalue_expr(t);"), ["changed A::w"])
differ("TypeTrait", "IsRvalueExpr", tmpl("T t; bool w = __is_rvalue_expr(t);"),
       tmpl("T t; bool w = __is_lvalue_expr(t);"), ["changed A::w"])

node("TypeTraitExpr.aux", tmpl("bool w = __is_class(T);"), tmpl("bool w = __is_union(T);"),
     ["changed A::w"])
node("TypeTraitExpr.a", tmpl("bool w = __is_same(T, int);"), tmpl("bool w = __is_same(T, long);"),
     ["changed A::w"])
node("TypeTraitExpr.b", "struct E {};\nbool v = __is_empty(E);",
     "struct E { int n; };\nbool v = __is_empty(E);", ["changed E", "added E::n", "changed v"])
node("ArrayTypeTraitExpr.aux", UL.format("__array_rank(int[1][2])"),
     UL.format("__array_extent(int[1][2], 0)"))
node("ArrayTypeTraitExpr.a", UL.format("__array_rank(int[1])"), UL.format("__array_rank(int[2])"))
differ("ArrayTraitExtra", "dim", tmpl("unsigned long w = __array_extent(T, 0);"),
       tmpl("unsigned long w = __array_extent(T, 1);"), ["changed A::w"])
differ("ArrayTraitExtra", "value", "typedef int I[1];\n" + UL.format("__array_rank(I)"),
       "typedef int I[1][2];\n" + UL.format("__array_rank(I)"), ["changed I", "changed v"])
node("ExpressionTraitExpr.aux", "bool v = __is_lvalue_expr(a);", "bool v = __is_rvalue_expr(a);",
     pre=VARS)
node("ExpressionTraitExpr.a", "bool v = __is_lvalue_expr(a);", "bool v = __is_lvalue_expr(b);",
     pre=VARS)
node("ExpressionTraitExpr.b", "bool v = __is_lvalue_expr(a);",
     "bool v = __is_lvalue_expr(a + 0);", pre=VARS,
     note="`ExpressionTraitExpr.b`: the result differs (and with it the operand).")

# ---------------------------------------------------------------------------
# offsetof.

OF = "unsigned long v = __builtin_offsetof({});"
node("OffsetOfExpr.a", OF.format("S, x[a]"), OF.format("S, x[b]"), pre=OFFSET + "\n" + VARS)
node("OffsetOfExpr.b", OF.format("S, n"), OF.format("S, m"), pre=OFFSET)
differ("OffsetofStep", "Offset", OF.format("S, x[a]"), OF.format("S, y[a]"),
       pre=OFFSET + "\n" + VARS)
differ("OffsetofStep", "Offset.0", OF.format("S, x[a]"), OF.format("S, y[a]"),
       pre=OFFSET + "\n" + VARS)
differ("OffsetofStep", "Index", OF.format("S, x[a]"), OF.format("S, x[a].p"),
       pre="struct Q { int o; int p; };\nstruct S { int n; Q x[4]; };\n" + VARS,
       note="`Index`: the steps differ in number and kind.")
differ("OffsetofStep", "Index.index", OF.format("S, x[a]"), OF.format("S, x[b]"),
       pre=OFFSET + "\n" + VARS)
differ("OffsetofStep", "Index.elem_size", "typedef int E;\nstruct S { E x[4]; };\n" +
       OF.format("S, x[a]"), "typedef long E;\nstruct S { E x[4]; };\n" + OF.format("S, x[a]"),
       ["changed E", "changed S::x", "changed v"], pre=VARS)
TOF = "unsigned long w = __builtin_offsetof(T, {});"
differ("OffsetofComponent", "Field", tmpl(TOF.format("x.n")), tmpl(TOF.format("x[0]")),
       ["changed A::w"])
differ("OffsetofComponent", "Field.name", tmpl(TOF.format("n")), tmpl(TOF.format("m")),
       ["changed A::w"])
differ("OffsetofComponent", "Index", tmpl(TOF.format("x[0]")), tmpl(TOF.format("x.n")),
       ["changed A::w"])
differ("OffsetofComponent", "Index.index", tmpl(TOF.format("x[a]")), tmpl(TOF.format("x[b]")),
       ["changed A::w"], pre=VARS)

# ---------------------------------------------------------------------------
# Coroutines (in function bodies: the statements' area).

node("CoawaitExpr.aux-implicit", "Task f() { co_await never; }", "Task f() { co_yield 1; }",
     ["changed f()"], pre=CORO, needs="stmt")
node("CoawaitExpr.aux-suspend", "Task f() { co_await maybe; }", "Task f() { co_await hand; }",
     ["changed f()"], pre=CORO, needs="stmt")
node("CoawaitExpr.a", "Task f() { co_await never; }", "Task f() { co_await maybe; }",
     ["changed f()"], pre=CORO, needs="stmt")
node("CoyieldExpr.aux-implicit", "Task f() { co_yield 1; }", "Task f() { co_await never; }",
     ["changed f()"], pre=CORO, needs="stmt")
node("CoyieldExpr.aux-suspend", "Task f() { co_yield 1; }", "Task f() { co_yield 2; }",
     ["changed f()"], pre=CORO, needs="stmt",
     note="`CoyieldExpr.aux-suspend`: the operand differs; the suspend kind is the promise's.")
node("CoyieldExpr.a", "Task f() { co_yield 1; }", "Task f() { co_yield 2; }",
     ["changed f()"], pre=CORO, needs="stmt")
node("DependentCoawaitExpr.a", "template <class T> Task f(T t, T u) { co_await t; }",
     "template <class T> Task f(T t, T u) { co_await u; }", ["changed f(T, T)"], pre=CORO,
     needs="stmt")
for row in ("Void", "Bool", "Handle"):
    x = {"Void": "never", "Bool": "maybe", "Handle": "hand"}[row]
    y = {"Void": "maybe", "Bool": "hand", "Handle": "never"}[row]
    differ("SuspendKind", row, f"Task f() {{ co_await {x}; }}", f"Task f() {{ co_await {y}; }}",
           ["changed f()"], pre=CORO, needs="stmt")

# ---------------------------------------------------------------------------
# GNU and C extensions.

node("StmtExpr.a", "int v = ({ 1; });", "int v = ({ 2; });", needs="stmt")
node("AddrLabelExpr.a", "void f() { l: m: void *p = &&l; }", "void f() { l: m: void *p = &&m; }",
     ["changed f()"], needs="stmt")
CH = "int v = __builtin_choose_expr({});"
node("ChooseExpr.aux", CH.format("1, a, b"), CH.format("0, a, b"), pre=VARS)
node("ChooseExpr.a", CH.format("1, a, b"), CH.format("2, a, b"), pre=VARS)
node("ChooseExpr.b-first", CH.format("1, a, b"), CH.format("1, c, b"), pre=VARS)
node("ChooseExpr.b-second", CH.format("1, a, b"), CH.format("1, a, c"), pre=VARS)
node("VAArgExpr.a", "int v = __builtin_va_arg(ap, int);", "int v = __builtin_va_arg(aq, int);",
     pre="__builtin_va_list ap, aq;")
node("VAArgExpr.b", "int v = __builtin_va_arg(ap, int);",
     "int v = __builtin_va_arg(ap, unsigned);", pre="__builtin_va_list ap;")
node("AtomicExpr.aux", "int v = __atomic_add_fetch(&a, 1, 0);",
     "int v = __atomic_fetch_add(&a, 1, 0);", pre=VARS)
node("AtomicExpr.a", "int v = __atomic_load_n(&a, 0);", "int v = __atomic_load_n(&b, 0);",
     pre=VARS)
SH = "v4 v = __builtin_shufflevector({});"
node("ShuffleVectorExpr.aux-mask-known",
     "template <int N> struct A { v4 w = __builtin_shufflevector(vv, vv, 0, 1, 2, 3); };",
     "template <int N> struct A { v4 w = __builtin_shufflevector(vv, vv, N, 1, 2, 3); };",
     ["changed A::w"], pre=VEC)
node("ShuffleVectorExpr.a", SH.format("vv, vv, 0, 1, 2, 3"), SH.format("vv, vv2, 0, 1, 2, 3"),
     pre=VEC)
node("ShuffleVectorExpr.b", "constexpr int K = 0;\n" + SH.format("vv, vv, K, 1, 2, 3"),
     "constexpr int K = 3;\n" + SH.format("vv, vv, K, 1, 2, 3"), ["changed K", "changed v"],
     pre=VEC, note="`ShuffleVectorExpr.b`: only the folded index differs at the use.")
node("ConvertVectorExpr.a", "f4 v = __builtin_convertvector(vv, f4);",
     "f4 v = __builtin_convertvector(vv2, f4);", pre=VEC)
node("ExtVectorElementExpr.bits-arrow", "int v = pv->x;", "int v = (*pv).x;", pre=VEC)
node("ExtVectorElementExpr.a", "int v = vv.x;", "int v = vv2.x;", pre=VEC)
node("ExtVectorElementExpr.b", "int v = vv.x;", "int v = vv.y;", pre=VEC)
GS = "int v = _Generic({});"
node("GenericSelectionExpr.a-control", GS.format("a, int: 1, default: 2"),
     GS.format("b, int: 1, default: 2"), pre=VARS)
node("GenericSelectionExpr.a-associations", GS.format("a, int: 1, long: 2"),
     GS.format("a, int: 1, short: 2"), pre=VARS)
node("GenericSelectionExpr.b", GS.format("a, int: 1, long: 1"), GS.format("l, int: 1, long: 1"),
     pre=VARS)
differ("GenericControl", "Expr", GS.format("a, int: 1, default: 2"),
       GS.format("int, int: 1, default: 2"), pre=VARS)
differ("GenericControl", "Expr.0", GS.format("a, int: 1, default: 2"),
       GS.format("b, int: 1, default: 2"), pre=VARS)
differ("GenericControl", "Type", GS.format("int, int: 1, default: 2"),
       GS.format("a, int: 1, default: 2"), pre=VARS)
differ("GenericControl", "Type.0", GS.format("int, int: 1, long: 1"),
       GS.format("long, int: 1, long: 1"))
differ("GenericAssoc", "ty", GS.format("a, int: 1, long: 2"), GS.format("a, int: 1, short: 2"),
       pre=VARS)
differ("GenericAssoc", "expr", GS.format("a, int: 1, default: 2"),
       GS.format("a, int: 1, default: 3"), pre=VARS)

# ---------------------------------------------------------------------------
# The pseudo-object rewrite: the synthesized comparison of a defaulted
# `operator<=>` (its body is a derived body).


# `M` has `==` and `<` and no `<=>`: each member pair compares by the
# synthesized form (`kiln_sema` class/compare/three_way.rs:76).
SO = "std::strong_ordering operator<=>(const X &) const = default;"
node("PseudoObjectExpr.a", f"struct X {{ M m; M k; {SO} }};", f"struct X {{ M k; M m; {SO} }};",
     ["changed X", "changed X::operator<=>(const X &) const"], pre=CMP, needs="stmt")
node("PseudoObjectExpr.b", f"struct X {{ M m; {SO} }};", f"struct X {{ int m; {SO} }};",
     ["changed X", "changed X::m", "changed X::operator<=>(const X &) const"], pre=CMP,
     needs="stmt", note="`PseudoObjectExpr.b`: the synthesized form on side A only.")

# The tests of WP-031 that wait on these kinds (`# needs: expr`).
differ("ColdExprData", "array_trait", UL.format("__array_rank(int[1][2])"),
       UL.format("__array_rank(int[1])"), note="written by `expr_misc::array_trait_extra`")
differ("ColdExprData", "offsetof_layout", OF.format("S, x[a]"), OF.format("S, y[a]"),
       pre=OFFSET + "\n" + VARS, note="written by `expr_misc::offsetof_layout`")
differ("ColdExprData", "offsetof_pattern-components", tmpl(TOF.format("n")),
       tmpl(TOF.format("m")), ["changed A::w"],
       note="written by `expr_misc::offsetof_components`")
equal("ColdExprData", "source_loc", "int v = __builtin_LINE();", "\nint v = __builtin_LINE();",
      note="the values of `SourceLocValues` (expr_misc.rs)")


def write():
    names = set()
    for root, short, row, a, b, expect, opts in TESTS:
        key = (root, short, row)
        if key in names:
            sys.exit(f"duplicate {key}")
        names.add(key)
        d = os.path.join(ROOT, root, short, row)
        os.makedirs(d, exist_ok=True)
        sec = "§7.4" if short == "ColdExprData" else "§7"
        lines = ['#include "side.h"']
        if "std" in opts:
            lines.insert(0, f"// std: {opts['std']}")
        note = opts.get("note")
        if note is None:
            note = f"`{row}`: " + ("only the position differs." if root == "equal"
                                     else "the fact differs.")
        lines.append(f"// {sec} {short}: {note}")
        if opts.get("pre"):
            lines.append(opts["pre"])
        lines.append("#ifdef B")
        lines.append(b)
        lines.append("#else")
        lines.append(a)
        lines.append("#endif")
        with open(os.path.join(d, "t.cpp"), "w") as f:
            f.write("\n".join(lines) + "\n")
        out = []
        if opts.get("needs"):
            out += ["# pending", f"# needs: {opts['needs']}"]
        out += expect
        exp = os.path.join(d, "expect.txt")
        if out:
            with open(exp, "w") as f:
                f.write("\n".join(out) + "\n")
        elif os.path.exists(exp):
            os.remove(exp)
    print(f"{len(TESTS)} tests written")


if __name__ == "__main__":
    write()
