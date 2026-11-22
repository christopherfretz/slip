#!/usr/bin/env python3
"""Generates the pair tests of WP-030 (attributes and annotations,
`spec/digest-kinds.md` §10.1, §10.5) under `crates/slip_digest/tests/`.

Run from anywhere: `python3 crates/slip_digest/tests/gen/attrs.py`. It
writes every directory of the table below (`t.cpp` and `expect.txt`) and
removes nothing; the generated directories are committed, and this script
is not part of the gate (WP-030 "Tests").

A test is `(root, short, row, a, b, expect, opts)`:
- `a`/`b`: the source of side A and side B (the `#ifdef B` branches);
- `expect`: the lines of `expect.txt`;
- `opts`: `pre` (source before the branches), `post` (after them),
  `std`, `errors`, `needs` (marks the test `# pending` on that area),
  `note` (the comment line after the section).
"""

import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)

TESTS = []


def t(root, short, row, a, b, expect, **opts):
    TESTS.append((root, short, row, a, b, expect, opts))


def differ(short, row, a, b, expect, **opts):
    t("differ", short, row, a, b, expect, **opts)


def equal(short, row, a, b, **opts):
    t("equal", short, row, a, b, [], **opts)


def kind(row, a, b, name, **opts):
    """`AttrKind/<row>`: the attribute on side B only."""
    opts.setdefault("note", "the attribute on one side only.")
    differ("AttrKind", row, a, b, [f"changed {name}"], **opts)


def fn(attr, name="f()", decl="void f()", **opts):
    """A function declaration with `attr` written before it on side B."""
    return (f"{decl};", f"{attr} {decl};", name)


def gnu(row, spelling, decl="void f()", name="f()", **opts):
    a, b, n = fn(f"__attribute__(({spelling}))", name, decl)
    kind(row, a, b, n, **opts)


# ---------------------------------------------------------------------------
# §10.1 AttrKind: one test per kind that is not `unbuilt`.

kind("ALIGNED", "int v;", "alignas(8) int v;", "v")
kind("ASSUME", "void f(int x) { }", "void f(int x) { [[assume(x > 0)]]; }", "f(int)",
     needs="stmt", std="c++23")
kind("CARRIES_DEPENDENCY", "void f();", "[[carries_dependency]] void f();", "f()")
kind("DEPRECATED", "void f();", "[[deprecated]] void f();", "f()")
kind("FALLTHROUGH",
     "void f(int x) { switch (x) { case 0: case 1: break; } }",
     "void f(int x) { switch (x) { case 0: [[fallthrough]]; case 1: break; } }",
     "f(int)", needs="stmt")
kind("INDETERMINATE", "void f() { int x; }", "void f() { int x [[indeterminate]]; }", "f()",
     needs="stmt", std="c++26")
kind("LIKELY", "void f(int x) { if (x) { } }", "void f(int x) { if (x) [[likely]] { } }",
     "f(int)", needs="stmt")
kind("UNLIKELY", "void f(int x) { if (x) { } }", "void f(int x) { if (x) [[unlikely]] { } }",
     "f(int)", needs="stmt")
kind("MAYBE_UNUSED", "int v;", "[[maybe_unused]] int v;", "v")
kind("NODISCARD", "int f();", "[[nodiscard]] int f();", "f()")
kind("NORETURN", "void f();", "[[noreturn]] void f();", "f()")
kind("NO_UNIQUE_ADDRESS", "struct E {}; struct S { E e; };",
     "struct E {}; struct S { [[no_unique_address]] E e; };", "S::e")
gnu("ALWAYS_INLINE", "always_inline", decl="inline void f()")
gnu("NOINLINE", "noinline")
kind("PACKED", "struct S { char c; int i; };",
     "struct __attribute__((packed)) S { char c; int i; };", "S")
gnu("USED", "used", decl="static void f() {}")
gnu("UNUSED", "unused")
gnu("WEAK", "weak")
gnu("VISIBILITY", 'visibility("hidden")')
kind("SECTION", "int v;", 'int v __attribute__((section("s")));', "v")
gnu("FORMAT", "format(printf, 1, 2)", decl="void f(const char *, ...)",
    name="f(const char *, ...)")
gnu("CONST", "const", decl="int f()")
gnu("PURE", "pure", decl="int f()")
gnu("COLD", "cold")
gnu("NOTHROW", "nothrow")
kind("ENABLE_IF", "void f(int x);",
     'void f(int x) __attribute__((enable_if(x > 0, "")));', "f(int)")
kind("DIAGNOSE_IF", "void f(int x);",
     'void f(int x) __attribute__((diagnose_if(x < 0, "m", "warning")));', "f(int)")
kind("TRIVIAL_ABI", "struct S { S(S &&); ~S(); };",
     "struct [[clang::trivial_abi]] S { S(S &&); ~S(); };", "S")
kind("PREFERRED_NAME",
     "template <class T> struct X {};",
     "template <class T> struct [[clang::preferred_name(I)]] X {};",
     "X", pre="template <class T> struct X;\nusing I = X<int>;")
kind("EXCLUDE_FROM_EXPLICIT_INSTANTIATION",
     "template <class T> struct X { void f(); };",
     "template <class T> struct X { __attribute__((exclude_from_explicit_instantiation)) void f(); };",
     "X::f()")
