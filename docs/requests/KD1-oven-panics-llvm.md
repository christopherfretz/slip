# KD1 — two panics of the Oven on LLVM 19.1.7

Sent to the Kiln agent 2026-11-22. Found by the first run of
`slip parse` over the `llvm` corpus (`docs/wp/WP-014.md`): 4061
translation units, 4053 `ok`, 2 diagnosed as clang diagnoses them, and 6
whose worker died in a panic of Kiln's constant evaluator at the pinned
revision `4bfd999da`. Slip runs the front end through
`kiln_driver::syntax_only`; both reproduce with `-std=c++17` and no
other option. Each was reduced by hand from the backtrace.

## 1. A constructor expression given to `rv_inner`

`kiln_oven/src/compile.rs:4298`, "`rv_inner` evaluates a scalar prvalue,
not a CXXConstructExpr". Reached from `check_for_int_overflow` on a full
expression (`int_overflow.rs`, `overflow_scan` into `overflow_eval`,
`fold_for_overflow`).

```cpp
struct S { unsigned x = 0; };
S *p;
void f(unsigned long n) { p = new S[n]; }
```

All three are needed: the assignment (a bare `new S[n];` or an
initialiser `S *p = new S[n];` is fine), a bound that is not a constant
(`new S[2]` is fine), and an array (`new S` is fine). Five units of
LLVM: `clang/lib/AST/DeclObjC.cpp`, `DeclTemplate.cpp`, `Stmt.cpp`,
`clang/lib/Sema/DelayedDiagnostic.cpp`,
`clang/lib/Tooling/Inclusions/Stdlib/StandardLibrary.cpp`.

## 2. A prvalue `DeclRefExpr` that names a variable

`kiln_oven/src/compile.rs:4112`, "a prvalue DeclRefExpr names an
enumerator with its value or a function". Reached from
`check_bit_field_assignment` (`bitfield_constant.rs`,
`bit_field_truncation`, `fold_int`) on an expression statement of an
instantiated function body (`Substituter::stmt_node`), through
`Compiler::member`, `member_object` and `rv`.

```cpp
struct T { union { unsigned a : 2; int other; }; };
template <class P> void ensure(T *t, T *ct) { t->a = ct->a; }
void g(T *t) { ensure<int>(t, t); }
```

Needed: the anonymous union (a direct member is fine), the arrow (`ct.a`
through a reference is fine), and the instantiation (the same body in a
function that is not a template is fine). So the substituter seems to
rebuild `ct->a` through the anonymous union with the pointer as a
prvalue `DeclRefExpr`, without its lvalue-to-rvalue conversion; the
bit-field check is only the first thing to evaluate it. One unit of
LLVM: `clang/lib/AST/Type.cpp` (`TypePropertyCache<Private>::ensure`).
