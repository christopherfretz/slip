# KR9 — what a name bound in a substitution whose result Kiln does not keep

A request from Slip to Kiln, and the Slip-side record of the exchange
about Kiln's WP-3540. Status: **sent 2026-11-24** (stage 0 of
[docs/wp/WP-153.md](../wp/WP-153.md); revised after the design review
and the supervisor's rulings of 2026-11-24; §10 lists what changed).
**Ruled the same day: Slip's digest reads roots for all four kinds**
(§5.3), so the region of §5.1 is not asked unless a kind falls back to
rows.
Kiln was read at `4a83f6e3c` (Slip's pin). Kiln's WP-3540 note itself
was not read: §2 and §3 are the supervisor's summary of it, of the
answers given, and of Kiln's answer of 2026-11-24 (§2.1), which is
used here as fact. The digest design this request serves is
[spec/landed/digest/WP-153.md](../../spec/landed/digest/WP-153.md)
("the proposal"). Kiln cannot read Slip's repository: §2.1, §4, §5, §6 and §9
were sent inline in four messages, with the additions marked "(sent
inline)" below.

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

Slip needs, for every substitution an observer completed, **what the
substitution selected**: what each name of the pattern bound, and which
function each operator, constructor and conversion in it selected,
under a key that names the substitution and that Slip can map to a key
of its own (a declaration and canonical template arguments).

The binding also decides what is instantiated, and that can change
what a program does (the proposal's §1 has programs that print a line
before a rename and nothing after it). Slip closes the behaviour on its
own side (its WP-155); §5.9 asks one thing for it.

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
   A key has concrete arguments. It existed only once its substitution
   succeeded with at least one row (**amended, §2.1**).