gnu("ABI_TAG", 'abi_tag("v1")')
gnu("INTERNAL_LINKAGE", "internal_linkage")
kind("LIFETIMEBOUND", "const int &f(const int &x);",
     "const int &f(const int &x [[clang::lifetimebound]]);", "f(const int &)", needs="decl_data")
gnu("NO_SANITIZE", 'no_sanitize("address")')
kind("MODE", "template <class T> struct X { T v; };",
     "template <class T> struct X { T v __attribute__((mode(DI))); };", "X::v")
kind("ASM_LABEL", "void f();", 'void f() asm("g");', "f()")
differ("AttrKind", "BUILTIN", "void f() { }", "void f() { __builtin_abs(1); }",
       ["added __builtin_abs(int)"], needs="stmt",
       note="`BUILTIN`: a lazily declared builtin carries the implicit attribute.")
gnu("MALLOC", "malloc", decl="void *f()")
gnu("ALLOC_SIZE", "alloc_size(1)", decl="void *f(int)", name="f(int)")
gnu("ALLOC_ALIGN", "alloc_align(1)", decl="void *f(int)", name="f(int)")
gnu("HOT", "hot")
kind("FLAG_ENUM", "enum E { A = 1 };", "enum __attribute__((flag_enum)) E { A = 1 };", "E")
kind("ENUM_EXTENSIBILITY", "enum E { A };",
     "enum __attribute__((enum_extensibility(open))) E { A };", "E")
kind("CORO_ONLY_DESTROY_WHEN_COMPLETE", "struct S {};",
     "struct [[clang::coro_only_destroy_when_complete]] S {};", "S")
gnu("OPTNONE", "optnone")
gnu("MINSIZE", "minsize")
gnu("NAKED", "naked")
kind("MAY_ALIAS", "struct S {};", "struct __attribute__((may_alias)) S {};", "S")
gnu("NONNULL", "nonnull", decl="void f(int *)", name="f(int *)")
gnu("RETURNS_NONNULL", "returns_nonnull", decl="int *f()")
kind("INIT_PRIORITY", "S s;", "S s __attribute__((init_priority(200)));", "s",
     pre="struct S { S(); };")
kind("LOOP_HINT", "void f() { for (;;) { } }",
     "void f() {\n#pragma clang loop unroll(enable)\n  for (;;) { }\n}", "f()", needs="stmt")
kind("UNINITIALIZED", "void f() { int x; }", "void f() { int x [[clang::uninitialized]]; }",
     "f()", needs="stmt")
kind("TYPE_VISIBILITY", "struct S {};",
     'struct __attribute__((type_visibility("hidden"))) S {};', "S")
kind("TYPE_NULLABLE", "class S {};", "class _Nullable S {};", "S")
gnu("TARGET", 'target("avx")')
gnu("GNU_INLINE", "gnu_inline", decl="extern inline void f() {}")
gnu("NODEBUG", "nodebug")
gnu("MIN_VECTOR_WIDTH", "min_vector_width(128)")
kind("WEAKREF", "static void g();", 'static void g() __attribute__((weakref("h")));', "g()")
kind("ALIAS", 'extern "C" void g();', 'extern "C" void g() __attribute__((alias("h")));', "g()",
     pre='extern "C" void h() {}')
gnu("ANALYZER_NORETURN", "analyzer_noreturn")
gnu("DISABLE_TAIL_CALLS", "disable_tail_calls")
kind("IFUNC", 'extern "C" void g();', 'extern "C" void g() __attribute__((ifunc("r")));', "g()",
     pre='extern "C" void (*r())() { return 0; }')
gnu("ARTIFICIAL", "artificial", decl="inline void f() {}")
gnu("CONSTRUCTOR", "constructor")
gnu("DESTRUCTOR", "destructor")
kind("TLS_MODEL", "thread_local int v;",
     'thread_local int v __attribute__((tls_model("local-exec")));', "v")
gnu("RETURNS_TWICE", "returns_twice")
kind("PASS_OBJECT_SIZE", "void f(void *p);",
     "void f(void *p __attribute__((pass_object_size(0))));", "f(void *)", needs="decl_data")
gnu("OVERLOADABLE", "overloadable")
kind("MUSTTAIL", "int g(); int f() { return g(); }",
     "int g(); int f() { [[clang::musttail]] return g(); }", "f()", needs="stmt")
kind("TRANSPARENT_UNION", "union U { int *a; };",
     "union __attribute__((transparent_union)) U { int *a; };", "U")
kind("SELECTANY", "inline int v = 1;", "__attribute__((selectany)) inline int v = 1;", "v")
kind("MS_STRUCT", "struct S {};", "struct __attribute__((ms_struct)) S {};", "S")
kind("GCC_STRUCT", "struct S {};", "struct __attribute__((gcc_struct)) S {};", "S")
gnu("RETAIN", "retain,used", decl="static void f() {}")
kind("NO_DESTROY", "S s;", "[[clang::no_destroy]] S s;", "s", pre="struct S { ~S(); };")
kind("ALWAYS_DESTROY", "S s;", "[[clang::always_destroy]] S s;", "s", pre="struct S { ~S(); };")
gnu("NO_BUILTIN", 'no_builtin("memcpy")', decl="void f() {}")
gnu("TARGET_CLONES", 'target_clones("avx2", "default")', decl="void f() {}")
gnu("NO_STACK_PROTECTOR", "no_stack_protector")
gnu("PATCHABLE_FUNCTION_ENTRY", "patchable_function_entry(1, 0)")
gnu("ZERO_CALL_USED_REGS", 'zero_call_used_regs("used")')
gnu("SPECULATIVE_LOAD_HARDENING", "speculative_load_hardening")
gnu("NO_SPECULATIVE_LOAD_HARDENING", "no_speculative_load_hardening")
gnu("NO_SPLIT_STACK", "no_split_stack")
gnu("NO_INSTRUMENT_FUNCTION", "no_instrument_function")
gnu("NO_PROFILE_INSTRUMENT_FUNCTION", "no_profile_instrument_function")
gnu("XRAY_INSTRUMENT", "xray_always_instrument")
gnu("XRAY_LOG_ARGS", "xray_always_instrument, xray_log_args(1)", decl="void f(int)",
    name="f(int)")
