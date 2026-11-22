# KQ1 amendment 1 — what the audit of `odr_def` found on Kiln's side, and one more field

A reply from Slip to Kiln. Status: **sent** (2026-11-22). It amends
[KQ1](KQ1-digest-fields.md) §E (§1 to §6) and adds one request for a
field of `Ast` (§7) and two small reports (§8).

Read: `kiln_abi::odr_def` (eleven files) and the parts of
`kiln_abi/src/odr.rs` that feed it, at Kiln `62d190007` (Slip's pin),
against `kiln/spec/abi.md` "Definition hash (`odr_def`)". Kiln main
`83566cc47` was compared on the same paths and has no change to them.
The full audit, one line per read, is in
[spec/landed/digest-kinds/WP-042.md](../../spec/landed/digest-kinds/WP-042.md).

Each item below is a pair of definitions:

- they differ in meaning;
- `odr_def` hashes them alike, as far as reading the code shows;
- none is in the Misses table (`abi.md:2554-2573`) as it is worded.

The sources were not compiled; they show the shape. Each is one inline
function or variable, defined in two units, with the macro `TU2` defined
in one of them.

## Design or implementation

| Part | Kind |
|---|---|
| §1 Local classes | **Implementation.** Three more facts in `local_class`. |
| §2 Coroutine lambdas | **Implementation.** Hash the call operator's body instead of `LambdaData` `body`. |
| §3 Structured bindings | **Implementation**, with a small **design** question: how to hash an implicit expression the walk does not reach today. |
| §4 `cleanup` on a local | **Implementation**, or a decision to add a Misses row. |
| §5 `musttail` | **Decision**: a Misses row is enough. |
| §6 Concatenated literals | **Decision**: reword Miss #9, or narrow the placeholder. |
| §7 `offsetof`: operand type and designator | **Implementation.** A cold row filled where the builder already has both; the form of the designator is `OffsetofComponent`'s. |
| §8 Two reports | **Implementation** (an error bit; a diagnostic). Low. |

Nothing here is design without prior art. §1 to §6 are about Kiln's
own ODR hash (Slip does not depend on them: its digest already tells
each pair apart); §7 is the one thing Slip waits on.

## 1. Local classes: `union` against `struct`, and attributes

`local_class` (`kiln/crates/kiln_abi/src/odr_def/lambda.rs:386-412`)
writes the kind, the bases and the members. It does not write:

- `RecordData` `tag` (`kiln_ast` `decl.rs:1336`): `DeclKind` is
  `CXXRecord` for both;
- the class's attributes or its fields' (`local_class_member`,
  lambda.rs:414-436, calls no `listed_attrs`);
- a local enumeration's underlying type.

Miss #13 sends `alignas`/`packed` to kind 3, but a local class has no
kind-3 row: `NotTuLocal::scopes` fails inside a function, `odr.rs:154`.

```cpp
inline int f() {
#ifdef TU2
  union U { int a; double b; };
#else
  struct U { int a; double b; };
#endif
  return sizeof(U);   // 8 against 16; `sizeof` hashes the type item, not the value
}

inline int g() {
#ifdef TU2
  struct __attribute__((packed)) S { char c; int i; };
#else
  struct S { char c; int i; };
#endif
  return sizeof(S);   // 5 against 8
}
```

Asked: write `RecordData` `tag` after the kind (`Class` may stay equal
to `Struct`). Run `listed_attrs` for the local class and for each field.
For a local enumeration, write its underlying type.

## 2. Coroutine lambdas: the coroutine body is not hashed

`closure_digest` hashes `LambdaData` `body` (lambda.rs:109).

- kiln_sema `lambda/mod.rs:1129` stores the compound statement there
  before it is wrapped.
- `end_function_body` wraps it into the `CoroutineBodyStmt`
  (`coroutine.rs:1139`), and `set_body` (`lambda/mod.rs:1082`) gives only
  the call operator that wrapped body.
- So for a coroutine lambda, `misc.rs:177-189` is never reached: the
  promise, the initial and final suspends, the implicit `co_return` and
  the allocation are not hashed.
- An ordinary coroutine is hashed in full through `FunctionData` `body`
  (mod.rs:471).

```cpp
#include <coroutine>
struct start {};
struct task { struct promise_type {
  task get_return_object() { return {}; }
  start initial_suspend() { return {}; }
  std::suspend_never final_suspend() noexcept { return {}; }
  void return_void() {} void unhandled_exception() {} }; };

inline std::suspend_never operator co_await(const start&) { return {}; }
#ifdef TU2
inline std::suspend_always operator co_await(start&&) { return {}; }  // better match
#endif

inline auto g = []() -> task { co_return; };  // starts suspended in TU2 only
```

Asked: hash the call operator's `FunctionData` `body` in
`closure_digest`, and keep `LambdaData` `body` for what else reads it.

## 3. Structured bindings: the `get` a binding calls

`local_decl` declares each binding (decl.rs:240-246) but does not read
`ColdDeclData` `binding_expr` or `binding_var`. For a tuple-like type,
the `get<i>` that initialises each binding is therefore not hashed.

```cpp
#include <utility>
namespace n { struct P { int a, b; };
  template<std::size_t I> const int& get(const P& p) { return I ? p.b : p.a; } }
template<> struct std::tuple_size<n::P> : std::integral_constant<std::size_t, 2> {};
template<std::size_t I> struct std::tuple_element<I, n::P> { using type = const int; };
#ifdef TU2
namespace n { template<std::size_t I> const int& get(P& p) { return p.b; } }  // better match
#endif

inline int f(n::P& p) { auto& [x, y] = p; return x; }   // p.a against p.b
```

Asked: for each binding with a `binding_var`, hash that variable's
initialiser, which holds the `get` call. A binding of the other two
forms (array, data member) follows from the type and needs nothing.

## 4. `cleanup` on a local variable

A local hashes only the listed attributes (decl.rs:238; the list is
`attrs.rs:42-49`). The function that `__attribute__((cleanup(f)))` calls
at scope exit is not hashed.

```cpp
inline void release_a(int*) {}
inline void release_b(int*) {}
inline void f() {
#ifdef TU2
  int x __attribute__((cleanup(release_b))) = 0;
#else
  int x __attribute__((cleanup(release_a))) = 0;
#endif
}
```

Asked: hash `CLEANUP` on a local by its function (by entity, as a
callee), or add the case to the Misses table.

Slip's digest marks `CLEANUP.a` unsupported today, because the node does
not say whether its word is a `DeclId` or an `ExprId`
(`spec/digest-kinds.md` §12). A word that says which would close it on
both sides.

## 5. `[[clang::musttail]]`

Only `ASSUME` is hashed among statement attributes (attrs.rs:178-195,
stmt.rs:147-150), so `[[clang::musttail]] return g(x);` and
`return g(x);` hash alike. The values are the same and the stack
behaviour is not. This is low: a Misses row is enough.

## 6. Every concatenated string literal is a placeholder

`StringLiteral` becomes the whole-literal placeholder whenever its end
location differs from its start (expr.rs:213). That is any
concatenation, not only one with a builtin-macro piece.

```cpp
inline const char* f() {
#ifdef TU2
  return "ab" "d";
#else
  return "ab" "c";
#endif
}
```

Miss #9 (`abi.md:2565`) describes the `__FILE__` case only. Either reword
it to "any literal concatenated from pieces", or keep the bytes when no
piece is in the scratch space. Kiln keeps only the first and last
pieces' locations, so that second option may need a bit on the node
saying that a piece came from a macro.

## Compared and found equal in substance

- **The declarator.** `FnSpec`, `FuncProto`, `FuncInfo`, the exception
  specification, the trailing requires-clause, the default arguments
  over the chain, the listed attributes.
- **Bodies.** Every statement kind. Every expression kind, with the
  transparent ones left out by design.
- **Object construction.** Initialisers, `new`/`delete`, the traits,
  ctor-initializers, `dtor_delete`.
- **Lambdas.** Captures, generic parameters, the call operator.
- **The signature area.** Dependent types, expressions and
  requires-expressions.

Slip is stricter in a few places (capture defaults, attribute order,
elidable copies). That is by purpose, not a gap of Kiln's.

Two notes, not gaps:

- `type_trait_value` truncates to bool (init.rs:192). The trait's
  argument types are hashed, so nothing is lost.
- `#pragma clang attribute` copies its nodes without `IMPLICIT`
  (kiln_sema `pragma_attr.rs:503-518`), so they are hashed as written.
  The comment at `attrs.rs:50-51` ("added by sema") is accurate.
  Slip's own table said otherwise and will be corrected.

## 7. `offsetof`: the class and the member path are not in the node

Not a gap of the ODR hash (the value is hashed, and two definitions
with one value generate one code), but a fact Slip's digest needs and
`Ast` drops. `Builder::offsetof` (`kiln_ast` builder.rs:3525-3550)
keeps the index expressions, the folded offset and `invalid_record`;
`ColdExprData` keeps layout steps for an offset that is not constant.
The operand type and the designator (which field, through which base)
are in the node only while the expression is dependent (the pattern
components).

```cpp
struct A  { int x, y; };
struct Bq { int p, q; };
unsigned long a = __builtin_offsetof(A, y);    // the same node as
unsigned long b = __builtin_offsetof(Bq, q);   // this one
```

So a transformation after which `offsetof(T, m)` names another class
of the same layout (the root-namespace migration is exactly a change of
which `T` a name finds), another member at the same offset (two members
of a union), or the same member through another base, is invisible to
the digest: Slip's promise is that every name denotes what it did, not
only that the value is the same. Slip records it under "Hazards
accepted for now" and has a pending test
(`differ/_/offsetof-other-class-same-layout`, `# needs: kiln`).

Asked: for a non-dependent `OffsetOfExpr`, keep the operand type (a
`QualType`) and the designator as resolved: per component, the field
(`DeclId`), the base class stepped through for a member found in a
base, or the index position, as `OffsetofComponent` has them for the
dependent form. A cold row is enough; nothing reads it but a consumer
of `Ast`.

## 8. Two small reports, not requests about the hash

- **`va_arg` of a class that is not trivially copyable.** Kiln reports
  the error and builds an ordinary `VAArgExpr` whose dependence has no
  `ERROR` bit, where every other erroneous form of the miscellaneous
  expression kinds becomes a `RecoveryExpr`
  (`struct N { N(const N&); }; void f(__builtin_va_list l) { (void)__builtin_va_arg(l, N); }`).
  A consumer that walks the body cannot see that the expression is
  erroneous. Asked: a `RecoveryExpr`, or the `ERROR` bit.
- **A friend template redeclared with a default template argument is
  not diagnosed** ([temp.param]/12: a default template argument shall
  not be specified in a friend template declaration unless it is a
  definition, and then no other declaration is allowed). When an
  instantiation of a class template declares the friend template with
  the default argument and the template is declared again later, Kiln
  accepts the program. Seen while testing Slip's rule for the
  declaration read (`spec/landed/digest/WP-032.md`); the shape, not
  compiled as written here:
  `template<class T> struct S { template<class U = int> friend void g(S, U) {} }; S<int> s; template<class U> void g(S<int>, U);`.
  Low: recorded so that it is not rediscovered.
