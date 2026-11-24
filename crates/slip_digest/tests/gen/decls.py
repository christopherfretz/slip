#!/usr/bin/env python3
"""Generates the pair tests of WP-039 (the declaration header, the names
and the keys: `spec/digest-kinds.md` §3.2, §5.3, §10.2, and the rows other
areas left) under `crates/slip_digest/tests/`.

Run from anywhere: `python3 crates/slip_digest/tests/gen/decls.py`. It
writes every directory of the lists below (`t.cpp`, and `expect.txt`
when it has a line) and removes nothing; the generated directories are
committed, and this script is not part of the gate.

A test is `(root, short, row, a, b, expect, opts)`:
- `a`/`b`: the source of side A and side B (the `#ifdef B` branches);
  `a` empty means side A has nothing there;
- `expect`: the lines of `expect.txt`;
- `opts`: `pre` (source before the branches), `post` (after them),
  `std`, `needs` (marks the test `# pending` on that area, `why` says
  why), `note` (the comment line), `include` (a support header).

The form of each `DeclKind` test (WP-039 "Interface") is said by its
note: "kind changed" (side B declares the same key with another kind,
the only form that fails when the tag is written as a constant),
"added" (side B declares what side A lacks), "through <unit>" (a kind
that is no unit of its own, shown by the unit that holds it) or
"written" (`equal`: a kind the table writes as another).
"""

import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)

TESTS = []

SECTION = {
    "DeclKind": "§5.3",
    "DeclNode": "§5.3",
    "DeclFlags": "§5.3",
    "DeclMisc": "§5.3",
    "DeclName": "§10.2",
    "DeclNameKind": "§10.2",
    "OverloadedOperator": "§10.2",
    "TagKeyword": "§4.3",
    "AutoKeyword": "§4.3",
    "ExcSpec": "§4.5",
    "NoexceptValue": "§4.5",
}


def t(root, short, row, a, b, expect, **opts):
    TESTS.append((root, short, row, a, b, expect, opts))


def differ(short, row, a, b, expect, **opts):
    t("differ", short, row, a, b, expect, **opts)


def equal(short, row, a, b, **opts):
    t("equal", short, row, a, b, [], **opts)


# ---------------------------------------------------------------------------
# §5.3 DeclKind: one test per row the table writes (`in`); `ConstevalBlock`
# and `ContractResultName` are WP-038's.

def kind(row, a, b, expect, form, **opts):
    opts.setdefault("note", f"`{row}`: {form}.")
    differ("DeclKind", row, a, b, expect, **opts)


kind("Namespace", "", "namespace n {}", ["added n"], "added")
kind("NamespaceAlias", "", "namespace m = n;", ["added m"], "added",
     pre="namespace n {}")
kind("FileScopeAsm", "", 'asm("nop");', ["added <asm>"], "added")
kind("UsingDirective", "", "using namespace n;", ["added <using_directive n>"],
     "added", pre="namespace n {}")
kind("Using", "", "using n::f;", ["added <using n::f>", "added <shadow n::f()>"],
     "added", pre="namespace n { void f(); }")
kind("UsingEnum", "", "using enum E;", ["added <using_enum E>", "added <shadow E::e>"], "added",
     pre="enum class E { e };", std="c++20")
kind("UsingShadow", "namespace n { void f(int); }",
     "namespace n { void f(int); void f(long); }",
     ["added n::f(long)", "added <shadow n::f(long)>"],
     "through the using-declaration: a second shadow of `using n::f`", post="using n::f;")
kind("ConstructorUsingShadow", "struct P { P(int); };", "struct P { P(int); P(long); };",
     ["changed P", "added P::P(long)", "changed D", "added D::<shadow P::P(long)>"],
     "through the inheriting using-declaration: a second constructor shadow",
     post="struct D : P { using P::P; };")
kind("UnresolvedUsingValue", "template <class T> struct D : T { };",
     "template <class T> struct D : T { using T::f; };",
     ["changed D", "added D::<using T::f>"], "added")
kind("UnresolvedUsingTypename", "template <class T> struct D : T { };",
     "template <class T> struct D : T { using typename T::type; };",
     ["changed D", "added D::<using T::type>"], "added")
