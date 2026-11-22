# pitfalls — the catalogue of semantic hazards

Status: draft (M1). The mutation suite of [digest.md](digest.md) §14 and
[verify.md](verify.md): every way an edit that looks meaning-preserving is
known to change a program, each as a test that a correct edit verifies and
a wrong edit does not. The catalogue grows: a mismatch found on a real
codebase that no row explains becomes a row before it is fixed.

The rows test the trusted base, not a transform. Both edits are written by
hand; nothing here depends on how Slip would have produced them.

## 1. The form of a test

One directory per row, `crates/slip_verify/tests/pitfalls/<id>-<slug>/`:

```
pre/            the subject: sources and headers; main.cpp is the translation unit
good/           the same tree after the correct edit           (optional, see below)
bad/            the same tree after the wrong edit; bad-2/, bad-3/ … for more
declared.txt    the declared changes, shared by good and every bad
expect.txt      what each tree must give
```

- A directory under `pre/` named `dep/` is read-only code: `good/` and
  `bad*/` must contain it unchanged, and the harness checks that.
- Sources are self-contained: no system header. A row that needs a library
  facility declares the few lines of it that it uses.
- Every tree is one observer, `main.cpp` compiled as C++20 unless
  `expect.txt` says `std: c++17` (or another). `-I.` and `-Idep`.
- `declared.txt` is the `Declared` of `verify.md` §2 written with names, one
  line each:

  ```
  relocate <entity> => <new parent> :: <new name>
  changed  <entity>
  created  <entity>
  removed  <entity>
  ```

  An entity is written as the report renders it (`verify.md` §6, `name`):
  a qualified name, with the parameter types for a function. The harness
  resolves each name to a key through the unit names of the image that has
  it (`pre/` for `relocate`, `changed` and `removed`; the tree under test
  for `created`), and builds the rule that undoes each relocation. A name
  that matches no unit or several is an error of the test.
- `expect.txt` has one line per tree:

  ```
  good:  verified
  bad:   mismatch changed <entity>
  bad-2: not_parsed
  ```

  `mismatch` is followed by one finding kind and entity that must be among
  the findings; `mismatch` alone accepts any. A row without `good/` says so
  with `good: none <reason>`.

The harness runs `pre/` through the map phase and each other tree through
the verify job. A `bad` tree that verifies fails the test: that is a hole
in the trusted base and blocks a release. A `good` tree that does not
verify fails it too: a verifier that rejects correct edits is not usable.
Every `good` tree and every `bad` tree is also compiled with the
production compiler where one is installed (tier T2), so that the test's
own C++ is known to be what its author thought; a `bad` tree marked
`not_parsed` must be rejected there too.

Many wrong edits are caught by the parse failing. Those rows are kept: they
document the hazard and pin the verdict. Where a hazard can also change
meaning **silently**, the row has a second `bad` tree arranged so that the
post-image still compiles (typically by adding a worse-matching fallback to
`pre/`), because that is the case only the digest catches.

## 2. Relocating a namespace

Setting for every row unless it says otherwise: `dep/` declares entities in
`namespace legacy` and keeps them there; owned code declares more in
`namespace legacy` and moves them to a new `namespace acme`. `declared.txt`
is `relocate legacy => :: acme` in its merged form (`verify.md` §2.1): the
rule maps `acme` onto `legacy`.