3. Rows are buffered per open substitution and committed as one
   contiguous run at its successful end. A failure drops the buffer
   (**amended for a failed atom, §2.1**). A committed key is never
   recorded again: the first completed run in translation order stands.
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
   names, expanded under an enclosing key). Kiln's reason as relayed: a
   row's `enclosing` is the alias template when the name is directly in
   the alias's type-id, and is not when the name sits in a declaration
   nested inside the type-id. **Against the pin this is not so**: for a
   name directly in an alias template's type-id `enclosing` is the
   namespace (§5.1's table). The statement describes Kiln after its
   WP-3143, which is later than the pin (§2.1).
10. Readable omissions: `Unrecorded::PrefixKeys { files }`,
    `ModuleKeys { module, files }`,
    `ModuleKeysLeftOut { module, keys, rows }`, and
    `subst_keys_complete()`.

### 2.1 Kiln's answer of 2026-11-24

1. **WP-3540 is amended and frozen again: a key for every completed
   substitution**, also with no row, also a check that ended
   unsatisfied.
   - An unsatisfied check is a key with `SubstOutcome::Unsatisfied`.
     The outcome is not part of the key's identity.
   - "Completed", per kind: `PartialMatch`, matched, its constraints
     included (a non-match gets no key); `DefaultArg` and `Alias`,
     substituted, and converted where the site checks, with no captured
     failure; `Satisfaction`, the check produced an answer (a
     substitution failure inside an atom makes the atom false and the
     check completes; a hard error does not).
   - **The one row rule: a name that bound has a row; a name that did
     not bind or was never reached has none.** Both trims (a failed
     atom, a false requires-expression) are withdrawn; a failed atom's
     cache entry keeps and replays the rows of its names that bound.
   - "The pattern has a dependent entry" is a static test filled by the
     parser, so keys do not come and go with what was reached.
   - Kiln's WP-3520 lands before its stage 2.
   - Estimate on `<bits/stdc++.h>`: 6,000 to 12,000 keys in all (was
     3,000 to 6,000), 0.2 to 0.4 MB more. Measured upper bounds: 3,377
     satisfaction checks computed, 3,524 distinct alias
     specialisations, 23,081 default-argument substitutions over 1,054
     templates.
2. **No partly dependent keys**; those rows stay in `specs()`.
3. **A root per key, costed.** `Alias`: complete and already kept
   alive (the alias cache), built without type locations.
   `DefaultArg`: complete; the root would be the argument before
   conversion. `PartialMatch`: the substituted argument list is built
   then dropped; interning it gives the root; the shortcut for a bare
   non-type parameter builds nothing. **`Satisfaction`: not complete**:
   a fully concrete requires-expression is rebuilt with an empty
   requirement list (the substituted requirements are orphaned); a
   failed atom has no root; a hit in the atom cache builds nothing (the
   entry would have to store its expression). For all kinds the
   operands are unevaluated (nothing odr-used, no body instantiated).
   About 0.1 to 0.3 MB on the header; a module image writes only
   reachable nodes, so roots add bytes there. Kiln recommends no field
   in the key: a parallel table `subst_roots` behind its own switch,
   later; the key stays 24 bytes.
4. **`enclosing`.** Kiln's WP-3143, later than Slip's pin, already
   fixes five cases of §5.1's table: a default template argument, an
   alias's type-id, a head requires-clause and a non-type parameter's
   type are held by the templated declaration; a member template's head
   names by the member, not the class. Residual: a generic lambda's
   head is held by the enclosing variable, not its call operator; a
   template template parameter's inner head by the outer template;
   names in a function-type parameter or a lambda inside an alias's
   type-id by that parameter or call operator. **The region does not
   exist**: `enclosing` gives the owner only.

### 2.2 Kiln's source at the pin

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
  (`kiln/spec/ast.md` §18.7.4, last item of the list; not probed).

Slip's design review dumped the written references at the pin and
confirmed: no row for a function or member template's default template
argument, for a partial specialisation's match, for the type of a
non-type template parameter, or for `explicit(bool)`; rows keyed by the
alias template and by the concept; rows under the specialisation for a
deduced signature, a `noexcept` operand, a requires-expression in a
body and an array bound.

## 3. What Slip answered

The numbers are those of Kiln's twelve questions.

| # | Answer | State |
|---|---|---|
| 1 | "Keys only where rows exist: enough" | **withdrawn** (told to Kiln 2026-11-24, with the question about an unsatisfied check). Kiln amended WP-3540 the same day: a key for every completed substitution (§2.1 item 1). §5.5 has the reason and what is left of it |
| 2 | No `selected` mark on a `PartialMatch` key; every matching partial specialisation gets a key, also one that loses partial ordering | ruled: confirmed |
| 3 | `DefaultArg` commits even if the enclosing deduction fails later | firm |
| 4 | Absence is enough for entries the standard never substituted (the right operand of a satisfied disjunction, an empty pack, a failed atom inside a satisfied whole). An entry that was substituted and stays dependent has a row | firm, and now Kiln's row rule (§2.1): a failed atom's names that bound before the failing one have rows. Which substitutions give "stays dependent" rows is asked in §4, P2 |
| 5 | A flag `SUBST_EXPANDED` for rows of an alias's names under an enclosing key | accepted. Slip does not name an entry by it or by `enclosing`; it needs the owner (§5.1) |
| 6 | No key ever has dependent arguments | ruled: confirmed. An alias-id, default argument or concept-id with partly concrete arguments gets no key, and Slip's digest does not read its `specs()` row (told to Kiln) |
| 7 | Redeclarations: the whole signature, in `specs()` under the specialisation | firm |
| 8, 12 | A module image that leaves a key out, and a PCH made without the switch: accepted if the omission is readable | firm; Slip's reading is §5.7 |
| 9 | A member's `Satisfaction` or `DefaultArg` key names the member of the specialisation (a `DeclId`) and the member's own arguments | firm; checked against `crates/slip_digest/src/keys.rs` (`key`, `spec_key`): every such declaration has a Slip key unless it is local (`is_local`: a closure's call operator, a member of a local class), and a key of a local declaration is counted and left out |
| 10 | A function template's signature in deduction and a requires-expression inside a function body stay in `specs()` only | firm for Kiln; Slip's digest reads the AST for the first and asks for the second (§5.4, §5.8) |
| 11 | A diagnosed error gets no key; an ill-formed program, no diagnostic required, gets the first run's rows | firm |

Also asked of Kiln's specification: a row carries the pattern entry,
the entity or the found set, `adl`, and "stays dependent"; rows within
a key are in a fixed order that does not depend on other keys; each
pull request lists what new rows a reader of `specs()` sees.

Unchanged: a substitution that failed has no key (a check that
completed unsatisfied is not one; §2.1). The order of need is now: `PartialMatch`,
`Satisfaction`, `DefaultArg`, **operators** (§5.3), `Alias`, the
out-of-line redeclaration.

## 4. The four properties, made exact

Stated for a key of `subst_keys` and for whatever is recorded under it:
rows, or a root (§5.3).

- **P1. One content per key.** For a key `K`, what is recorded is a
  function of the translation unit's text up to the first point where
  `K`'s substitution completes. It does not depend on how many later
  uses ask for `K`, on whether a later use is answered from a cache
  (the satisfaction cache, the atomic-constraint cache, the alias
  cache, the specialisation table), or on the order in which other
  keys are substituted. Kiln's rules 3 and 4 give this for rows. What
  Slip needs said in Kiln's specification: a replay into key `K2` of an
  atom first substituted under key `K1` gives `K2` exactly what a fresh
  substitution under `K2` would give.
- **P2. Everything that bound is recorded.** For rows, Kiln's rule
  (§2.1): a name that bound has a row; a name that did not bind or was
  never reached has none. A row is an entity, or, for an entry that
  the substitution left dependent, a row that says so, with the
  declarations lookup found now and whether argument-dependent lookup
  will run. A failed atom's names that bound before the failing one
  have rows, also when the atom is replayed from the cache. For a
  root: §5.3.

  **A question before "stays dependent" is asked in full**: which
  substitutions give such rows? Instantiating a class leaves the head
  names of every member template dependent and gives none today (the
  review's dump: a member template's default template argument inside
  an instantiated class has no row). Slip needs them for the four
  kinds' own patterns (a generic lambda or a nested template inside a
  default argument, a constraint, an alias's type-id, a partial
  specialisation's arguments). What would rows for every member
  template of every instantiated class cost? Slip does not ask for
  those before it knows.
- **P3. Keys can be listed and mapped.** `subst_keys` lists every key
  with its kind, its declaration (`DeclId`), its canonical argument
  list (an interned list id) and its payload, so that Slip forms
  (key of the declaration, digest of the arguments) without reading a
  position.
- **P4. A key's content does not depend on which other keys exist.** A
  nested keyed substitution's content is under its own key and not
  under the enclosing one, whether the nested key was substituted now
  or earlier. The one stated exception is `SUBST_EXPANDED`: an alias
  the enclosing pattern keeps only as its expansion has its names'
  rows under the enclosing key. That is a function of the enclosing
  pattern and so of the enclosing key.

**What a row must say, by kind of entry:**

| Entry after substitution | The row gives | Slip's digest reads |
|---|---|---|
| Resolved to a function, variable, member, enumerator, template, concept or namespace | `entity` | the entity |
| Resolved to a type name (a class, an enumeration, a typedef, an alias) | `entity` | the type it names |
| An unqualified call that argument-dependent lookup applied to, resolved | `entity`; `how`; `adl` | the entity only: the callee is the fact (`spec/digest.md` §6) |
| Left dependent | the flag; the declarations lookup found now; whether argument-dependent lookup runs later | the flag, the found set as a sorted set, the bit |
| Resolved to a builtin candidate | `how` `Builtin`, no entity | that fact |

`found`, `udir` and the order of an `adl` set are not read by the
digest (they say how the entity was reached). The rename and the
namespace rule's edit planning read them (`spec/edits.md` §9.4).

## 5. What Slip additionally needs

| Part | Kind | The Slip need behind it |
|---|---|---|
| §5.1 The region of an entry; the owner's residual cases | the region is **design** (small; it does not exist); the residual is **implementation** | naming a row's entry without a position; needed by every kind the digest reads as rows, by none it reads as a root |
| §5.2 `specs()` rows after stages 2 to 4 | **Implementation**, one question | the digest must not read one substitution twice, or a set that grows with every use |
| §5.3 A root per key; selections that have no written name | **Novel design** (Kiln has costed it); for `Satisfaction` two pieces of **implementation** | operators, constructors and conversions are in the promise; a root removes §5.1 for the digest |
| §5.4 Shapes with no key and no row | **Implementation** in the frozen scheme; one new kind is **design**; four questions | each is a substitution an observer makes and Slip cannot see |
| §5.5 A key for every completed substitution | granted (§2.1); one **question** on its condition, one count | a unit both images have is compared; one only one has is not |
| §5.6 The element of a pack expansion | **Design** (small), low priority | two elements that swap |
| §5.7 Readable omissions | **Implementation**: a confirmation | never equal by silence |
| §5.8 Operands a type or a value holds | **Questions**; small **implementation** where the answer is no | Slip reads them from the AST and must know they are there |
| §5.9 "Emitted whether or not referenced" | **Implementation** | Slip's WP-155 marks such units |

### 5.1 The region of an entry; the owner's residual cases

Slip's digest holds no position. Where it reads rows it names the
entry of a row by (owner, region, ordinal): the owner is the entity in
whose declaration the name is written, the region is the part of that
declaration, and the ordinal is the entry's rank among the `DEPENDENT`
entries of that (owner, region) in parse order.

**The owner.** At the pin `WrittenRef::enclosing`
(`kiln/spec/ast.md` §18.3) is not the owner. Slip's design review
dumped it:

| Name in | `enclosing` at the pin |
|---|---|
| a default template argument of a namespace-scope template | the namespace, or the translation unit |
| a default template argument of a member template | the class |
| a requires-clause in the template head | the namespace |
| an alias template's type-id, directly (`using R = decltype(T::s())`) | the namespace |
| the type of a non-type template parameter | the namespace |
| a lambda in a default template argument | the closure's call operator |
| a partial specialisation's arguments; a concept's body; a trailing requires-clause | the declaration (right) |
| a member's default argument; a parameter of a function type in an alias | the parameter |

Kiln's WP-3143 fixes the first five (§2.1 item 4). Slip takes it with
its next pin move; no stage that names an entry is built before. Of
the residual cases:

| Residual | For Slip |
|---|---|
| a generic lambda's head held by the enclosing variable | does not matter now: the call operator is local, and Slip leaves its keys out and counts them |
| a template template parameter's inner head held by the outer template | right as it is: the outer template is the owner |
| a name in a function-type parameter, or in a lambda, inside an alias's type-id, held by that parameter or call operator | **matters** for `Alias` rows and for `SUBST_EXPANDED` rows: the owner is the alias template. Asked: the owner for these, in `enclosing` or by an accessor |

**The region** (new; it does not exist). Needed, for every `DEPENDENT`
entry: which part of the owner's declaration the name is in. Slip's
list, for Kiln to close: the default argument of template parameter
*i*; the type or type-constraint of template parameter *i*; the head's
requires-clause; the signature; the trailing requires-clause; the
exception specification; the explicit-specifier; the default argument
of parameter *i*; the body or initialiser; an alias's type-id; a
concept's expression; a partial specialisation's arguments; a class's
base list; an attribute or annotation. Every entry has exactly one.
The region must be total over every place whose substitution
`written_spec_key` gives a key for.

Why the region: the set of `DEPENDENT` entries of an owner is not a
function of the owner's text. In the review's dump `c.h(t)` and
`q::k(t)` are dependent entries when `h` and `k` have two overloads and
resolved at the definition when they have one. So an edit elsewhere
renumbers the entries after such a name; the region confines that to
one part of the declaration. The region also says which substitution a
`specs()` row belongs to (the signature's at deduction, the body's at
definition), which the first draft asked as a "part" per row.

A question that goes with it: a signature and a requires-clause are
written once per redeclaration. Which redeclaration's entries does a
substitution of them read?

Acceptance: over Kiln's written-reference corpus, every entry that has
a row in `specs()` or `subst_rows` has an owner that is never a
namespace, never the translation unit, and never the class for a name
in the head of a member template; and a region.

**If Slip's digest reads roots for all four kinds (§5.3, its
recommendation), it needs neither the region nor the alias residual.**
The edit planning needs neither. So the region is asked only if the
ruling on the form leaves a kind on rows; Kiln's cost for it is one of
the inputs of that ruling.

### 5.2 `specs()` rows after stages 2 to 4

At the pin `specs()` keys an alias's rows by the alias template, a
concept's by the concept, and a requires-clause's by the templated
declaration. Slip's digest never reads a row whose `spec` is a pattern:
one set per pattern over the whole translation unit changes with every
added use.

Asked:

- **What happens to those rows** once `Alias`, `Satisfaction` and
  `DefaultArg` are keyed in `subst_keys`: do they stay in `specs()`
  (the table "is unchanged"), and are they the same rows?
- **A mark** on a `specs()` row whose substitution is also keyed in
  `subst_keys` (a flag, or a value of the region), so that a reader
  can skip it.

Slip's digest reads `specs()` only for a `spec` that is a
specialisation or a member of one, and only where the AST does not
hold the substituted operand (§5.8); in the outcome where Kiln gives
roots it reads `specs()` not at all.

### 5.3 A root per key; selections that have no written name

**The need.** A row needs an entry, and an entry needs a written name.
What a substitution selects without one has no row: the function of an
operator expression, a constructor, a conversion function, the
`begin`, `end` or `get` of a range-for or a structured binding.

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
it is found by ordinary lookup, `shl<a::X>::v` is 1 both times, and no
row can say it. This is the form of every "is streamable", "is equality
comparable" and "is callable with" trait, and operator lookup is the
lookup a namespace migration moves most easily. **Ruled: operators are
in Slip's promise**, ahead of `Alias` in order of need.

**The first form asked: a root per key.** Per key, the root of what
the substituter built (an expression, a type, or an argument list).
The nodes exist and are not freed. Slip's digest then reads the root
with the walk it uses for an instantiated body: every name, operator,
constructor and conversion is in it, and no entry needs a name. Rows
stay what the edit planning reads.

Kiln has costed it (§2.1 item 3) and proposes `subst_roots`, a
parallel table behind its own switch. **Asked, per kind:**

| Kind | Kiln's study | Slip asks |
|---|---|---|
| `Alias` | complete, kept alive, no type locations | the root. One thing a root without type locations lacks: the written type of a parameter of a function type whose type is adjusted (`using F = void (*)(int p[sizeof(T::s())])`): the bound's operand is only in the parameter's declaration. Can the root keep it? |
| `DefaultArg` | complete; before conversion | the root **after** the site's conversion and before evaluation (G5). The conversion can select a function: a converting constructor or a conversion function for a parameter of class type, a conversion function to a scalar parameter type (`template <class T, int = T()>`). Before conversion that selection is in neither the root nor a row |
| `PartialMatch` | built then dropped; intern the list | the root; a slot for a bare non-type parameter may hold its value only |
| `Satisfaction` | not complete: three pieces | two of them: the substituted requirements kept (G6; §5.4 item 6 needs it for instantiated bodies in any case), and the atom cache's entry holding its expression (G4). Not the third: a failed atom needs no root, only the fact that it failed. What its names bound before the failing one is in rows, which a digest on roots does not read: a limit Slip states |
| the kinds of §5.4 | — | as `DefaultArg` |

**Ruled (Slip's supervisor, 2026-11-24): Slip's digest reads roots
for all four kinds.** Concepts are where operators are, so
`Satisfaction` on rows alone would leave the lookups a migration moves
unseen. **What do the two pieces for `Satisfaction` cost, and can
`subst_roots` come per kind with the stage that gives the kind's keys
(`PartialMatch` with stage 1, and so on) and not later?** Slip's stage
for a kind starts when the pin has both the kind's keys and its
roots.

The guarantees Slip needs of a root:

| | Guarantee |
|---|---|
| G1 complete | every node reachable from a root has every field a reader of an instantiated body reads (types, value categories, `satisfied`, stored values). No node kind exists only under a substitution |
| G2 resolved | every name, operator, constructor, conversion and implicit step the substitution selected is in the nodes as in an instantiated body. No unresolved lookup remains for a call whose arguments are no longer dependent |
| G3 nothing dangles | every declaration a node refers to is valid and reachable: none belongs to a dropped frame (a parameter of a function type built inside the substitution, a closure type, an instantiated local) |
| G4 one root per key | P1 for roots: the first completed run's root stands; a key whose atoms are replayed from the atomic-constraint cache has roots equal to a fresh substitution's (sharing the cached nodes serves) |
| G5 after conversion, before the value | where only a value is kept (a converted non-type default argument, an evaluated atom), the root is the expression after the site's implicit conversion and before evaluation |
| G6 requires-expressions | a requires-expression in a root keeps its substituted requirements. At the pin one whose result is not dependent is rebuilt with an empty requirement list (`kiln/crates/kiln_sema/src/templ/concepts/requires.rs`) |
| G7 the marker | an argument's replacement in a root is under a `SubstTemplateTypeParm` or its expression counterpart, so that a reader can leave out how the first use spelled its arguments. The alias cache is keyed by sugared arguments at compatibility level 22 and above (`kiln/crates/kiln_sema/src/templ/class.rs` `alias_template_type`) |
| G8 the key's condition | a pattern whose only dependent selection is an operator still gets a key (§5.5) |

The shape of a root Slip would read, per kind: `PartialMatch`, one
slot per written argument of the partial specialisation;
`Satisfaction`, one slot per atomic constraint of the normal form, in
its order, empty for an atom not substituted or failed; `DefaultArg`
and `Alias`, one. The slot's place matters: it keeps "bound in the
first atom" apart from "bound in the second". Asked (sent inline):
is one slot per atom of the normal form a shape Kiln can give, also
for a concept-id's own check over the concept's normal form?

**The second form, if a root is not possible for a kind**: rows whose
entry is a pattern node and not a written-reference entry
(`PatternNode::Expr` exists, `kiln/crates/kiln_ast/src/written_refs.rs`),
for, in this order: (a) operator expressions (unary, binary, compound
assignment, subscript, call, arrow, a fold's operator, a rewritten
comparison), each with the function selected or the fact that a builtin
operator was; (b) constructors and conversion functions; (c) the rest.
The substituter rebuilds these through `act_on_*` with
`oper_rebuild_set` (`kiln/crates/kiln_sema/src/overload/oper.rs`). Slip
would then need a way to name such a node as it names an entry (§5.1).

**The builtin traits** that run overload resolution themselves
(`__is_constructible`, `__is_assignable`, `__is_convertible`,
`__is_invocable`, `__is_trivially_equality_comparable`, the
`__builtin_*_synthesizes_from_spaceship` family;
`kiln/crates/kiln_sema/src/templ/builtins/traits.rs` and
`traits/special.rs`) are the same class: only the trait's value is
stored. They come after the operators. Not probed. Asked (sent
inline): can what they selected be recorded at all? A root has only
the trait's node.

### 5.4 Shapes with no key and no row

Each of items 1 to 5 was read in Kiln's source at the pin.

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
   (`deduce_call.rs`), so its rows are keyed by `ft` and dropped. Only
   the bit is kept. Needed: the substituted operand kept with the
   specialisation (as `ColdDeclData::explicit_expr` holds it for a
   member of an instantiated class), or rows under the specialisation.
3. **Class template argument deduction.** The alias guide
   (`build_alias_guide`, `kiln/crates/kiln_sema/src/templ/ctad.rs`) is
   built with no frame; the default arguments of an inherited
   constructor's guide are substituted under
   `in_sfinae(DeducedArgSubstitution, c)`
   (`kiln/crates/kiln_sema/src/templ/ctad/inherit.rs`) and their rows
   dropped. Needed: Kiln's word on which of these bind a name of a
   pattern, and a record for those.
4. **Template template argument matching** (`PartialOrderingTtp`,
   `kiln/crates/kiln_sema/src/templ/ttarg22.rs`): the non-type
   parameter types of both lists are substituted and compared. No key.
   Needed: as item 1, at low priority.
5. **Variable template partial specialisations.** `get_var_spec`
   (`kiln/crates/kiln_sema/src/templ/var.rs`) matches through
   `match_partial_args`. Needed: a sentence that `PartialMatch` covers
   them.
6. **A requires-expression in an instantiated body.** Its substituted
   requirements are not kept (G6). Needed: kept, in the instantiated
   body as in a root.
7. **Two shapes with no `specs()` row**, met by the second review of
   Slip's WP-139 (the rename reads `specs()` to decide whether a
   dependent name binds its subject; with no row the name stays
   `unresolved` and the rename leaves it). The need here is the edit
   planning's, not the digest's.
   - A dependent name in **a partial specialisation's argument list**
     (`template <class T> struct PS<T, sizeof(T::s())>`, used as
     `PS<n::S<int>, 4>`): no row at the pin. Kiln's stage 1 gives its
     rows under the `PartialMatch` key; nothing more is asked.
   - A dependent name in **a deduction guide**
     (`template <class T> DG(T, Arr<sizeof(T::s())>) -> DG<T>;`). The
     review's probe did not use the guide, so it shows nothing about
     Kiln. **A question**: when class template argument deduction
     substitutes a guide, are its names' rows recorded, and under what
     key? It goes with item 3.

**Questions** (frames of `kiln/crates/kiln_sema/src/templ/stack.rs`
that Slip did not read further): does the substitution bind a name of
a pattern, and is the result kept?

- `Annotation`: an annotation's operand;
- `ExplicitArgSubstitution`: explicit template arguments substituted
  into a signature before deduction;
- `MetaSubstitution`: `std::meta::substitute`;
- `Enum`: the enumerators of a member enumeration completed on demand.

### 5.5 A key for every completed substitution

Granted by Kiln's answer of 2026-11-24 (§2.1 item 1). The reason is
recorded here because it is why answer 1 was withdrawn.

In Slip's comparison a derived unit that only one parse has is counted
and not reported (`spec/verify.md` §4 step 3), because a transform may
change what is instantiated. So "a key appears or disappears" is no
difference. A substitution that completes in both parses, with a name
bound before and none after, must have a key in both:

```cpp
namespace n { struct S { using a = int; }; }
template <class T> concept E = requires { typename T::a; } || true;
static_assert(E<n::S>);
```

Remove `a`: the check completes and is satisfied both times; the first
time it has a row, the second time none.

Slip writes the key's outcome (`SubstOutcome`) in the unit it makes of
the key, so that "satisfied before, unsatisfied after, the same names
bound" is a difference.

Left of it:

- **The condition** (a question). Kiln's test for "can have a key" is
  static: the pattern has a dependent entry. An operator has no entry.
  Is `val<T>() << val<T>()` (§5.3) a pattern with a dependent entry?
  If not, the condition Slip needs is "the pattern is
  instantiation-dependent", which is as static.
- **A count**, on `<bits/stdc++.h>`: the substitutions of the four
  kinds that **failed**. They stay without a key; the number is the
  size of a limit Slip states.

### 5.6 The element of a pack expansion

`push_spec` (`kiln/crates/kiln_ast/src/written_refs.rs`) keeps one row
per distinct resolution of an entry under a key, with no element
index. Two elements whose bindings swap give the same rows. Slip
states this as a limit of the row form. A root does not have it,
provided a pack expansion in a root holds each element's substituted
form in order (sent inline, as part of G1).

### 5.7 Readable omissions

Slip's reading, to be confirmed: `subst_keys_complete()` is false
exactly when at least one `Unrecorded` value exists; `PrefixKeys` and
`ModuleKeys` name every file whose substitutions may be missing;
`ModuleKeysLeftOut` names the module. Slip writes one indeterminate
unit per named file or module, and one for an image that has no key
table at all, so a comparison is never equal by silence (the
proposal's §6). Slip makes its own prefix and module images with the
switch on.

Asked (sent inline): the same for `subst_roots`. Is a root left out of
a module image, or a prefix made without the roots' switch, readable
per key or per file?

### 5.8 Operands a type or a value holds

Slip's digest writes canonical types and values. An operand that only
a type or a value holds is read from where the AST keeps it, in
ordinary code (`decltype(make(x)) y;`, `array<char, sizeof(f(x))>`) and
in instantiated declarations (`-> decltype(T::s())` in a
specialisation). Read at the pin
(`kiln/crates/kiln_ast/src/types/context.rs`, `node.rs`, `args.rs`,
`func.rs`; `kiln/crates/kiln_ast/src/decl.rs`):
`TypeView::Decltype { expr }`, `TypeView::TypeOf { operand }`,
`ConstArray::size_expr`, `ArgView::Expr` in a written argument list,
`TypeView::PackIndexing { index }`, `ExcSpec::NoexceptExpr { expr }`,
`ColdDeclData::explicit_expr`.

Questions:

1. Does a declaration made by instantiation store its type with these
   nodes, built from the **substituted** operand, in every case: a
   function specialisation's return and parameter types (also of a
   candidate that lost), a field, a local variable, a variable
   template's specialisation? `decltype_ty` and the constant-array arm
   (`kiln/crates/kiln_sema/src/templ/subst/types.rs`) were read; what
   the declaration keeps was not probed. Slip knows one exception: an
   instantiation-dependent constant bound is built without its
   `size_expr` (`spec/digest-kinds.md` §4.4 rule 7).
2. Does a template-id that is not dependent keep its expression
   argument in the written argument list, or only the converted value?
3. Where else is an expression written and only its value kept? Slip's
   guesses: a bit-field's width, an enumerator's value, `alignas(e)`,
   an attribute's argument.

Where the answer to 1 or 2 is no: the operand kept (implementation).

### 5.9 "Emitted whether or not referenced"

Slip's WP-155 reports a derived unit that is emitted whether or not it
is referenced when only one parse has it. Kiln's rule is
`var_must_emit`, `is_ctor_dtor_fn`
(`kiln/crates/kiln_codegen/src/global_init.rs`) and `USED`. Asked: a
predicate on the AST, in `kiln_ast` or `kiln_sema`, that says of a
declaration what code generation will decide, so that Slip does not
rebuild it from attributes, the initialiser's value and the
destructor. Slip builds its own first and replaces it.

## 6. Still open between Slip and Kiln

1. **The form** (§5.3), ruled as roots for all four kinds: the cost
   and the time of the two pieces for `Satisfaction`; the root after
   conversion for `DefaultArg`; the adjusted parameter in an `Alias`
   root; `subst_roots` per kind with the kind's stage; the guarantees
   G1 to G8.
2. §5.1: not asked now. The region, the alias residual and
   redeclarations are needed only if a kind falls back to rows; Kiln's
   rough cost for the region is welcome as an input should that
   happen.
3. §5.5: the condition for a key; the count of failures.
4. §4, P2: which substitutions give "stays dependent" rows, and the
   cost of all.
5. §5.4: the kind for a parameter's type; the four questions.
6. §5.8: the three questions.
7. §9: how a concept named in a type-constraint is keyed.

## 7. What is not asked

- **A record of a failed substitution**, in either form. A
  substitution that failed decided nothing through its names: the
  candidate is gone, the partial specialisation is not matched, and
  that outcome is compared. What a name bound before the failure is a
  limit Slip states; what that binding instantiated is behaviour, and
  Slip's WP-155 reports it. A nested substitution that succeeded inside
  a failed one keeps its key (answer 3). A check that completed
  unsatisfied is not a failed substitution and has its key (§2.1).
- **A `selected` mark** on a `PartialMatch` key.
- **Keys with dependent arguments** (answer 6).
- **A change to `specs()`'s keys.** Its rows stay keyed by `DeclId`.
- **"Stays dependent" rows for every member template of every
  instantiated class**, until §4's cost question is answered.

## 8. What Slip does until then

The limit is stated in `spec/digest.md` §2.3 and
`spec/digest-kinds.md` "Hazards accepted for now". Slip's WP-155 lands
first and needs nothing of Kiln. The stage that reads operands from
the AST (§5.8) needs only the answers to its questions. The other
stages follow Kiln's, each when its keys are in the pin (the
proposal's §10).

## 9. Acceptance examples

For each, the key expected and what is recorded under it: a row for
the named entry, or a root in which the name is resolved. `pre.h` is

```cpp
struct Base { static int s(); };
namespace n { struct S : Base { static int s(); }; }
```

| Source after `#include "pre.h"` | Key | Recorded |
|---|---|---|
| `template <class T, int = sizeof(T::s())> int call() { return 0; }` `int u() { return call<n::S>(); }` | `DefaultArg` (`call`, parameter 1, `<n::S>`) | `s` → `n::S::s` |
| `template <class T> using R = decltype(T::s()); R<n::S> v;` | `Alias` (`R`, `<n::S>`) | `s` → `n::S::s` |
| `template <class T> concept HasS = requires { T::s(); };` `template <HasS T> int g() { return 0; }` `int u() { return g<n::S>(); }` | **a question**: `Satisfaction` (`HasS`, `<n::S>`), `Satisfaction` (`g`, `<n::S>`), or both? Under normalisation the type-constraint is `g`'s atom, and rule 4 replays atoms "into the open key" | `s` → `n::S::s` under whichever key |
| `template <class T> int g() requires requires { T::s(); } { return 0; }` `int u() { return g<n::S>(); }` | `Satisfaction` (`g`, `<n::S>`) | `s` → `n::S::s` |

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

Key `PartialMatch` (the partial specialisation, `<a::X, void>`);
recorded: `swap` → `a::swap(X &, X &)`, with its `adl` set in a row.
Whether `ref` (a template name whose arguments are dependent) is a
`DEPENDENT` entry with a row is Kiln's rule. The root here is
`void_t<decltype(swap(...))>`, a transparent alias: Slip needs the
written argument of the alias specialisation, with its `decltype`
operand, reachable from the root, not only the canonical `void` (sent
inline). The operator form of §5.3 (`shl<a::X>`) is an acceptance
example too: a `PartialMatch` key whose root holds the `operator<<`
selected.

Order independence (P1): each example with the use written twice, with
the second use first, with the argument spelled through an alias
(`using N = n::S;`), and with the first use inside an instantiation
that a later declaration forces, gives the same keys and the same
content.

## 10. Changed after the design review

By the review's list "KR9 before it is sent" and the rulings:

1. §3 answer 1 withdrawn; the key for every completed substitution and
   the unsatisfied check asked (§5.5); the count of failures kept.
2. §5.1 is needed by every kind on rows, not by `Alias` alone; the
   pin's `enclosing` given as a table; §2 item 9 corrected against the
   pin; owner and region asked together; the acceptance tightened.
3. §5.2: the per-row "part" is replaced by the entry's region; asked
   what becomes of today's `specs()` rows keyed by a pattern, and a
   mark for rows keyed in `subst_keys`.
4. §5.3 and §7: a root per key is the first form, asked as a cost
   question for every kind; the argument that it carries the first
   use's sugar is dropped (it is guarantee G7 instead).
5. §5.4: `Annotation`, `ExplicitArgSubstitution`, `MetaSubstitution`,
   `Enum` added as questions; the requires-expression's requirements
   added.
6. §4 P2: which substitutions give "stays dependent" rows, and the
   cost, asked before all are.
7. §9: the row for a type-constraint is a question.
8. §5.9 new: the predicate "emitted whether or not referenced".

Also: answer 6 recorded as ruled; operators moved ahead of `Alias` in
the order of need; §5.8 new (operands in the AST).

After Kiln's answer of 2026-11-24 (§2.1): the key for every completed
substitution is recorded as granted and §5.5 reduced to its condition
and one count; P2 restated as Kiln's row rule; §5.1 reduced to the
region, the alias residual and the acceptance; §5.3 asks per kind, on
Kiln's cost study.