kind("UnresolvedUsingIfExists", "", "using n::g __attribute__((using_if_exists));",
     ["added <using n::g>", "added <shadow g>", "added g"],
     "added: a using-declaration whose target does not exist", pre="namespace n {}")
kind("Typedef", "", "typedef int T;", ["added T"], "added")
kind("Enum", "", "enum E {};", ["added E"], "added")
kind("EnumConstant", "enum E { a };", "enum E { a, b };", ["changed E", "added b"], "added")
kind("CXXRecord", "", "struct S {};", ["added S"], "added")
kind("ClassTemplateSpecialization", "", "template <> struct S<int> {};",
     ["added S<int>"], "added", pre="template <class> struct S {};")
kind("ClassTemplatePartialSpecialization", "", "template <class T> struct S<T *> {};",
     ["added S<type-parameter-0-0 *>"], "added",
     pre="template <class> struct S {};")
kind("Field", "struct S { int a; };", "struct S { int a; int b; };", ["changed S", "added S::b"],
     "added")
kind("IndirectField", "struct S { int a; };", "struct S { union { int a; }; };",
     ["changed S", "changed S::a", "added S::(anonymous union for a)",
      "added S::(anonymous union for a)::a"],
     "kind changed: a field against an indirect field of one key")
kind("Friend", "struct S { };", "struct S { friend void f(); };",
     ["changed S", "added S::<friend f()>", "added f()"], "added")
kind("Function", "", "void f();", ["added f()"], "added")
kind("CXXMethod", "struct S { };", "struct S { void f(); };", ["changed S", "added S::f()"],
     "added (a member function against a function cannot have one key)")
kind("CXXConstructor", "struct S { };", "struct S { S(int); };", ["changed S", "added S::S(int)"],
     "added")
kind("CXXDestructor", "struct S { };", "struct S { ~S(); };", ["changed S", "added S::~S()"],
     "added")
kind("CXXConversion", "struct S { };", "struct S { operator int(); };",
     ["changed S", "added S::operator int()"], "added")
kind("CXXDeductionGuide", "", "S(char) -> S<int>;",
     ["added <deduction guide for S>(char)"], "added",
     pre="template <class T> struct S { S(T); };", std="c++17")
kind("Var", "", "int v;", ["changed <order>", "added v"], "added")
kind("ParmVar", "void f(int);", "void f(int = 1);", ["changed f(int)"],
     "through the function: a parameter with a default argument")
kind("Binding", "", "auto [a, b] = p;",
     ["changed <order>", "added [a, b]", "added a", "added b"], "added",
     pre="struct P { int x, y; } p;", std="c++17")
kind("VarTemplateSpecialization", "", "template <> int v<int> = 1;",
     ["changed <order>", "added v<int>"], "added",
     pre="template <class> int v = 0;", std="c++17")
kind("VarTemplatePartialSpecialization", "", "template <class T> int v<T *> = 1;",
     ["added v<type-parameter-0-0 *>"], "added",
     pre="template <class> int v = 0;", std="c++17")
kind("ClassTemplate", "", "template <class> struct S;", ["added S"], "added")
kind("FunctionTemplate", "", "template <class T> void g(T);",
     ["added template <class> void g(type-parameter-0-0)"], "added")
kind("VarTemplate", "", "template <class T> T w;", ["added w"], "added",
     std="c++17")
kind("TypeAliasTemplate", "", "template <class T> using A = T *;", ["added A"],
     "added")
kind("Concept", "", "template <class T> concept C = true;", ["added C"],
     "added", std="c++20")
kind("BuiltinTemplate", "", "using X = __make_integer_seq<Seq, int, 2>;",
     ["added X"], "added: a use of the builtin template",
     pre="template <class T, T...> struct Seq {};")
kind("TemplateTypeParm", "template <int> struct S;", "template <class> struct S;",
     ["removed S", "added S"],
     "kind changed: a type parameter against a constant one, in one template (the head is in "
     "the key, §3.3)")
kind("NonTypeTemplateParm", "template <template <class> class> struct S;",
     "template <int> struct S;", ["removed S", "added S"],
     "kind changed: a constant parameter against a template one")