gnu("DISABLE_SANITIZER_INSTRUMENTATION", "disable_sanitizer_instrumentation")
gnu("FUNCTION_RETURN", 'function_return("thunk-extern")')
gnu("CFI_CANONICAL_JUMP_TABLE", "cfi_canonical_jump_table")
gnu("FORCE_ALIGN_ARG_POINTER", "force_align_arg_pointer")
gnu("FLATTEN", "flatten")
gnu("LEAF", "leaf")
gnu("NOMERGE", "nomerge")
gnu("NOT_TAIL_CALLED", "not_tail_called")
gnu("NODUPLICATE", "noduplicate")
gnu("CONVERGENT", "convergent")
gnu("NOUWTABLE", "nouwtable")
gnu("ASSUME_ALIGNED", "assume_aligned(16)", decl="void *f()")
kind("ALIGN_VALUE", "void f(int *p);", "void f(int *p __attribute__((align_value(16))));",
     "f(int *)", needs="decl_data")
kind("CALLBACK", "void f(void (*cb)(int), int x);",
     "void f(void (*cb)(int), int x) __attribute__((callback(cb, x)));", "f(void (*)(int), int)")
kind("NOESCAPE", "void f(int *p);", "void f(int *p __attribute__((noescape)));", "f(int *)", needs="decl_data")
kind("MAYBE_UNDEF", "void f(int x);", "void f(int x __attribute__((maybe_undef)));", "f(int)", needs="decl_data")
kind("CODE_ALIGN", "void f() { for (;;) { } }",
     "void f() { [[clang::code_align(16)]] for (;;) { } }", "f()", needs="stmt")
kind("LOADER_UNINITIALIZED", "int v;", "[[clang::loader_uninitialized]] int v;", "v")
kind("LTO_VISIBILITY_PUBLIC", "struct S {};", "struct [[clang::lto_visibility_public]] S {};",
     "S")
kind("OMP_ASSUME", "void f();", '[[omp::assume("ompx_a")]] void f();', "f()")
kind("STANDALONE_DEBUG", "struct S {};", "struct __attribute__((standalone_debug)) S {};", "S")
kind("CORO_AWAIT_ELIDABLE", "struct S {};", "struct [[clang::coro_await_elidable]] S {};", "S")
kind("CORO_AWAIT_ELIDABLE_ARGUMENT", "void f(int x);",
     "void f([[clang::coro_await_elidable_argument]] int x);", "f(int)", needs="decl_data")
kind("GSL_OWNER", "struct S {};", "struct [[gsl::Owner]] S {};", "S")
kind("GSL_POINTER", "struct S {};", "struct [[gsl::Pointer]] S {};", "S")
gnu("UNAVAILABLE", "unavailable")
kind("REQUIRE_CONSTANT_INIT", "int v = 1;", "[[clang::require_constant_initialization]] int v = 1;",
     "v")
kind("ANNOTATE", "void f();", '[[clang::annotate("x")]] void f();', "f()")
kind("NONSTRING", "char s[4];", "char s[4] __attribute__((nonstring));", "s")
kind("NOCOMMON", "int v;", "int v __attribute__((nocommon));", "v")
kind("PRAGMA_CLANG_SECTION", "int v;", '#pragma clang section bss = "s"\nint v;', "v")
kind("CODE_MODEL", "int v;", 'int v __attribute__((model("small")));', "v")
kind("X86_INTERRUPT", "struct F; void f(F *);",
     "struct F; __attribute__((interrupt)) void f(F *);", "f(F *)")
gnu("WEAK_IMPORT", "weak_import")
kind("SWIFT_PARAM_ABI", "void f(void *p) __attribute__((swiftcall));",
     "void f(void *p __attribute__((swift_context))) __attribute__((swiftcall));", "f(void *)", needs="decl_data")

# ---------------------------------------------------------------------------
# §10.1 AttrNode: the rows every node has.

differ("AttrNode", "kind", "__attribute__((cold)) void f();", "__attribute__((hot)) void f();",
       ["changed f()"], note="`kind`: one kind for another.")
differ("AttrNode", "syntax", "__attribute__((cold)) void f();", "[[gnu::cold]] void f();",
       ["changed f()"], note="`syntax`: `[[gnu::cold]]` against `__attribute__((cold))`.")
differ("AttrNode", "flags-implicit",
       'extern "C" int printf(const char *, ...);',
       'extern "C" int printf(const char *, ...) __attribute__((format(printf, 1, 2)));',
       ["changed printf(const char *, ...)"],
       note="`flags-implicit`: the `format` Kiln gives `printf` against the same one written.")