| Id | Hazard | Correct edit | Wrong edit | The wrong edit gives |
|---|---|---|---|---|
| N01 | Moved code names a staying type without qualification (`Dep`, found through the enclosing `legacy`) | `legacy::Dep` | left as `Dep` | `not_parsed`; with a global `::Dep` in `pre/`: `mismatch removed` the function whose signature held `legacy::Dep` |
| N02 | A use outside the namespace spells a moved entity `legacy::x::T` | `acme::x::T` | left | `not_parsed` |
| N03 | An unqualified call finds a **moved** function by argument-dependent lookup through a **staying** argument type | callee qualified `acme::use(d)` | left | `not_parsed`; with a variadic `::use(...)`: `mismatch changed` the caller |
| N04 | An unqualified call finds a **staying** function through a **moved** argument type | `legacy::touch(m)` | left | `not_parsed`; with a fallback: `mismatch changed` the caller |
| N05 | An operator found through the namespace of a moved operand, declared by the dependency (`a == b` using `legacy::operator==`) | `using legacy::operator==;` added in `acme`, declared `created` | left | `not_parsed`; where the operands convert to a built-in type: `mismatch changed` the caller (the built-in operator on converted values) |
| N06 | A dependency template calls a customisation point unqualified (`prepare(t)` inside `legacy::store<T>`); the hook for a moved type is owned | type and hook both move | type moves, hook stays in a `namespace legacy` block | `not_parsed`; with a generic `legacy::prepare` in `dep/`: `mismatch derived_changed legacy::store<legacy::Mine>` **and no declared unit differs** |
| N07 | An explicit specialisation of a dependency template written in an owned `namespace legacy` block | the block is split; the specialisation stays in `legacy` and names `acme::Mine` | moved into `acme` with the rest | `not_parsed` |
| N08 | `using namespace legacy;` in client code that then names a moved entity without qualification | `using namespace acme;` added (the same unit after mapping: nothing to declare) | left | `not_parsed`; with a global of that name: `mismatch changed` the user |
| N09 | A namespace alias (`namespace L = legacy;`) used to name moved and staying entities | a second alias for `acme`, uses of moved entities respelled | alias retargeted to `acme` | `not_parsed` at a use of a staying entity |
| N10 | A using-declaration `using legacy::T;` naming a moved entity | `using acme::T;` | left | `not_parsed` |
| N11 | A friend function or an elaborated type specifier that declares an entity into the nearest enclosing namespace | moves with its class | — | `good` only: the injected entity is relocated like any other |
| N12 | `extern "C"` function declared inside `legacy` | moves with the block; its key does not change | renamed with a prefix | `mismatch removed` the C function |
| N13 | `inline namespace v1` inside `legacy` | `acme { inline namespace v1 {…} }` | `inline` dropped | `mismatch changed legacy::v1` (and uses through `legacy::` no longer parse; second tree qualifies them `acme::v1::` so that only the namespace differs) |
| N14 | The unnamed namespace inside `legacy` | moves with the block | hoisted out of the namespace | `mismatch removed` its members |
| N15 | An owned macro whose body spells `legacy::make<T>()` | body edited | body left | `not_parsed`; with `legacy::make` also declared by `dep/`: `mismatch changed` each function that expands it |
| N16 | A dependency macro that spells `legacy::T` for an argument naming a moved entity | `namespace legacy { using acme::Mine; }` added and declared `created` | nothing added | `not_parsed` |
| N17 | A stringised name (`#T` in a macro, giving `"legacy::Mine"`) | use left spelling `legacy::Mine` through a compatibility declaration, or the unit declared `changed` | respelled `acme::Mine` with nothing declared | `mismatch changed` the user: the string is a value of the program |
| N18 | `typeid(T).name()`, `__PRETTY_FUNCTION__`, `std::source_location` inside moved code | — | — | `good` only: symbolic (`digest.md` §9) |
| N19 | A moved class template and an owned partial specialisation of it in another header | both move | the specialisation's block is left in `legacy` | `not_parsed` |
| N20 | A moved function is also declared by the dependency (a forward declaration in `dep/`) | not moved (it is not owned alone) | moved | `mismatch indeterminate legacy::f`: `acme::f` mapped onto the staying `legacy::f` is two entities with one key |
| N21 | Relocation into a namespace that **already exists** and has a function of the name moved code calls unqualified (`helper(1)` meant `::helper(int)`; `acme::helper(long)` now hides it). Declared with one rule per moved entity, not a merged namespace | `::helper(1)` | left | `mismatch changed` the caller |
| N22 | The same, where an entity of the moved name and signature already exists in the destination | — | moved anyway | `good: none`; `not_parsed` if both are defined, else `mismatch removed` the pre-existing `acme` entity, which the rule captured |
| N23 | A default argument or default member initialiser in moved code that names a staying entity | qualified | left | as N01, and the silent tree must show `mismatch changed` at a **caller** that uses the default |
| N24 | A base class named without qualification (`struct Mine : Base`) where `Base` stays | `legacy::Base` | left | `not_parsed`; with a global `::Base`: `mismatch changed legacy::Mine` |

## 3. Renaming an entity

Setting: one entity is renamed in place; `declared.txt` is one `relocate`.

| Id | Hazard | Correct edit | Wrong edit | The wrong edit gives |
|---|---|---|---|---|
| R01 | The new name joins an existing overload set, and a call that chose another member of it now prefers the renamed function (`g(1)` chose `g(long)`; `f(int)` becomes `g(int)`) | — | renamed | `good: none`; `mismatch changed` the caller of `g` |
| R02 | A renamed member hides a base-class member that the class's own functions use unqualified | uses qualified `Base::y` | left | `mismatch changed` the member function |
| R03 | A virtual function renamed in the base and not in one override (no `override` written) | every override renamed, one rule each | one left | `mismatch` on the derived class or its function: it no longer overrides (`digest.md` "Open questions", overriding relations) |
| R04 | A renamed customisation point called unqualified from a dependency template | — | renamed | `good: none`; as N06 |
| R05 | A local variable renamed to the name of another local in an enclosing scope that the block also uses | another name | that name | `mismatch changed` the function. A second tree renames a local harmlessly: `good`, names of locals are not digested |
| R06 | A name produced by token pasting (`PREFIX##_init`) | the macro argument edited | the definition edited, the paste left | `not_parsed` |
| R07 | A member renamed to a name that a `requires` or `decltype` probe elsewhere tests for (`requires { t.size(); }`), making a constrained overload viable for the class | — | renamed | `good: none`; `mismatch changed` the caller that now selects the constrained overload, or `derived_changed` on the template that does |
| R08 | Two names swapped (`f` ↔ `g`), two rules | every use swapped | one call left | `mismatch changed` that caller |
| R09 | A renamed enumerator used as a template argument and in a `case` | all uses | — | `good` only: constant values are by value, the enumerator by key |
| R10 | A renamed type whose name was reached through an alias elsewhere | alias target respelled | — | `good` only |
| R11 | A function renamed where the translation unit keeps a block-scope declaration of the old name (`void g() { void f(int); f(1); }`) | that declaration renamed | left | `mismatch indeterminate`: the stale declaration and the renamed function are two entities with one key. (The same staleness in a translation unit that does not see the renamed declaration is `verify.md` §8, agreement between observers.) |