kind("TemplateTemplateParm", "template <class> struct S;",
     "template <template <class> class> struct S;", ["removed S", "added S"],
     "kind changed: a template parameter against a type one")
kind("LifetimeExtendedTemporary", "const int &r = 1;", "const int &r = 2;", ["changed r"],
     "through the reference it extends: the temporary's initialiser")
kind("TemplateParamObject", "template <> struct N<V{1, 2}> {};",
     "template <> struct N<V{1, 3}> {};",
     ["removed N<V{1, 2}>", "added N<V{1, 3}>"],
     "two specialisations on class-type constants that differ in one member (WP-039 item 3)",
     pre="struct V { int a, b; };\ntemplate <V> struct N {};", std="c++20")
kind("StaticAssert", "", "static_assert(true);", ["added <static_assert>"],
     "added", std="c++17")
kind("Label", "void f() { }", "void f() { l:; }", ["changed f()"],
     "through the function that holds it")
equal("DeclKind", "Empty", "int v;", "int v;;", note="`Empty`: declares nothing.")
equal("DeclKind", "AccessSpec", "struct S { int a; };", "struct S { public: int a; };",
      note="`AccessSpec`: access is written by each member.")

# ---------------------------------------------------------------------------
# §5.3 DeclNode: the `data` rows left, and `flags`.

def data(row, a, b, expect, note, **opts):
    differ("DeclNode", f"{row}.data", a, b, expect, note=f"`{row}.data`: {note}.", **opts)


data("Function", "void f();", "inline void f();", ["changed f()"], "`inline`")
data("CXXMethod", "struct S { void f(); };", "struct S { virtual void f(); };",
     ["changed S", "changed S::f()"], "`virtual`")
data("CXXConstructor", "struct S { S(int); };", "struct S { explicit S(int); };",
     ["changed S::S(int)"], "`explicit`")
data("CXXDestructor", "struct S { ~S(); };", "struct S { virtual ~S(); };",
     ["changed S", "changed S::~S()"], "`virtual`")
data("CXXConversion", "struct S { operator int(); };", "struct S { explicit operator int(); };",
     ["changed S::operator int()"], "`explicit`")
data("CXXDeductionGuide", "S(char) -> S<int>;", "explicit S(char) -> S<int>;",
     ["changed <deduction guide for S>(char)"], "`explicit`",
     pre="template <class T> struct S { S(T); };", std="c++17")
data("ClassTemplate", "template <class T> struct S;", "template <class T = int> struct S;",
     ["changed S"], "a default template argument")
data("FunctionTemplate", "template <class T> void g(T);", "template <class T = int> void g(T);",
     ["changed template <class> void g(type-parameter-0-0)"], "a default template argument")
data("VarTemplate", "template <class T> T w;", "template <class T = int> T w;", ["changed w"],
     "a default template argument", std="c++17")
data("TypeAliasTemplate", "template <class T> using A = T;",
     "template <class T = int> using A = T;", ["changed A"], "a default template argument")
data("Concept", "template <class T> concept C = true;",
     "template <class T, class U> concept C = true;", ["removed C", "added C"],
     "the template head (in the key, §3.3)",
     std="c++20")
data("ClassTemplateSpecialization", "template <> struct S<int> {};",
     "template <> struct S<int> { int a; };", ["changed S<int>", "added S<int>::a"],
     "the class's members", pre="template <class> struct S {};")
data("ClassTemplatePartialSpecialization", "template <class T> struct S<T *> {};",
     "template <class T> struct S<T *> { int a; };",
     ["changed S<type-parameter-0-0 *>", "added S<type-parameter-0-0 *>::a"],
     "the class's members", pre="template <class> struct S {};")
data("VarTemplateSpecialization", "template <> int v<int> = 1;", "template <> int v<int> = 2;",
     ["changed v<int>"], "the initialiser", pre="template <class> int v = 0;", std="c++17")
data("VarTemplatePartialSpecialization", "template <class T> int v<T *> = 1;",
     "template <class T> int v<T *> = 2;", ["changed v<type-parameter-0-0 *>"],
     "the initialiser", pre="template <class> int v = 0;", std="c++17")
data("Var", "int v = 0;", "inline int v = 0;", ["changed v"], "`inline`", std="c++17")
data("ParmVar", "void f(int = 1);", "void f(int = 2);", ["changed f(int)"],
     "the default argument")