equal("AttrNode", "flags-gnu-scoped", "[[gnu::deprecated]] void f();", "[[deprecated]] void f();",
      note="`flags-gnu-scoped`: the `gnu::` scope is spelling.")

# Per-kind rows.

differ("AttrNode", "ASSUME.flags-ignored",
       "void f(int x) { [[assume(x > 0)]]; }", "int g(); void f(int x) { [[assume(g() > 0)]]; }",
       ["changed f(int)"], needs="stmt", std="c++23")
equal("AttrNode", "FALLTHROUGH.flags-scoped",
      "void f(int x) { switch (x) { case 0: [[fallthrough]]; case 1: break; } }",
      "void f(int x) { switch (x) { case 0: [[clang::fallthrough]]; case 1: break; } }",
      needs="stmt")
equal("AttrNode", "NODISCARD.flags-clang-scoped",
      "[[clang::warn_unused_result]] int f();", "[[nodiscard]] int f();")
equal("AttrNode", "LIFETIMEBOUND.flags-args",
      "const int &f(const int &x [[clang::lifetimebound]]);",
      "const int &f(const int &x [[clang::lifetimebound()]]);", needs="decl_data")
differ("AttrNode", "CONSTRUCTOR.flags-priority-dependent",
       "template <int N> struct X { __attribute__((constructor(101))) static void f(); };",
       "template <int N> struct X { __attribute__((constructor(N))) static void f(); };",
       ["changed X::f()"])
differ("AttrNode", "DESTRUCTOR.flags-priority-dependent",
       "template <int N> struct X { __attribute__((destructor(101))) static void f(); };",
       "template <int N> struct X { __attribute__((destructor(N))) static void f(); };",
       ["changed X::f()"])
differ("AttrNode", "MODE.flags-deferred",
       "template <class T> struct X { int v __attribute__((mode(DI))); };",
       "template <class T> struct X { T v __attribute__((mode(DI))); };",
       ["changed X::v"])
differ("AttrNode", "GSL_OWNER.flags-has-type", "struct [[gsl::Owner]] S {};",
       "struct [[gsl::Owner(int)]] S {};", ["changed S"])
differ("AttrNode", "GSL_POINTER.flags-has-type", "struct [[gsl::Pointer]] S {};",
       "struct [[gsl::Pointer(int)]] S {};", ["changed S"])
differ("AttrNode", "DIAGNOSE_IF.flags-error",
       'void f(int x) __attribute__((diagnose_if(x < 0, "m", "warning")));',
       'void f(int x) __attribute__((diagnose_if(x < 0, "m", "error")));',
       ["changed f(int)"])
differ("AttrNode", "DIAGNOSE_IF.flags-arg-dependent",
       'void f(int x) __attribute__((diagnose_if(true, "m", "warning")));',
       'void f(int x) __attribute__((diagnose_if(x < 0, "m", "warning")));',
       ["changed f(int)"])
differ("AttrNode", "DIAGNOSE_IF.flags-group",
       'void f(int x) __attribute__((diagnose_if(x < 0, "m", "warning")));',
       'void f(int x) __attribute__((diagnose_if(x < 0, "m", "warning", "format")));',
       ["changed f(int)"])
differ("AttrNode", "DEPRECATED.a", "[[deprecated]] void f();", '[[deprecated("m")]] void f();',
       ["changed f()"], note="`DEPRECATED.a`: a message against none.")
differ("AttrNode", "NODISCARD.a", "[[nodiscard]] int f();", '[[nodiscard("m")]] int f();',
       ["changed f()"])
differ("AttrNode", "UNAVAILABLE.a", "__attribute__((unavailable)) void f();",
       '__attribute__((unavailable("m"))) void f();', ["changed f()"])
differ("AttrNode", "ALLOC_SIZE.a", "void *f(int, int) __attribute__((alloc_size(1)));",
       "void *f(int, int) __attribute__((alloc_size(2)));", ["changed f(int, int)"], needs="expr")
differ("AttrNode", "ALLOC_SIZE.b", "void *f(int, int) __attribute__((alloc_size(1)));",
       "void *f(int, int) __attribute__((alloc_size(1, 2)));", ["changed f(int, int)"])
differ("AttrNode", "ALLOC_ALIGN.a", "void *f(int, int) __attribute__((alloc_align(1)));",
       "void *f(int, int) __attribute__((alloc_align(2)));", ["changed f(int, int)"], needs="expr")
differ("AttrNode", "ENABLE_IF.a", 'void f(int x) __attribute__((enable_if(x > 0, "")));',
       'void f(int x) __attribute__((enable_if(x > 1, "")));', ["changed f(int)"], needs="expr")
differ("AttrNode", "ENABLE_IF.b", 'void f(int x) __attribute__((enable_if(x > 0, "m")));',
       'void f(int x) __attribute__((enable_if(x > 0, "n")));', ["changed f(int)"], needs="expr")
differ("AttrNode", "DIAGNOSE_IF.a",
       'void f(int x) __attribute__((diagnose_if(x < 0, "m", "warning")));',
       'void f(int x) __attribute__((diagnose_if(x < 1, "m", "warning")));',
       ["changed f(int)"], needs="expr")
differ("AttrNode", "DIAGNOSE_IF.b",
       'void f(int x) __attribute__((diagnose_if(x < 0, "m", "warning")));',
       'void f(int x) __attribute__((diagnose_if(x < 0, "n", "warning")));',
       ["changed f(int)"], needs="expr")
