# KR9 — what a name bound in a substitution whose result Kiln does not keep

A request from Slip to Kiln, and the Slip-side record of the exchange
about Kiln's WP-3540. Status: **draft, not sent** (stage 0 of
[docs/wp/WP-153.md](../wp/WP-153.md); it goes to design review first).
Kiln was read at `4a83f6e3c` (Slip's pin). Kiln's WP-3540 note itself
was not read: §2 and §3 are the supervisor's summary of it and of the
answers given (2026-11-24). The digest design this request serves is
[spec/landed/digest/WP-153.md](../../spec/landed/digest/WP-153.md).

## 1. The Slip need

Slip verifies a transform by comparing digests of the two parses. A
name inside a substitution whose result Kiln does not keep is seen only
through the substitution's outcome:

```cpp
struct Base { static int s(); };
namespace n { struct S : Base { static int s(); }; }
template <class T, int = sizeof(T::s())> int call() { return 0; }
int u() { return call<n::S>(); }
```

Rename `n::S::s`. `T::s` at `call<n::S>` now finds `Base::s`. The
default argument's value is in the specialisation's key and is equal;
the substituted expression is in no declaration. Every digest is equal.

Slip needs, for every substitution an observer made, **what each
dependent name of the substituted pattern bound**, under a key that
names the substitution and that Slip can map to a key of its own
(a declaration and canonical template arguments).

The same need, measured in program behaviour: the binding decides what
is instantiated. `spec/landed/digest/WP-153.md` §1 has programs that
print a line before the rename and nothing after it, with the outcome
of the substitution equal (clang 19 and GCC agree).

## 2. What Kiln proposed (WP-3540, phase 0; frozen for stages 0 and 1)

1. A second table; `WrittenRefs::specs()` is unchanged.
   `WrittenRefs::subst_keys` (24-byte keys) and `subst_rows` (24-byte
   rows), behind the sub-switch `-fkiln-written-ref-keys`.
2. A key is (kind, template, canonical arguments, payload):
   - `PartialMatch`: the partial specialisation, the specialisation's
     arguments; the deduced list as payload;
   - `Satisfaction`: the concept or the constrained declaration, the
     arguments shown;
   - `DefaultArg`: the template, the parameter, the prior arguments
     (beside the re-keyed `specs()` row of Kiln WP-3520);
   - `Alias`: the alias template, the canonical arguments.
   A key has concrete arguments. It exists only once its substitution
   succeeded with at least one row.
3. Rows are buffered per open substitution and committed as one
   contiguous run at its successful end. A failure drops the buffer. A
   committed key is never recorded again: the first successful run in
   translation order stands.
4. Every cache maps one-to-one to a key, except the atomic-constraint
   cache, whose entries store row bodies and replay them into the open
   key.
5. Nesting: the innermost frame decides. A nested keyed substitution
   has its own buffer. A nested real instantiation stays in `specs()`.
6. An out-of-line redeclaration's whole signature (return type,
   parameters, `noexcept`, requires-clause) gets rows in `specs()`
   under the specialisation's `DeclId`.
7. Found on the way: a member template's default argument in a class
   specialisation has no row today; a "home context" rule fixes that
   and the gap for an unqualified call in a function template's own
   requires-clause.
8. Stages: 0 the table and the switch; 1 `PartialMatch`; 2
   `Satisfaction` (changes `specs()` on purpose); 3 `DefaultArg`; 4
   `Alias`; 5 the redeclaration's rows. Estimate: 0.4 to 1.25 MB on
   `<bits/stdc++.h>`.