data("TemplateTypeParm", "template <class T> struct S;", "template <class... T> struct S;",
     ["removed S", "added S"], "a pack (the head is in the key, §3.3)")
data("NonTypeTemplateParm", "template <int N> struct S;", "template <int... N> struct S;",
     ["removed S", "added S"], "a pack (the head is in the key, §3.3)")
data("TemplateTemplateParm", "template <template <class> class T> struct S;",
     "template <template <class> class... T> struct S;", ["removed S", "added S"],
     "a pack (the head is in the key, §3.3)")
data("Namespace", "namespace n {}", "inline namespace n {}", ["changed n"], "`inline`")
data("IndirectField", "struct S { union { int a; }; };",
     "struct S { union { union { int a; }; }; };",
     ["changed S::a", "changed S::(anonymous union for a)",
      "changed S::(anonymous union for a)::a",
      "added S::(anonymous union for a)::(anonymous union for a)",
      "added S::(anonymous union for a)::(anonymous union for a)::a"],
     "the chain of fields")
data("Friend", "struct S { friend void f(); };", "struct S { friend void g(); };",
     ["changed S", "removed S::<friend f()>", "added S::<friend g()>", "removed f()", "added g()"],
     "the entity befriended")
data("ConstructorUsingShadow", "struct P { P(int); };", "struct P { P(long); };",
     ["changed P", "removed P::P(int)", "added P::P(long)", "changed D",
      "removed D::<shadow P::P(int)>", "added D::<shadow P::P(long)>"],
     "the target constructor", post="struct D : P { using P::P; };")
data("UnresolvedUsingValue", "using T::f;", "using U::f;",
     ["changed D", "removed D::<using T::f>", "added D::<using U::f>"], "the qualifier",
     pre="template <class T, class U> struct D : T, U {", post="};")
data("UnresolvedUsingTypename", "using typename T::type;", "using typename U::type;",
     ["changed D", "removed D::<using T::type>", "added D::<using U::type>"], "the qualifier",
     pre="template <class T, class U> struct D : T, U {", post="};")
data("TypeAlias", "using X = int;", "using X = long;", ["changed X"], "the type")
data("StaticAssert", 'static_assert(true, "a");', 'static_assert(true, "b");',
     ["removed <static_assert>", "added <static_assert>"],
     "the message (content-keyed: the key moves)")
data("LifetimeExtendedTemporary", "P r{1, 1, 1};", "P r{1, 2, 3};", ["changed r"],
     "the record is written (the temporaries' mangling indices)",
     pre="struct P { const int &a; const int &b; const int &c; };", needs="expr",
     why="the aggregate initialiser is Pending(Expr), as in LetData/mangling_index")
differ("DeclNode", "flags",
       "", "consteval { std::meta::define_aggregate(^^S, {std::meta::data_member_spec(^^int, {.name = \"e\"})}); }",
       ["changed S", "added S::e", "added <consteval_block>"],
       note="`flags`: the record is written (`0-injected`: a member `define_aggregate` made, "
            "against none).",
       pre='#include "meta.h"\nstruct S;', std="c++26")

# ---------------------------------------------------------------------------
# §5.3 DeclFlags, DeclMisc.

differ("DeclFlags", "0-injected",
       "struct S { int e; };",
       "struct S;\nconsteval { std::meta::define_aggregate(^^S, {std::meta::data_member_spec(^^int, {.name = \"e\"})}); }",
       ["changed S::e", "added <consteval_block>"],
       note="`0-injected`: a member `define_aggregate` made against one written.",
       pre='#include "meta.h"', std="c++26")
differ("DeclFlags", "0-implicit", "struct S { };", "struct S { S() = default; };",
       ["changed S", "added S::S()"],
       note="`0-implicit`: a defaulted default constructor written against the implicit one.",
       post="S s;")
differ("DeclMisc", "0-name-independent", "int _;", "int _; int _;",
       ["changed S", "added S::_ (name-independent 1)"],
       note="`0-name-independent`: a second member named `_` (P2169); the bit follows from the "
            "name and the scope, so no pair flips it alone.",
       pre="struct S {", post="};", std="c++26")