differ("AttrNode", "ALIGNED.a", "struct __attribute__((aligned)) S {};",
       "struct __attribute__((aligned(4))) S {};", ["changed S"])
differ("AttrNode", "ALIGNED.b", "alignas(int) char v;", "alignas(long) char v;", ["changed v"])
differ("AttrNode", "ASSUME.a", "void f(int x) { [[assume(x > 0)]]; }",
       "void f(int x) { [[assume(x > 1)]]; }", ["changed f(int)"], needs="stmt", std="c++23")
differ("AttrNode", "PREFERRED_NAME.b",
       "template <class T> struct [[clang::preferred_name(I)]] X {};",
       "template <class T> struct [[clang::preferred_name(L)]] X {};",
       ["changed X"], pre="template <class T> struct X;\nusing I = X<int>;\nusing L = X<long>;")
differ("AttrNode", "NONNULL.a", "void f(int *, int *) __attribute__((nonnull));",
       "void f(int *, int *) __attribute__((nonnull(1)));", ["changed f(int *, int *)"])
differ("AttrNode", "ASM_LABEL.a", 'void f() asm("g");', 'void f() asm("h");', ["changed f()"],
       needs="value")
differ("AttrNode", "SECTION.a", 'int v __attribute__((section("s")));',
       'int v __attribute__((section("t")));', ["changed v"], needs="value")
differ("AttrNode", "TARGET.a", '__attribute__((target("avx"))) void f();',
       '__attribute__((target("sse4.2"))) void f();', ["changed f()"], needs="value")
differ("AttrNode", "OMP_ASSUME.a", '[[omp::assume("ompx_a")]] void f();',
       '[[omp::assume("ompx_b")]] void f();', ["changed f()"], needs="value")
differ("AttrNode", "PRAGMA_CLANG_SECTION.a", '#pragma clang section bss = "s"\nint v;',
       '#pragma clang section bss = "t"\nint v;', ["changed v"], needs="value")
differ("AttrNode", "PRAGMA_CLANG_SECTION.b", '#pragma clang section bss = "s"\nint v;',
       '#pragma clang section data = "s"\nint v;', ["changed v"])
differ("AttrNode", "ALIAS.a", 'extern "C" void g() __attribute__((alias("h")));',
       'extern "C" void g() __attribute__((alias("i")));', ["changed g()"], needs="value",
       pre='extern "C" void h() {}\nextern "C" void i() {}')
differ("AttrNode", "IFUNC.a", 'extern "C" void g() __attribute__((ifunc("r")));',
       'extern "C" void g() __attribute__((ifunc("q")));', ["changed g()"], needs="value",
       pre='extern "C" void (*r())() { return 0; }\nextern "C" void (*q())() { return 0; }')
t("indeterminate", "AttrNode", "CLEANUP.a",
  "void c(int *); void f() { int x __attribute__((cleanup(c))); }", None,
  ["indeterminate f()"], needs="stmt",
  note="`CLEANUP.a`: the node does not say whether `a` is a declaration or an expression.")
differ("AttrNode", "ANNOTATE.a", '[[clang::annotate("x")]] void f();',
       '[[clang::annotate("y")]] void f();', ["changed f()"], needs="expr")
differ("AttrNode", "ANNOTATE.b", '[[clang::annotate("x")]] void f();',
       '[[clang::annotate("x", 1)]] void f();', ["changed f()"])
differ("AttrNode", "CALLBACK.a",
       "void f(void (*cb)(int, int), int x, int y) __attribute__((callback(cb, x, y)));",
       "void f(void (*cb)(int, int), int x, int y) __attribute__((callback(cb, y, x)));",
       ["changed f(void (*)(int, int), int, int)"])
differ("AttrNode", "VISIBILITY.a", '__attribute__((visibility("hidden"))) void f();',
       '__attribute__((visibility("protected"))) void f();', ["changed f()"])
equal("AttrNode", "VISIBILITY.b", '__attribute__((visibility("hidden"))) void f();',
      '__attribute__((visibility("internal"))) void f();')
differ("AttrNode", "TYPE_VISIBILITY.a", 'struct __attribute__((type_visibility("hidden"))) S {};',
       'struct __attribute__((type_visibility("protected"))) S {};', ["changed S"])
differ("AttrNode", "FORMAT.a", "__attribute__((format(printf, 1, 2))) void f(const char *, ...);",
       "__attribute__((format(scanf, 1, 2))) void f(const char *, ...);",
       ["changed f(const char *, ...)"])
differ("AttrNode", "FORMAT.b", "__attribute__((format(printf, 1, 2))) void f(const char *, ...);",
       "__attribute__((format(printf, 1, 0))) void f(const char *, ...);",
       ["changed f(const char *, ...)"])
differ("AttrNode", "LIFETIMEBOUND.a",
       "struct S { const int &f(const int &x [[clang::lifetimebound]]); };",
       "struct S { const int &f(const int &x) [[clang::lifetimebound]]; };",
       ["changed S::f(const int &)"], needs="decl_data")
differ("AttrNode", "NO_SANITIZE.a", '__attribute__((no_sanitize("address"))) void f();',
       '__attribute__((no_sanitize("thread"))) void f();', ["changed f()"])