## 4. Edits that rename nothing

Setting: `declared.txt` is empty; the edit claims to change spelling or
layout only.

| Id | Hazard | Correct edit | Wrong edit | The wrong edit gives |
|---|---|---|---|---|
| E01 | Qualifying a member call suppresses virtual dispatch | `this->f()` | `Base::f()` | `mismatch changed` the caller |
| E02 | Qualifying a call disables argument-dependent lookup (`using std::swap; swap(a, b);` with a user `swap`) | left unqualified | `std::swap(a, b)` | `mismatch changed` the caller |
| E03 | Parentheses that are not redundant: `decltype((x))`; `(f)(x)` | `((x) + 1)` | `decltype((x))`, `(f)(x)` | `mismatch changed`; the first `good` tree adds only redundant ones |
| E04 | Reordering `#include`s changes what a non-dependent call inside a template sees at its definition | an order that keeps it | the other order | `mismatch changed` the template |
| E05 | Moving a function definition past a later, better overload (`h(long)`, caller, `h(int)`) | moved, call qualified by a cast that keeps `h(long)` — or not moved | moved | `mismatch changed` the caller |
| E06 | Respelling through a namespace alias, a using-declaration, an injected class name, a typedef of the same type | any of them | a typedef that adds `const` | `mismatch changed` |
| E07 | `struct` to `class` on a redeclaration; `override` added; a parameter renamed; a redundant `this->` | each | — | `good` only |
| E08 | An initialiser respelled so that `auto` deduces another type (`auto x = 1;` to `auto x = 1u;`) | `auto x = (1);` | `1u` | `mismatch changed` |
| E09 | Two namespace-scope variable definitions with dynamic initialisers exchanged | two functions exchanged | the variables | `mismatch changed` the order unit |
| E10 | A default argument changed on one declaration | — | changed | `good: none`; `mismatch changed` the function **and** a caller that relies on the default |
| E11 | Removing an `#include` that only seemed unused: it held an explicit specialisation, or a better overload | an include nothing depended on | that include | `mismatch` (`derived_changed` for the specialisation, `changed` for the overload); `removed` findings for what the header declared are expected and must be declared in the `good` tree |
| E12 | A declaration inserted in an inner scope shadows a name used later in it | inserted with a fresh name | with the used name | `mismatch changed` |
| E13 | Lines inserted above a use of `__LINE__` | — | inserted | until locations are symbolic (`digest.md` "Open questions"): `mismatch changed` the user. This row pins today's behaviour and is rewritten when that question closes |
| E14 | A `using namespace` moved from function scope to namespace scope | — | moved | `good: none`; `mismatch added` the directive, and `changed` wherever a later lookup now finds something else |
| E15 | A conversion made explicit where the implicit one chose another path (`T x = y;` to `T x(y);` with an `explicit` constructor present) | `T x = T(y)` where equivalent | `T x(y)` | `mismatch changed` |
| E16 | A lambda's default capture replaced by an explicit list that omits or by-values a variable | the same captures | `[=]` for `[&]` | `mismatch changed` |

## 5. What a row is for

Each row names the fact the digest must hold for the wrong edit to be
caught. When a row's test first passes, the rows of `digest-kinds.md` that
carry that fact name it as their must-differ test where they have no
smaller one (`digest.md` §13 obligation 5). A row whose wrong edit is
caught only by a failed parse (`not_parsed` in every `bad` tree) is marked
so in its `expect.txt`, and counted separately in the suite's summary: it
exercises the pipeline, not the digest.

## Tests

- `crates/slip_verify/tests/pitfalls.rs` — runs every directory under
  `tests/pitfalls/` as §1 says, and fails if a row of this file has no
  directory or a directory has no row.

## Open questions

- R11 and the agreement between observers: the single-observer form is
  written above; the form with two observers (one sees the renamed
  declaration, the other only a stale one) needs the check that `verify.md`
  §8 defers, and a test format with more than one translation unit.
- Rows for macros beyond N15–N17 and R06 wait for `spec/macros.md`.
- Rows that need more than one configuration (an entity declared under
  `#if` in one observer only) wait for the coverage report.
- Coroutines, modules and Objective-C are not represented.
- The production-compiler check of §1 uses whatever `clang++` provisioning
  installs; a row that the two compilers disagree on is a Kiln report, and
  is marked in `expect.txt` until it is resolved.