# An exported declaration against one that is not: since Kiln WP-2477 the
# owner is the module table's alone (`differ/Owner/exported`).

# ---------------------------------------------------------------------------
# §10.2 DeclNameKind and DeclName: a name in a key (digest-kinds §3.2: the
# first seven tags of `Name` are `DeclNameKind`'s). A pair whose two
# names differ in the kind alone (same payload) fails when the kind is
# written as a constant: the two keys meet and the unit is `changed`.

def name(short, row, a, b, expect, note, **opts):
    differ(short, row, a, b, expect, note=f"`{row}`: {note}.", **opts)


name("DeclNameKind", "Identifier", "void _x(unsigned long long);",
     "void operator\"\"_x(unsigned long long);",
     ["removed _x(unsigned long long)", "added operator\"\"_x(unsigned long long)"],
     "an identifier against a literal operator of one suffix", std="c++17")
name("DeclNameKind", "LiteralOperator", "void operator\"\"_x(unsigned long long);",
     "void _x(unsigned long long);",
     ["removed operator\"\"_x(unsigned long long)", "added _x(unsigned long long)"],
     "a literal operator against an identifier of one spelling", std="c++17")
name("DeclNameKind", "Constructor", "struct S { ~S(); };", "struct S { S(); };",
     ["changed S", "removed S::~S()", "added S::S()"],
     "a constructor against a destructor: neither name has a payload")
name("DeclNameKind", "Destructor", "struct S { S(); };", "struct S { ~S(); };",
     ["changed S", "removed S::S()", "added S::~S()"],
     "a destructor against a constructor")
name("DeclNameKind", "Operator", "struct S { void f(); };", "struct S { void operator()(); };",
     ["changed S", "removed S::f()", "added S::operator()()"],
     "an operator against an identifier")
name("DeclNameKind", "Conversion", "struct S { int f(); };", "struct S { operator int(); };",
     ["changed S", "removed S::f()", "added S::operator int()"],
     "a conversion function against an identifier")
name("DeclNameKind", "DeductionGuide", "", "S(char) -> S<int>;",
     ["added <deduction guide for S>(char)"],
     "added", pre="template <class T> struct S { S(T); };", std="c++17")

name("DeclName", "0-kind", "struct S { ~S(); };", "struct S { S(); };",
     ["changed S", "removed S::~S()", "added S::S()"], "the kind alone differs")
name("DeclName", "Identifier.0-payload", "void f();", "void g();",
     ["removed f()", "added g()"], "the spelling")
name("DeclName", "Operator.0-payload", "struct S { S operator+(S); };",
     "struct S { S operator-(S); };",
     ["changed S", "removed S::operator+(S)", "added S::operator-(S)"], "the operator")
name("DeclName", "Conversion.0-payload", "struct S { operator int(); };",
     "struct S { operator long(); };",
     ["changed S", "removed S::operator int()", "added S::operator long()"], "the type")
name("DeclName", "LiteralOperator.0-payload", "void operator\"\"_a(unsigned long long);",
     "void operator\"\"_b(unsigned long long);",
     ["removed operator\"\"_a(unsigned long long)", "added operator\"\"_b(unsigned long long)"],
     "the suffix", std="c++17")
name("DeclName", "Constructor.0-payload", "struct S { S(); };", "struct T { T(); };",
     ["removed S", "removed S::S()", "added T", "added T::T()"],
     "the class: a key has no payload (§3.2), the parent key names the class")
name("DeclName", "Destructor.0-payload", "struct S { ~S(); };", "struct T { ~T(); };",
     ["removed S", "removed S::~S()", "added T", "added T::~T()"],
     "the class, through the parent key")
name("DeclName", "DeductionGuide.0-payload",
     "template <class T> struct S { S(T); }; S(char) -> S<int>;",
     "template <class T> struct U { U(T); }; U(char) -> U<int>;",
     ["removed S", "removed S::S<T>(type-parameter-0-0)", "removed <deduction guide for S>(char)",
      "added U", "added U::U<T>(type-parameter-0-0)", "added <deduction guide for U>(char)"],
     "the template, through the discriminator", std="c++17")