equal("AttrNode", "NO_SANITIZE.b-spelling", '__attribute__((no_sanitize("address"))) void f();',
      "__attribute__((no_sanitize_address)) void f();")
differ("AttrNode", "NO_SANITIZE.b-coverage", '__attribute__((no_sanitize("address"))) void f();',
       '__attribute__((no_sanitize("address", "coverage"))) void f();', ["changed f()"])
differ("AttrNode", "ENUM_EXTENSIBILITY.a",
       "enum __attribute__((enum_extensibility(open))) E { A };",
       "enum __attribute__((enum_extensibility(closed))) E { A };", ["changed E"])
differ("AttrNode", "INIT_PRIORITY.a", "S s __attribute__((init_priority(200)));",
       "S s __attribute__((init_priority(300)));", ["changed s"], pre="struct S { S(); };")
differ("AttrNode", "MIN_VECTOR_WIDTH.a", "__attribute__((min_vector_width(128))) void f();",
       "__attribute__((min_vector_width(256))) void f();", ["changed f()"])
differ("AttrNode", "CONSTRUCTOR.a-priority", "__attribute__((constructor(101))) void f();",
       "__attribute__((constructor(102))) void f();", ["changed f()"])
differ("AttrNode", "CONSTRUCTOR.a-expression",
       "template <int N> struct X { __attribute__((constructor(N))) static void f(); };",
       "template <int N> struct X { __attribute__((constructor(N + 1))) static void f(); };",
       ["changed X::f()"], needs="expr")
differ("AttrNode", "DESTRUCTOR.a-priority", "__attribute__((destructor(101))) void f();",
       "__attribute__((destructor(102))) void f();", ["changed f()"])
differ("AttrNode", "DESTRUCTOR.a-expression",
       "template <int N> struct X { __attribute__((destructor(N))) static void f(); };",
       "template <int N> struct X { __attribute__((destructor(N + 1))) static void f(); };",
       ["changed X::f()"], needs="expr")
differ("AttrNode", "TLS_MODEL.a", 'thread_local int v __attribute__((tls_model("local-exec")));',
       'thread_local int v __attribute__((tls_model("initial-exec")));', ["changed v"])
differ("AttrNode", "PASS_OBJECT_SIZE.a", "void f(void *p __attribute__((pass_object_size(0))));",
       "void f(void *p __attribute__((pass_object_size(1))));", ["changed f(void *)"], needs="decl_data")
differ("AttrNode", "PASS_OBJECT_SIZE.b", "void f(void *p __attribute__((pass_object_size(0))));",
       "void f(void *p __attribute__((pass_dynamic_object_size(0))));", ["changed f(void *)"], needs="decl_data")
differ("AttrNode", "PATCHABLE_FUNCTION_ENTRY.a",
       "__attribute__((patchable_function_entry(1, 0))) void f();",
       "__attribute__((patchable_function_entry(2, 0))) void f();", ["changed f()"])
differ("AttrNode", "PATCHABLE_FUNCTION_ENTRY.b",
       "__attribute__((patchable_function_entry(2, 0))) void f();",
       "__attribute__((patchable_function_entry(2, 1))) void f();", ["changed f()"])
differ("AttrNode", "ZERO_CALL_USED_REGS.a", '__attribute__((zero_call_used_regs("used"))) void f();',
       '__attribute__((zero_call_used_regs("all"))) void f();', ["changed f()"])
differ("AttrNode", "XRAY_INSTRUMENT.a", "__attribute__((xray_always_instrument)) void f();",
       "__attribute__((xray_never_instrument)) void f();", ["changed f()"])
differ("AttrNode", "XRAY_LOG_ARGS.a",
       "__attribute__((xray_always_instrument, xray_log_args(1))) void f(int, int);",
       "__attribute__((xray_always_instrument, xray_log_args(2))) void f(int, int);",
       ["changed f(int, int)"])
differ("AttrNode", "FUNCTION_RETURN.a", '__attribute__((function_return("keep"))) void f();',
       '__attribute__((function_return("thunk-extern"))) void f();', ["changed f()"])
differ("AttrNode", "ASSUME_ALIGNED.a", "void *f() __attribute__((assume_aligned(16)));",
       "void *f() __attribute__((assume_aligned(32)));", ["changed f()"])
differ("AttrNode", "ASSUME_ALIGNED.b", "void *f() __attribute__((assume_aligned(16)));",
       "void *f() __attribute__((assume_aligned(16, 4)));", ["changed f()"])
differ("AttrNode", "ALIGN_VALUE.a", "void f(int *p __attribute__((align_value(16))));",
       "void f(int *p __attribute__((align_value(32))));", ["changed f(int *)"], needs="decl_data")
differ("AttrNode", "CODE_ALIGN.a", "void f() { [[clang::code_align(16)]] for (;;) { } }",
       "void f() { [[clang::code_align(32)]] for (;;) { } }", ["changed f()"], needs="stmt")
differ("AttrNode", "CODE_MODEL.a", 'int v __attribute__((model("small")));',
       'int v __attribute__((model("large")));', ["changed v"])
differ("AttrNode", "SWIFT_PARAM_ABI.a",
       "void f(void *p __attribute__((swift_context))) __attribute__((swiftcall));",
       "void f(void *p __attribute__((swift_async_context))) __attribute__((swiftcall));",
       ["changed f(void *)"], needs="decl_data")