9. At the freeze: a row flag `SUBST_EXPANDED` (the row is of an alias's
   names, expanded under an enclosing key). Kiln's reason: a row's
   `enclosing` is the alias template when the name is directly in the
   alias's type-id, and is not when the name sits in a declaration
   nested inside the type-id (a parameter of a function type, a
   lambda's call operator).
10. Readable omissions: `Unrecorded::PrefixKeys { files }`,
    `ModuleKeys { module, files }`,
    `ModuleKeysLeftOut { module, keys, rows }`, and
    `subst_keys_complete()`.

What Kiln's source says today, where this request leans on it:

- The key of a row is read off the instantiation stack by
  `written_spec_key` (`kiln/crates/kiln_sema/src/written_specs.rs`). An
  `AliasTemplate` or `DefaultTemplateArg` frame gives the frame's
  template; a constraint frame gives the declaration the constraint
  belongs to; any other frame that is an instantiation
  (`FrameKind::is_instantiation`,
  `kiln/crates/kiln_sema/src/templ/stack.rs`) gives its entity; every
  other frame gives no key and nothing is recorded.
- `in_sfinae_held` (`kiln/crates/kiln_sema/src/templ/deduce_call.rs`)
  ends with `end_written_spec_sfinae`, which drops the rows keyed by the
  deduced template itself, also when deduction succeeded.
- Arenas are never truncated: `in_sfinae_held` takes
  `self.ast.exprs.len()` only to flush member references. The nodes a
  substitution built stay allocated; nothing points at them.
- An operator expression has no entry and so no row
  (`kiln/spec/ast.md` §18.7.4, last item of the list).

## 3. What Slip answered

The numbers are those of Kiln's twelve questions. "(p)" was given as
provisional until this design; §3.1 has the design's word on each.

| # | Answer | State |
|---|---|---|
| 1 | Keys only where rows exist: enough | firm; **see §6 item 1** |
| 2 | No `selected` mark on a `PartialMatch` key; every matching partial specialisation gets a key, also one that loses partial ordering; Slip reads the chosen pattern from the specialisation | (p): **confirmed** |
| 3 | `DefaultArg` commits even if the enclosing deduction fails later | firm |
| 4 | Absence is enough for entries the standard never substituted (the right operand of a satisfied disjunction, an empty pack, a failed atom inside a satisfied whole). An entry that was substituted and stays dependent has a row | firm |
| 5 | No flag for rows of an alias's names under an enclosing key | (p): **changed at the freeze** to the flag `SUBST_EXPANDED`; Slip does not rely on the entry's `enclosing` for such a row (§5.1) |
| 6 | No key ever has dependent arguments | (p): **confirmed as Slip's recommendation; the supervisor rules** (§6 item 2) |
| 7 | Redeclarations: the whole signature, in `specs()` under the specialisation | firm |
| 8, 12 | A module image that leaves a key out, and a PCH made without the switch: accepted if the omission is readable | firm; Slip's reading is §5.7 |
| 9 | A member's `Satisfaction` or `DefaultArg` key names the member of the specialisation (a `DeclId`) and the member's own arguments | firm; checked: §3.1 |
| 10 | A function template's signature in deduction (keyed by the formed specialisation) and a requires-expression inside a function body (keyed by the instantiation) stay in `specs()` only | firm; it makes §5.2 necessary |
| 11 | A diagnosed error gets no key; an ill-formed program, no diagnostic required, gets the first run's rows | firm |

Also asked of Kiln's specification: a row carries the pattern entry,
the entity or the found set, `adl`, and "stays dependent"; rows within
a key are in a fixed order that does not depend on other keys; each
pull request lists what new rows a reader of `specs()` sees.

Told to Kiln earlier and unchanged: successes only, first; the order of
need (`PartialMatch`, `Satisfaction`, `DefaultArg`, `Alias`, the
out-of-line `noexcept` operand).

### 3.1 The design's word on the provisional answers

- **2, confirmed.** A match that loses partial ordering is a
  substitution that succeeded, and its names bound. Slip writes one
  unit per `PartialMatch` key, chosen or not. Which pattern was chosen
  is already in the specialisation (`SpecInfo` `from`), so the key
  needs no mark.
- **5.** The flag is harmless to Slip and is not what Slip names the
  entry by. Slip names an entry by the declaration that owns it and its
  ordinal there (§5.1). For a name nested in a declaration inside an
  alias's type-id the owner cannot be found from `enclosing` today;
  until §5.1 is answered Slip marks such a unit indeterminate.
- **6.** See §6 item 2.
- **9, checked.** Every declaration that is not local has a Slip key
  (`crates/slip_digest/src/keys.rs` `key`, `spec_key`): a member of a
  class template specialisation is keyed under the specialisation's
  key. A local declaration has none (`keys.rs` `is_local`): the call
  operator of a closure, a member of a local class. A `Satisfaction`
  key of a constrained generic lambda therefore has no Slip key yet
  (`spec/landed/digest/WP-153.md` §6.3).

## 4. The four properties, made exact

Stated for both tables. "Key" is a `subst_keys` key, or for `specs()`
the pair (`spec`, part) of §5.2.

- **P1. One row set per key.** For a key `K`, the set of
  (entry, row body) pairs is a function of the translation unit's text
  up to the first point where `K`'s substitution succeeds. It does not
  depend on how many later uses ask for `K`, on whether a later use is
  answered from a cache (the satisfaction cache, the atomic-constraint
  cache, the alias cache, the specialisation table), or on the order in
  which other keys are substituted. Kiln's rules 3 and 4 give this: the
  first successful run stands, and a replayed atom gives the rows its
  first substitution gave. What Slip needs said in Kiln's
  specification: a replay into key `K2` of an atom first substituted
  under key `K1` gives `K2` exactly the rows a fresh substitution under
  `K2` would give.
- **P2. Every substituted entry has a row.** Every `DEPENDENT` entry of
  the pattern that the substitution reached has a row under the key: an
  entity; or, for an entry that the substitution left dependent, a row
  that says so, with the declarations lookup found now and whether
  argument-dependent lookup will run. An entry the substitution did
  not reach has none (answer 4). An entry it reached and that failed
  to bind cannot occur under a key: the substitution failed and has no
  key. The exception is a failed atom or requirement inside a
  satisfaction check that completed: its names up to the failing one
  have rows, the failing one has none.
- **P3. Keys can be listed and mapped.** `subst_keys` lists every key
  with its kind, its declaration (`DeclId`), its canonical argument
  list (an interned list id) and its payload, so that Slip forms
  (key of the declaration, digest of the arguments) without reading a
  position.
- **P4. A key's rows do not depend on which other keys exist.** A
  nested keyed substitution's rows are under its own key and not under
  the enclosing one, whether the nested key was substituted now or
  earlier. The one stated exception is `SUBST_EXPANDED`: an alias the
  enclosing pattern keeps only as its expansion has its names' rows
  under the enclosing key. That is a function of the enclosing pattern
  and so of the enclosing key.

**What a row must say, by kind of entry:**

| Entry after substitution | The row gives | Slip reads |
|---|---|---|
| Resolved to a function, variable, member, enumerator, template, concept or namespace | `entity` | the entity |
| Resolved to a type name (a class, an enumeration, a typedef, an alias) | `entity` | the type it names |
| An unqualified call that argument-dependent lookup applied to, resolved | `entity`; `how`; `adl` | the entity only: the callee is the fact (`spec/digest.md` §6) |
| Left dependent (a name inside a generic lambda or a nested template of the pattern) | the flag; the declarations lookup found now; whether argument-dependent lookup runs later | the flag, the found set as a sorted set, the bit |
| Resolved to a builtin candidate | `how` `Builtin`, no entity | that fact |

`found`, `udir` and the order of an `adl` set are not read by the
digest (they say how the entity was reached). The rename and the
namespace rule's edit planning read them (`spec/edits.md` §9.4).

## 5. What Slip additionally needs

| Part | Kind |
|---|---|
| §5.1 The owner of an entry | **Design** (small): the form is Kiln's; the property is stated here |
| §5.2 The part of a `specs()` row | **Design** (small) |
| §5.3 Selections that have no written name | **Novel design** |
| §5.4 Shapes with no key and no row | **Implementation** in the frozen scheme; one new kind is **design** |
| §5.5 A key for every substitution made | **Design**, asked only if the supervisor rules for it (§6 item 1); a count is asked now |
| §5.6 The element of a pack expansion | **Design** (small), low priority |
| §5.7 Readable omissions | **Implementation**: a confirmation |

### 5.1 The owner of an entry

Slip's digest holds no position. It names the entry of a row by a pair
(owner, ordinal): the owner is the entity in whose declaration the name
is written, and the ordinal is the entry's rank among the `DEPENDENT`
entries of that owner in parse order. This name survives a rename of
the identifier, a move of the declaration, and a qualifier added to a
name that is not dependent.

Needed: for every `DEPENDENT` entry, the owner: the innermost enclosing
declaration that is declared at namespace or class scope (a template, a
partial specialisation, an alias template, a concept, a member), through
whatever declaration is nested between (a parameter of a function type,
a template parameter, a lambda's call operator, a local declaration).
`WrittenRef::enclosing` (`kiln/spec/ast.md` §18.3) is that declaration
in most cases and is not for the nested ones Kiln named at the freeze;
names of a template head and of an alias declaration are enclosed by
the lexical context until Kiln WP-3143.

Either form serves: `enclosing` made to hold this for `DEPENDENT`
entries, or an accessor `owner(entry) -> DeclId`. With it Slip does not
need `SUBST_EXPANDED` to name an entry.

Acceptance: over Kiln's written-reference corpus, every entry that has
a row in `specs()` or `subst_rows` has an owner that is not a namespace
and not the translation unit.

### 5.2 The part of a `specs()` row

`specs()` keys a row by a `DeclId`. For a function that one `DeclId`
gathers rows of substitutions made at different times: its signature
(at deduction), its body (when it is defined), its exception
specification (when it is resolved), each default argument (when a call
uses it). Slip puts a fact that exists only on demand in a unit that
exists only on that demand (`spec/digest.md` §1 rule 5). One unit per
`DeclId` would differ between two parses that differ only in whether a
body was instantiated.

Needed: per `specs()` row, which substitution recorded it, to the
grain of Kiln's frames: the declaration (the signature, or for a class
the members' declarations made with the class), the definition (a
function's body, a variable's initialiser), the exception
specification, a default function argument (and which parameter; the
entry's `enclosing` parameter gives it today), a default member
initialiser, an out-of-line redeclaration's signature (stage 5).
`written_spec_key` knows the frame's kind when it returns the key.
`SpecRef` has `flags: u8` and `how: RefLookup` after six 4-byte
fields in 28 bytes, so a byte may be free; the form is Kiln's.

Until this exists Slip reads nothing of `specs()` for its digest, and
the shapes of answer 10 stay under the stated limit.

### 5.3 Selections that have no written name

A row needs an entry, and an entry needs a written name. What a
substitution selects without one has no row: the function of an
operator expression, a constructor, a conversion function, the
`begin`, `end` or `get` of a range-for or a structured binding. In a
kept expression Slip reads the selection off the expression
(`spec/digest.md` §6). In a substitution that is not kept nothing
holds it.

```cpp
namespace a { struct X {}; int operator<<(X, X); }
template <class...> using void_t = void;
template <class T> T val();
template <class T, class = void> struct shl { static const int v = 0; };
template <class T>
struct shl<T, void_t<decltype(val<T>() << val<T>())>> { static const int v = 1; };
int u = shl<a::X>::v;
```

Move `operator<<` to the global namespace and declare it before `shl`:
it is found by ordinary lookup instead of argument-dependent lookup,
`shl<a::X>::v` is 1 both times, and no row can say it. This is the
form of every "is streamable", "is equality comparable" and "is
callable with" trait, and operator lookup is the lookup a namespace
migration moves most easily.

Needed, in this order: (a) operator expressions of a pattern (unary,
binary, compound assignment, subscript, call, arrow, a fold's operator,
a rewritten comparison), each with the function selected or the fact
that a builtin operator was; (b) constructors and conversion functions;
(c) the rest. Two forms are possible and the choice is Kiln's:

- rows whose entry is a pattern node and not a written-reference entry
  (`PatternNode::Expr` exists, `kiln/crates/kiln_ast/src/written_refs.rs`);
  the substituter rebuilds these through `act_on_*` with
  `oper_rebuild_set` (`kiln/crates/kiln_sema/src/overload/oper.rs`), a
  small number of sites;
- per key, the root of what the substituter built (an expression, a
  type, an argument list). The nodes exist and are not freed. Slip's
  ordinary walk then reads every selection, implicit ones included.
  This form does not serve a failed substitution, and Slip would have
  to keep the sugar an argument carried out of the unit, so it is the
  second choice.

The builtin traits that run overload resolution themselves
(`__is_constructible`, `__is_assignable`, `__is_convertible`,
`__is_invocable`, `__is_trivially_equality_comparable`, the
`__builtin_*_synthesizes_from_spaceship` family;
`kiln/crates/kiln_sema/src/templ/builtins/traits.rs`) are the same
class: only the trait's value is stored. They come after (a) to (c).

### 5.4 Shapes with no key and no row

Each was read in Kiln's source at the pin.

1. **The type of a non-type template parameter, substituted with the
   prior arguments at a template-id.** `nttp_arg_param_type`
   (`kiln/crates/kiln_sema/src/templ/args.rs`) pushes
   `FrameKind::PriorArgSubstitution`, which is not an instantiation, so
   `written_spec_key` gives no key. The substituted type is used to
   convert the argument and is not kept.

   ```cpp
   struct Base { static int s(); };
   namespace n { struct S : Base { static int s(); }; }
   template <class T, decltype(T::s()) N = 0> struct P {};
   P<n::S> p;
   ```

   Needed: a kind (the template, the parameter, the prior arguments),
   as `DefaultArg`. In deduction the same type is substituted under
   `DeducedArgSubstitution` keyed by the template, and dropped.
2. **The explicit-specifier of a function template specialisation.**
   `function_spec_explicit`
   (`kiln/crates/kiln_sema/src/templ/subst/decls.rs`) runs inside
   `in_sfinae(FrameKind::DeducedArgSubstitution, ft, ..)`
   (`deduce_call.rs`, after `deduced_constraints`), so its rows are
   keyed by `ft` and dropped. Only the bit is kept. Needed: rows under
   the formed specialisation, as the signature's.
3. **Class template argument deduction.** The alias guide
   (`build_alias_guide`, `kiln/crates/kiln_sema/src/templ/ctad.rs`) is
   built with no frame; the default arguments of an inherited
   constructor's guide are substituted under
   `in_sfinae(DeducedArgSubstitution, c)`
   (`kiln/crates/kiln_sema/src/templ/ctad/inherit.rs`) and their rows
   dropped. Needed: Kiln's word on which of these bind a name of a
   pattern, and rows for those.
4. **Template template argument matching** (`PartialOrderingTtp`,
   `kiln/crates/kiln_sema/src/templ/ttarg22.rs`): the non-type
   parameter types of both lists are substituted and compared. No key.
   Needed: as item 1, at low priority.
5. **Variable template partial specialisations.** `get_var_spec`
   (`kiln/crates/kiln_sema/src/templ/var.rs`) matches through the same
   `match_partial_args`. Needed: a sentence that `PartialMatch` covers
   them.

### 5.5 A key for every substitution made

See §6 item 1. Asked now: on `<bits/stdc++.h>`, the number of
substitutions of the four kinds whose pattern has at least one
`DEPENDENT` entry and that (a) succeeded with no row, (b) failed.
Those are the keys the frozen design does not make.

### 5.6 The element of a pack expansion

`push_spec` (`kiln/crates/kiln_ast/src/written_refs.rs`) keeps one row
per distinct resolution of an entry under a key, with no element
index. Two elements whose bindings swap give the same rows. Slip
accepts this as a stated limit. If a row can carry the element's index
at no cost in the new table, Slip will read it.

### 5.7 Readable omissions

Slip's reading, to be confirmed: `subst_keys_complete()` is false
exactly when at least one `Unrecorded` value exists; `PrefixKeys` and
`ModuleKeys` name every file whose substitutions may be missing;
`ModuleKeysLeftOut` names the module. Slip writes one indeterminate
unit per named file or module, so a comparison is never equal by
silence (`spec/landed/digest/WP-153.md` §6.1). Slip makes its own
prefix and module images with the switch on.

## 6. Still open between Slip and Kiln

1. **A key that only one parse has.** Answer 1 said that a key
   appearing or disappearing between the two parses is a difference.
   In Slip's comparison it is not: a derived unit that one side lacks
   is counted and not reported (`spec/verify.md` §4 step 3), because a
   transform may change what is instantiated. So a substitution that
   bound names before and fails after, or reaches none of its names
   after, with everything it decides equal, is not reported. Two ways
   to close it: Kiln makes a key for every substitution made whose
   pattern has a `DEPENDENT` entry, with its ending (rows; none;
   failed), so that a key's presence says only that the substitution
   was asked for; or Slip reports a one-sided key, and then a transform
   that changes what is instantiated cannot verify. The first is
   exact. Its cost is §5.5's count. The supervisor rules
   (`spec/landed/digest/WP-153.md`, open question 3).
2. **Kiln's question 6: an alias-id with partly concrete arguments.**

   ```cpp
   template <class X, class Y> using A2 = typename X::t (*)(typename Y::t);
   ```

   used as `A2<n::S, T>` in a template that is never instantiated.
   `X::t` binds at the expansion; `specs()` has the row under `A2`; no
   key forms. Slip's recommendation: no partly dependent key, and the
   digest does not read these rows. The name bound while a pattern was
   parsed, with nothing instantiated, so it is held to the standard of
   a name written directly in that pattern (`typename n::S::t`): seen
   through the pattern's canonical type. Rows keyed by the alias
   template alone would make one set for all uses in the translation
   unit, which changes whenever a transform adds or removes a use. The
   supervisor rules (open question 6).
3. **Failed substitutions.** Not asked (§7). The case where a name
   bound, the substitution failed later, and the binding reached the
   program exists (`spec/landed/digest/WP-153.md` §1). Slip proposes to
   close it in its comparison and not with rows.
4. §5.3: which form, and what it costs.
5. §5.1 and §5.2: the form.

## 7. What is not asked

- **Rows of a failed substitution.** A substitution that failed decided
  nothing through its names: the candidate is gone, the partial
  specialisation is not matched, and that outcome is compared. What it
  can still do is instantiate: a name that bound before the failing one
  can force a definition, and a definition can emit code. Slip closes
  that from its own side (a derived unit of a kind that is emitted
  without a reference, present in one image only, becomes a finding;
  open question 1 of the proposal), which also covers every other
  demand no row describes. A nested substitution that succeeded inside
  a failed one keeps its key (answer 3), and that is wanted.
- **A `selected` mark** on a `PartialMatch` key (§3.1).
- **Keys with dependent arguments** (§6 item 2, if ruled as
  recommended).
- **The substituted form itself** as the primary record. The rows are
  what Kiln designed and what the edit planning needs; §5.3 offers the
  form as one way to give selections that have no name.
- **A change to `specs()`'s keys.** Its rows stay keyed by `DeclId`;
  §5.2 asks for one more fact per row.

## 8. What Slip does until then

The limit is stated in `spec/digest.md` §2.3 and
`spec/digest-kinds.md` "Hazards accepted for now". Nothing is counted
(`spec/landed/digest/WP-153.md` §12). The build's stages follow
Kiln's: each Slip package starts when its rows are in the pin (§10 of
the proposal).

## 9. Acceptance examples

For each, the expected content under Slip's reading. `pre.h` is

```cpp
struct Base { static int s(); };
namespace n { struct S : Base { static int s(); }; }
```

| Source after `#include "pre.h"` | Key | Rows |
|---|---|---|
| `template <class T, int = sizeof(T::s())> int call() { return 0; }` `int u() { return call<n::S>(); }` | `DefaultArg` (`call`, parameter 1, `<n::S>`) | `s` → `n::S::s` |
| `template <class T> using R = decltype(T::s()); R<n::S> v;` | `Alias` (`R`, `<n::S>`) | `s` → `n::S::s` |
| `template <class T> concept HasS = requires { T::s(); };` `template <HasS T> int g() { return 0; }` `int u() { return g<n::S>(); }` | `Satisfaction` (`HasS`, `<n::S>`); none for `g` unless its own constraint has a row | `s` → `n::S::s` |
| `template <class T> auto f(T) -> decltype(T::s()); int u = sizeof(f(n::S()));` | `specs()`, `f<n::S>`, part declaration | `s` → `n::S::s` |
| `template <class T> void g(T) noexcept(noexcept(T::s())) {}` `bool u = noexcept(g(n::S()));` | `specs()`, `g<n::S>`, part exception specification | `s` → `n::S::s` |
| `template <class T> int h() { if constexpr (requires { T::s(); }) return 1; return 0; }` `int u() { return h<n::S>(); }` | `specs()`, `h<n::S>`, part definition | `s` → `n::S::s` |
| `template <class T> struct A { int a[sizeof(T::s())]; }; A<n::S> x;` | `specs()`, `A<n::S>`, part declaration of the class's members | `s` → `n::S::s` |

A partial specialisation's match, with argument-dependent lookup:

```cpp
namespace a { struct X {}; void swap(X &, X &); }
template <class...> using void_t = void;
template <class T> T &ref();
template <class T, class = void> struct swappable { static const int v = 0; };
template <class T>
struct swappable<T, void_t<decltype(swap(ref<T>(), ref<T>()))>> { static const int v = 1; };
int u = swappable<a::X>::v;
```

Key `PartialMatch` (the partial specialisation, `<a::X, void>`); rows:
`swap` → `a::swap(X &, X &)`, with its `adl` set. Whether `ref` (a
template name whose arguments are dependent) is a `DEPENDENT` entry
with a row is Kiln's rule; Slip reads the row if there is one.

Order independence (P1): each example with the use written twice, with
the second use first, with the argument spelled through an alias
(`using N = n::S;`), and with the first use inside an instantiation
that a later declaration forces, gives the same keys and the same rows.