# ---------------------------------------------------------------------------
# §10.2 OverloadedOperator: as the name of an operator function. Each
# operator against a partner of the same signature, so that a tag written
# as a constant makes the two keys meet (`changed` for `removed`/`added`).

UNARY = ["Tilde", "Exclaim", "Arrow", "PlusPlus", "MinusMinus", "Coawait"]
BINARY = ["Plus", "Minus", "Star", "Slash", "Percent", "Caret", "Amp", "Pipe", "Equal", "Less",
          "Greater", "PlusEqual", "MinusEqual", "StarEqual", "SlashEqual", "PercentEqual",
          "CaretEqual", "AmpEqual", "PipeEqual", "LessLess", "GreaterGreater", "LessLessEqual",
          "GreaterGreaterEqual", "EqualEqual", "ExclaimEqual", "LessEqual", "GreaterEqual",
          "Spaceship", "AmpAmp", "PipePipe", "Comma", "ArrowStar", "Call", "Subscript"]
ALLOC = ["New", "ArrayNew"]
DEALLOC = ["Delete", "ArrayDelete"]
SPELL = {
    "New": " new", "Delete": " delete", "ArrayNew": " new[]", "ArrayDelete": " delete[]",
    "Plus": "+", "Minus": "-", "Star": "*", "Slash": "/", "Percent": "%", "Caret": "^",
    "Amp": "&", "Pipe": "|", "Tilde": "~", "Exclaim": "!", "Equal": "=", "Less": "<",
    "Greater": ">", "PlusEqual": "+=", "MinusEqual": "-=", "StarEqual": "*=",
    "SlashEqual": "/=", "PercentEqual": "%=", "CaretEqual": "^=", "AmpEqual": "&=",
    "PipeEqual": "|=", "LessLess": "<<", "GreaterGreater": ">>", "LessLessEqual": "<<=",
    "GreaterGreaterEqual": ">>=", "EqualEqual": "==", "ExclaimEqual": "!=", "LessEqual": "<=",
    "GreaterEqual": ">=", "Spaceship": "<=>", "AmpAmp": "&&", "PipePipe": "||",
    "PlusPlus": "++", "MinusMinus": "--", "Comma": ",", "ArrowStar": "->*", "Arrow": "->",
    "Call": "()", "Subscript": "[]", "Coawait": " co_await",
}


def op_decl(op):
    s = SPELL[op]
    if op in UNARY:
        return f"S operator{s}();", f"S::operator{s}()"
    if op in ALLOC:
        return f"static void *operator{s}(Z);", f"S::operator{s}(unsigned long)"
    if op in DEALLOC:
        return f"static void operator{s}(void *);", f"S::operator{s}(void *)"
    return f"S operator{s}(int);", f"S::operator{s}(int)"


for group in (UNARY, BINARY, ALLOC, DEALLOC):
    for i, op in enumerate(group):
        partner = group[(i + 1) % len(group)]
        (a, na), (b, nb) = op_decl(op), op_decl(partner)
        differ("OverloadedOperator", op, f"struct S {{ {a} }};", f"struct S {{ {b} }};",
               ["changed S", f"removed {na}", f"added {nb}"],
               note=f"`{op}`: `operator{SPELL[op].strip()}` against "
                    f"`operator{SPELL[partner].strip()}` of one signature.",
               pre="using Z = decltype(sizeof 0);", std="c++20")

# ---------------------------------------------------------------------------
# Rows other areas left.

# `TagKeyword` is written by a dependent name only (types.rs
# `DependentName`): each keyword against the next, in one alias.
KEYWORDS = [("Typename", "typename "), ("Struct", "struct "), ("Union", "union "),
            ("Enum", "enum ")]
for i, (row, kw) in enumerate(KEYWORDS):
    prow, pkw = KEYWORDS[(i + 1) % len(KEYWORDS)]
    differ("TagKeyword", row, f"using X = {kw}T::x;", f"using X = {pkw}T::x;", ["changed A::X"],
           note=f"`{row}`: `{kw}T::x` against `{pkw}T::x`.",
           pre="template <class T> struct A {", post="};", std="c++20")