differ("AttrNode", "ABI_TAG.a", '__attribute__((abi_tag("v1"))) void f();',
       '__attribute__((abi_tag("v2"))) void f();', ["changed f()"])
differ("AttrNode", "NO_BUILTIN.a", '__attribute__((no_builtin("memcpy"))) void f() {}',
       '__attribute__((no_builtin("memset"))) void f() {}', ["changed f()"])
differ("AttrNode", "TARGET_CLONES.a", '__attribute__((target_clones("avx2", "default"))) void f() {}',
       '__attribute__((target_clones("sse4.2", "default"))) void f() {}', ["changed f()"])
differ("AttrNode", "TARGET_CLONES.b", '__attribute__((target_clones("avx2", "default"))) void f() {}',
       '__attribute__((target_clones("avx2", "avx2", "default"))) void f() {}', ["changed f()"])
differ("AttrNode", "LOOP_HINT.a",
       "void f() {\n#pragma clang loop unroll_count(2)\n  for (;;) { }\n}",
       "void f() {\n#pragma clang loop unroll_count(4)\n  for (;;) { }\n}", ["changed f()"],
       needs="stmt")
differ("AttrNode", "LOOP_HINT.b",
       "void f() {\n#pragma clang loop unroll(enable)\n  for (;;) { }\n}",
       "void f() {\n#pragma clang loop unroll(disable)\n  for (;;) { }\n}", ["changed f()"],
       needs="stmt")
differ("AttrNode", "MODE.a", "template <class T> struct X { T v __attribute__((mode(DI))); };",
       "template <class T> struct X { T v __attribute__((mode(SI))); };", ["changed X::v"])
equal("AttrNode", "MODE.b", "template <class T> struct X { T v __attribute__((mode(DI))); };",
      "template <class T> struct X { T v __attribute__((mode(__DI__))); };")
differ("AttrNode", "GSL_OWNER.b", "struct [[gsl::Owner(int)]] S {};",
       "struct [[gsl::Owner(long)]] S {};", ["changed S"])
differ("AttrNode", "GSL_POINTER.b", "struct [[gsl::Pointer(int)]] S {};",
       "struct [[gsl::Pointer(long)]] S {};", ["changed S"])

# ---------------------------------------------------------------------------
# §10.1 AttrSyntax, LoopHint*, SanitizerSet, AttrArena.

differ("AttrSyntax", "Cxx11", "__attribute__((noreturn)) void f();", "[[noreturn]] void f();",
       ["changed f()"])
differ("AttrSyntax", "Gnu", "[[gnu::cold]] void f();", "__attribute__((cold)) void f();",
       ["changed f()"])
differ("AttrSyntax", "Keyword", "__attribute__((aligned(8))) int v;", "alignas(8) int v;",
       ["changed v"])
differ("AttrSyntax", "Pragma", "__attribute__((weak)) void f();", "void f();\n#pragma weak f",
       ["changed f()"], note="`Pragma`: `#pragma weak` against `__attribute__((weak))`.")

LOOP = "void f() {{\n#pragma {p}\n  for (;;) {{ }}\n}}"


def loop(short, row, a, b, **opts):
    differ(short, row, LOOP.format(p=a), LOOP.format(p=b), ["changed f()"], needs="stmt", **opts)


loop("LoopHint", "option", "clang loop vectorize(enable)", "clang loop interleave(enable)")
loop("LoopHint", "state", "clang loop unroll(enable)", "clang loop unroll(full)")
loop("LoopHint", "spelling", "clang loop unroll(full)", "unroll")
loop("LoopHintOption", "Vectorize", "clang loop interleave(enable)", "clang loop vectorize(enable)")
loop("LoopHintOption", "VectorizeWidth", "clang loop interleave_count(4)",
     "clang loop vectorize_width(4)")
loop("LoopHintOption", "Interleave", "clang loop vectorize(enable)", "clang loop interleave(enable)")
loop("LoopHintOption", "InterleaveCount", "clang loop vectorize_width(4)",
     "clang loop interleave_count(4)")
loop("LoopHintOption", "Unroll", "clang loop vectorize(enable)", "clang loop unroll(enable)")
loop("LoopHintOption", "UnrollCount", "clang loop interleave_count(4)", "clang loop unroll_count(4)")
loop("LoopHintOption", "UnrollAndJam", "unroll", "unroll_and_jam")
loop("LoopHintOption", "UnrollAndJamCount", "unroll 4", "unroll_and_jam 4")
loop("LoopHintOption", "PipelineDisabled", "clang loop vectorize(disable)",
     "clang loop pipeline(disable)")
loop("LoopHintOption", "PipelineInitiationInterval", "clang loop unroll_count(4)",
     "clang loop pipeline_initiation_interval(4)")
loop("LoopHintOption", "Distribute", "clang loop vectorize(enable)", "clang loop distribute(enable)")
loop("LoopHintOption", "VectorizePredicate", "clang loop vectorize(enable)",
     "clang loop vectorize_predicate(enable)")
loop("LoopHintState", "Enable", "clang loop unroll(disable)", "clang loop unroll(enable)")
loop("LoopHintState", "Disable", "clang loop unroll(enable)", "clang loop unroll(disable)")
loop("LoopHintState", "Numeric", "clang loop vectorize_width(scalable)",
     "clang loop vectorize_width(4)")
loop("LoopHintState", "FixedWidth", "clang loop vectorize_width(4, scalable)",
     "clang loop vectorize_width(4, fixed)")