equal("_", "tag-keyword-none-is-typename", "using X = typename T::x;", "using X = T::x;",
      note="TagKeyword `None` is canonical as `Typename` (kiln_ast/src/types/canon.rs:899 "
           "`canonical_keyword`), so no canonical type holds it.",
      pre="template <class T> struct A {", post="};", std="c++20")
differ("AutoKeyword", "Auto", "decltype(auto) f();", "auto f();",
       ["changed f()"], note="`Auto`: `auto` against `decltype(auto)`, undeduced.", std="c++17")
differ("AutoKeyword", "DecltypeAuto", "auto f();", "decltype(auto) f();",
       ["changed f()"], note="`DecltypeAuto`.", std="c++17")
differ("AutoKeyword", "GnuAutoType", "template <class T> auto v = T();",
       "template <class T> __auto_type v = T();", ["changed v"],
       note="`GnuAutoType`: `__auto_type` against `auto`, undeduced in a template.", std="c++17")
# `Dynamic` is reachable only before C++17, which the snippet runner has
# no mode for (crates/slip_kiln/src/snippet.rs:435); from C++17 Kiln's
# type holds the canonical `None` (kiln_ast/src/types/canon.rs:657).
equal("_", "exc-dynamic-is-none-in-cxx17", "void f() noexcept(false);", "void f() throw(int);",
      note="`ExcSpec` `Dynamic` in C++17: `throw(int)` is canonical `None` "
           "(kiln_ast/src/types/canon.rs:657).",
      std="c++17", args="-Wno-dynamic-exception-spec")
t("demand", "ExcSpec", "Deferred", "", "bool g() { return noexcept(S()); }", ["added g()"],
  note="`Deferred`, §11.2: a use that resolves the deferred specification of a defaulted "
       "constructor changes no unit.",
  pre="struct S { S() = default; };")
t("demand", "ExcSpec", "Deferred.0", "", "bool g() { return noexcept(S(s)); }", ["added g()"],
  note="`Deferred.0`, §11.2: the function whose specification is deferred, resolved by a use.",
  pre="struct S { S(); S(const S &) = default; };\nextern S s;")
differ("ExcSpec", "DependentNoexcept", "template <class T> void f() noexcept;",
       "template <class T> void f() noexcept(sizeof(T) > 1);",
       ["changed template <class> void f()"],
       note="`DependentNoexcept`: a dependent operand against `noexcept`.")
differ("ExcSpec", "DependentNoexcept.0", "template <class T> void f() noexcept(sizeof(T) > 1);",
       "template <class T> void f() noexcept(sizeof(T) > 2);",
       ["changed template <class> void f()"], note="`DependentNoexcept.0`: the operand.",
       needs="expr", why="the operand's profile is Pending(Expr)")
differ("NoexceptValue", "Dependent.0", "template <class T> void f() noexcept(sizeof(T) > 1);",
       "template <class T> void f() noexcept(sizeof(T) > 2);",
       ["changed template <class> void f()"], note="`Dependent.0`: the operand.",
       needs="expr", why="the operand's profile is Pending(Expr)")


# ---------------------------------------------------------------------------
# digest §2.2, WP-039 fix round item 1: an explicit specialisation of a
# member of an implicitly instantiated class is written in source, so it
# is a declared unit (`added`, not `derived added`), whatever the member.

SPEC_PRE = "template <class T> struct A {"
for row, member, spec, expect in [
    ("member-spec-class-template", "template <class U> struct C;",
     "template <> template <class U> struct A<long>::C { void m(); };", ["added A<long>::C", "added A<long>::C::m()"]),
    ("member-spec-variable-template", "template <class U> static int v;",
     "template <> template <class U> int A<long>::v = 1;", ["added A<long>::v"]),
    ("member-spec-enumeration", "enum E : int;",
     "template <> enum A<long>::E : int { x };", ["added A<long>::E", "added A<long>::x"]),
    ("member-spec-static-data-member", "static int s;",
     "template <> int A<long>::s = 1;", ["changed <order>", "added A<long>::s"]),
]:
    differ("_", row, "", spec, expect,
           note=f"§2.2: `{spec}` is written in source, so a declared unit.",
           pre=f"{SPEC_PRE} {member} }};")