loop("LoopHintState", "ScalableWidth", "clang loop vectorize_width(4, fixed)",
     "clang loop vectorize_width(4, scalable)")
loop("LoopHintState", "AssumeSafety", "clang loop vectorize(enable)",
     "clang loop vectorize(assume_safety)")
loop("LoopHintState", "Full", "clang loop unroll(enable)", "clang loop unroll(full)")
loop("LoopHintSpelling", "ClangLoop", "unroll", "clang loop unroll(full)")
loop("LoopHintSpelling", "Unroll", "clang loop unroll(full)", "unroll")
loop("LoopHintSpelling", "Nounroll", "clang loop unroll(disable)", "nounroll")
loop("LoopHintSpelling", "UnrollAndJam", "unroll", "unroll_and_jam")
loop("LoopHintSpelling", "NounrollAndJam", "nounroll", "nounroll_and_jam")

differ("SanitizerSet", "0", '__attribute__((no_sanitize("address"))) void f();',
       '__attribute__((no_sanitize("address", "thread"))) void f();', ["changed f()"],
       note="`0`: one sanitizer more in the set.")

differ("AttrArena", "decl_attrs", "void f();", "__attribute__((cold)) void f();",
       ["changed f()"], note="`decl_attrs`: a declaration's attributes are in its unit.")

# ---------------------------------------------------------------------------
# §10.5 Annotation, AnnotArena.

differ("AnnotArena", "of_decl", "void f();", "[[=1]] void f();", ["changed f()"], std="c++26")
differ("AnnotArena", "of_base", "struct Base {}; struct D : Base {};",
       "struct Base {}; struct D : [[=1]] Base {};", ["changed D"], std="c++26")
differ("Annotation", "written_type", "[[=1]] void f();", "[[=1L]] void f();", ["changed f()"],
       std="c++26")
differ("Annotation", "value", "[[=1]] void f();", "[[=2]] void f();", ["changed f()"],
       std="c++26", needs="value")
differ("Annotation", "expr", "[[=1]] void f();", "[[=1 + 0]] void f();", ["changed f()"],
       std="c++26", needs="expr")

# ---------------------------------------------------------------------------
# The prose rules of §10.1 (`_` directories, which the tables test ignores).

differ("_", "attrs-chain-second", "void f();\nvoid f();",
       "void f();\n__attribute__((cold)) void f();", ["changed f()"],
       note="the encoding `attrs`: an attribute on the second declaration is in the unit.")
equal("_", "attrs-chain-first", "void f();\n__attribute__((cold)) void f();",
      "__attribute__((cold)) void f();\nvoid f();",
      note="the encoding `attrs`: the same attribute on the first declaration instead.")
equal("_", "attrs-inherited-asm-label", 'void f() asm("g");',
      'void f() asm("g");\nvoid f();',
      note="the encoding `attrs`: the `inherited` copy of an asm label adds nothing.")
differ("_", "attrs-enable-if-order",
       'void f(int x) __attribute__((enable_if(x > 0, "a"), enable_if(x > 1, "b")));',
       'void f(int x) __attribute__((enable_if(x > 1, "b"), enable_if(x > 0, "a")));',
       ["changed f(int)"], needs="expr",
       note="the encoding `attrs`: the order is kept (two `enable_if` swapped).")
differ("_", "attrs-order", "__attribute__((cold, noinline)) void f();",
       "__attribute__((noinline, cold)) void f();", ["changed f()"],
       note="the encoding `attrs`: the stored order is kept.")


def write():
    names = set()
    for root, short, row, a, b, expect, opts in TESTS:
        key = (root, short, row)
        if key in names:
            sys.exit(f"duplicate {key}")
        names.add(key)
        d = os.path.join(ROOT, root, short, row)
        os.makedirs(d, exist_ok=True)
        sec = "§10.5" if short in ("Annotation", "AnnotArena") else "§10.1"
        lines = ['#include "side.h"']
        if "std" in opts:
            lines.insert(0, f"// std: {opts['std']}")
        if opts.get("errors"):
            lines.insert(0, "// errors: yes")
        label = short if short != "_" else "rule"
        note = opts.get("note")
        if note is None:
            note = f"`{row}`: " + ("only the spelling differs." if root == "equal"
                                     else "the fact differs.")
        if not note.startswith("`") and short != "_":
            note = f"`{row}`: {note}"
        lines.append(f"// {sec} {label}: {note}")
        if opts.get("pre"):
            lines.append(opts["pre"])
        if b is None:
            lines.append(a)
        else:
            lines.append("#ifdef B")
            lines.append(b)
            if a:
                lines.append("#else")
                lines.append(a)
            lines.append("#endif")
        if opts.get("post"):
            lines.append(opts["post"])
        with open(os.path.join(d, "t.cpp"), "w") as f:
            f.write("\n".join(lines) + "\n")
        out = []
        if opts.get("needs"):
            out += ["# pending", f"# needs: {opts['needs']}"]
        if root != "equal" or not opts.get("needs"):
            out += [e for e in expect if e is not None]
        exp = os.path.join(d, "expect.txt")
        if out:
            with open(exp, "w") as f:
                f.write("\n".join(out) + "\n")
        elif os.path.exists(exp):
            os.remove(exp)
    print(f"{len(TESTS)} tests written")


if __name__ == "__main__":
    write()