# digest §3.3, WP-039 fix round item 2: a name-independent declarator
# names no unnamed class, so the two classes are named by their first
# members, and swapping their bodies changes the members `_` only.
differ("_", "unnamed-class-of-name-independent-swap",
       "struct { int a; } _; struct { int b; } _;",
       "struct { int b; } _; struct { int a; } _;",
       ["changed S::_ (name-independent 0)", "changed S::_ (name-independent 1)"],
       note="§3.3: two classes declared by name-independent members `_`, their bodies swapped.",
       pre="struct S {", post="};\nint use = sizeof(S);", std="c++26")
differ("_", "unnamed-class-of-name-independent-members",
       "struct { int _; } _; struct { long _; } _;",
       "struct { long _; } _; struct { int _; } _;",
       ["changed S::(anonymous struct 0)::_ (name-independent 0)",
        "changed S::(anonymous struct 1)::_ (name-independent 0)"],
       note="§3.3: a name-independent member names no unnamed class either; the ordinal does.",
       pre="struct S {", post="};", std="c++26")

# WP-039 fix round item 3: the name-independent ordinal of a member of a
# class template is counted over the pattern's members.
differ("_", "name-independent-in-class-template", "T _; int _;", "T _; long _;",
       ["changed S::_ (name-independent 1)"],
       note="§3.3 `field`: two `_` members of a class template, distinct and determinate.",
       pre="template <class T> struct S {", post="};", std="c++26")
differ("_", "name-independent-in-instantiation", "T _; int _;", "T _; long _;",
       ["changed S::_ (name-independent 1)", "derived-changed S<char>::_ (name-independent 1)"],
       note="§3.3 `field`: the same members in an instantiation.",
       pre="template <class T> struct S {", post="};\nS<char> s;", std="c++26")

# Item 4: an explicit object parameter is in the display name, and both
# functions are in one translation unit so that the sweep sees the key.
differ("_", "explicit-object-and-first-parameter", "void f(S&);", "void f(this S&); void f(S&);",
       ["changed S", "added S::f(this S &)"], note="§3.3 `function`: `f(this S&)` against `f(S&)`.",
       pre="struct S {", post="};", std="c++26")
differ("_", "explicit-object-and-first-parameter-templates", "template <class T> void f(T&&);",
       "template <class T> void f(this T&&); template <class T> void f(T&&);",
       ["changed S", "added template <class> void S::f(this type-parameter-0-0 &&)"], note="§3.3 `function`: `f(this T&&)` against `f(T&&)`, templates.",
       pre="struct S {", post="};", std="c++26")

# Item 5d: two conversion functions of one class in one translation unit
# (the conversion's type is the name's payload, K12).
differ("_", "two-conversion-functions", "operator int();", "operator int(); operator long();",
       ["changed S", "added S::operator long()"], note="§3.2: two conversion functions of one class.",
       pre="struct S {", post="};")
differ("_", "two-conversion-function-templates",
       "template <class T> operator T*(); template <class T> operator T();",
       "template <class T> operator T*(); template <class T> operator T(); "
       "template <class T> operator const T();",
       ["changed S", "added template <class> S::operator const type-parameter-0-0()"], note="§3.2: three conversion function templates of one class.",
       pre="struct S {", post="};")

def write():
    names = set()
    for root, short, row, a, b, expect, opts in TESTS:
        key = (root, short, row)
        if key in names:
            sys.exit(f"duplicate {key}")
        names.add(key)
        d = os.path.join(ROOT, root, short, row)
        os.makedirs(d, exist_ok=True)
        # A module declaration must open the unit (after the header lines),
        # so a module test takes no side header, as in `differ/Owner`.
        module = opts.get("pre", "").startswith("export module")
        lines = [] if module else ['#include "side.h"']
        if "args" in opts:
            lines.insert(0, f"// args: {opts['args']}")
        if "std" in opts:
            lines.insert(0, f"// std: {opts['std']}")
        if opts.get("errors"):
            lines.insert(0, "// errors: yes")
        lines.append(f"// {SECTION.get(short, 'digest-kinds')} {short if short != '_' else 'rule'}"
                     f" {opts['note']}")
        if opts.get("pre"):
            lines.append(opts["pre"])
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
            if opts.get("why"):
                out.append(f"# {opts['why']}")
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
