# 03 — Equivalence: proving everything resolves as before

## 1. The claim to be checked

For a transformation declared meaning-preserving up to a relocation map `M`
(old entity → new entity), and for each observer:

> Every reference in the pre-image, written or implicit, has a counterpart in
> the post-image that resolves to the same entity mapped through `M`; there
> are no other references; and every type, constant value and conversion is
> the same up to `M`.

This is a per-reference property. It is not "the code still compiles" (a call
that silently binds to a different overload compiles) and not "the same names
appear" (ADL makes the name a poor witness of the callee). Slip checks it
without tracking references individually: it hashes Kiln's resolved AST before
and after and compares.

The checker is independent of how the edits were produced. A transform, a
native refactoring, the spelling constructor, the macro logic and the reducer
can each be wrong; the checker catches the result. This is the part of Slip
that has to be right, and so it is the part that is specified first, kept
small, and attacked with mutation tests.

## 2. Entity keys

An **entity key** identifies an entity independently of TU, run and source
position. It is structural, not a string:

```
key      := kind, scope, name, [signature], [template-arguments]
scope    := key of the enclosing entity
            | anonymous-namespace marker | TU-local marker (for internal linkage)
            | position: (key of enclosing named entity, kind, ordinal)   -- lambdas, local classes,
                                                                            unnamed types, block scope
signature:= parameter type keys, cv/ref qualifiers, template head, constraints, return type
```

- Keys exist for everything a reference can denote, not only things with
  linkage: namespaces, aliases, enumerators, concepts, template parameters,
  using-declarations, local variables. This is why mangled names are not used.
- Unnamed and local entities are keyed by ordinal position inside their
  nearest named ancestor, never by byte offset, because offsets move under
  edits.
- Inline namespaces are part of the scope with a marker, so moving something
  into or out of one is visible.
- `M` is applied structurally: a key maps if it or any key it contains (scope,
  parameter types, template arguments) maps. `legacy::f(legacy::A)` relocates
  to `acme::f(acme::A)`, and to `acme::f(legacy::A)` if `A` stays.

Keys are also what facts are indexed by across TUs, and what `slip::key_of`
returns.

## 3. The semantic digest

For each declaration, Slip records two things:

- a **shape hash**: a Merkle hash over the declaration's resolved AST in which
  each referenced entity is replaced by its index in order of first
  occurrence; and
- the **key list**: the distinct entity keys it references, in that order.

Two declarations are equivalent up to `M` when their shape hashes are equal
and their key lists are equal element by element after mapping through `M`.
Separating shape from keys is what lets pre-digests be computed during map,
before `M` is known (which entities move is settled in reduce), with no
re-parse of the pre-image; and when lists differ, the differing element names
the entity that changed.

What the shape hash covers:

| Included | Why |
|---|---|
| Every resolved name: variables, functions, types, members, enumerators, namespaces in qualifiers, templates | The core claim |
| The selected function of every call, including overloaded operators, conversion functions, constructors, destructors, `operator new`/`delete`, range-for `begin`/`end`, structured-binding `get`, coroutine customisation points | ADL and overload resolution live here |
| Implicit conversion sequences and value categories | A different sequence is a different program |
| Default arguments used, default member initialisers, implicitly defined special members and what they call | Not written, still resolved |
| Types of every expression and declaration, as keys | Covers `decltype`, `auto`, deduction, CTAD |
| Constant values: array bounds, enumerator values, template arguments, `static_assert` and `requires` results, `sizeof`/`alignof`, `noexcept` | Covers SFINAE, concepts and traits through their outcomes |
| Each template specialisation instantiated in the TU: its key, and the digest of its instantiated definition | Dependent names resolve at instantiation; this is where shared-header disagreement shows |
| Attributes, linkage, storage, deleted/defaulted, virtual overriding relations, base classes, access | |

| Excluded | Why |
|---|---|
| Source locations, file names, line numbers | Edits move them |
| How a name was spelled: qualified or not, through which alias or using-declaration | Requalification is the point; an alias and its target are the same entity |
| Redundant parentheses, comments, whitespace, macro structure | Not semantic. (`decltype((x))` is caught through the type.) |

**The standard for completeness is code generation.** Hod generates code from
this same AST, so anything that can change the emitted program is in the AST;
the digest's specification is "every field code generation or constant
evaluation reads, with locations removed". Equal digests up to `M` then mean
equal generated code up to symbol names. The spec enumerates AST node kinds
and fields with the included/excluded decision for each, and a test fails when
Kiln adds a field the digest spec does not classify.

**Values derived from names and positions.** `__PRETTY_FUNCTION__`,
`std::source_location`, `typeid(T).name()`, `__LINE__`, `__FILE__` and
`__COUNTER__` yield values that legitimately change. They enter the digest
symbolically ("the name of entity K", "the line of this token"). If such a
value flows somewhere it changes a type or a constant (an array bound from
`__LINE__`), the digest differs and Slip reports it, because that is a real
change.

**Whole-TU comparison.** The set of declared entity keys must match under `M`
(nothing appears or disappears), and each entity's digest must match. New
diagnostics in the post-image are failures.

## 4. Declared changes

| Declaration by the transform | Effect on verification |
|---|---|
| none | Everything must match exactly |
| `relocate(entity, new_name)` | Adds a pair to `M` |
| `expect_changed(declaration, why)` | That declaration's digest is exempt; the edit's own obligations are checked instead (for example: the callee at this site resolves to key K). Every other declaration must still match, including callers, overriders and instantiations. |
| `create_file`, inserted declarations | New entities are permitted where declared; they must not change the resolution of anything existing |
| removals | Declared removed entities must have no remaining references |

Verification of intentionally behaviour-changing transforms is therefore
weaker inside the changed declarations and full strength everywhere else:
"I changed these functions, prove nothing else moved" is checkable.

## 5. Spelling a name at a destination

`slip::spell(entity)` asks Slip to produce the name. The constructor works on
the pre-image model plus the pending relocations:

1. Enumerate candidate spellings from shortest to fully qualified
   (`T`, `x::T`, `acme::x::T`, `::acme::x::T`).
2. For each, simulate lookup from the destination scope conservatively using
   the declaration tables: accept the candidate only if lookup provably finds
   the target and nothing else. "Conservatively" means that anything not
   modelled exactly (a dependent base, a using-directive in a scope not fully
   known, a later redeclaration) causes the candidate to be skipped.
3. Each observer reports its shortest accepted candidate; reduce takes the
   longest across observers ([02](02-pipeline.md) §3).

The constructor is a heuristic for producing good-looking code. It is not
trusted: the verifier proves the chosen spelling resolves to the target in
every observer, and on failure repair escalates that site to the next longer
candidate. A policy option skips the search and always uses a fixed style.

## 6. Localising a mismatch, and repair

A mismatching declaration gives the entity key that differs. To turn that into
a source location, the worker walks the pre- and post-image ASTs of that
declaration in parallel (the pre-image is re-parsed for this; mismatches are
rare) and reports the first differing node with its written reference:

```
src/io/Writer.cpp:412:9  call `flush(buf)`
  before: legacy::io::flush(legacy::Buffer&)     [found by ADL through legacy::Buffer]
  after:  dep::flush(dep::Handle)                [found by unqualified lookup]
  observer: src/io/Writer.cpp [release-linux]
```

The repair hook receives exactly this. The stock repair for meaning-preserving
transforms is "spell the original referent explicitly at this site", subject
to the rules in §7 for the cases where qualifying is itself a change.

## 7. The motivating case: migrating a root namespace

Setup. Owned code and its dependencies both declare things in `namespace
legacy`. Owned code moves to `acme`. Dependencies are read-only and keep
`legacy`.

**What moves.** An entity moves if and only if every declaration of it, across
all observers, is in owned code. That is a folded fact
([01](01-transform-api.md) §7). The namespace `legacy` itself does not move:
it continues to exist. Consequences that a rename-the-token approach gets
wrong:

- A `namespace legacy { … }` block in owned code may contain members that
  stay: explicit specialisations of dependency templates, forward declarations
  of dependency classes, overloads that must remain findable by dependency
  code. Such a block is split (`} namespace legacy { … } namespace acme {`),
  not renamed.
- An owned entity that a dependency also declares (a forward declaration in a
  dependency header) does not move, and is reported.
- An owned entity that read-only code *uses* by its old name needs a
  compatibility declaration left in `legacy` (`using acme::T;`). Whether that
  is acceptable is policy; it is not neutral, since `T`'s associated namespace
  for ADL becomes `acme`. Verification of the dependency headers, which every
  observer parses, says whether it held.

**Hazards and what catches them.**

| # | Hazard | Proposed by | Proved by |
|---|---|---|---|
| 1 | Unqualified use inside moved code of a dependency entity in `legacy`, previously found through the enclosing namespace | Qualify (`legacy::Dep`) | Digest |
| 2 | Qualified use `legacy::x::T` of a moved entity | Respell qualifier | Digest |
| 3 | Unqualified call that found a *moved* function by ADL through a *staying* argument type | Qualify the callee; if the call relied on ADL to choose among namespaces, report | Digest: selected function per call |
| 4 | Unqualified call that found a *staying* function by ADL through a *moved* type | Qualify the callee | Digest |
| 5 | Operators found by ADL (`a == b`, `os << x`) where operand type and operator part ways | Cannot be qualified without changing syntax. Add a using-declaration for the operator in the destination namespace, or report | Digest |
| 6 | Customisation points called from dependency templates (`swap`, `begin`, `hash_value`, user hooks): the call is in read-only code, the lookup happens at instantiation | Nothing to edit at the call. Keep the hook findable (leave it in `legacy`, or add an overload) or report | Digest of the instantiation |
| 7 | Explicit or partial specialisations of dependency templates written in owned `namespace legacy` blocks | Stay; split the block | Parse + digest |
| 8 | `using namespace legacy;`, namespace aliases, using-declarations naming moved entities | Add/replace the directive for the new namespace | Digest |
| 9 | Name collisions and new ambiguity in `acme`; a moved name hiding something at a use site | Pre-check from folded facts (names declared in both); otherwise repair by qualifying | Parse + digest |
| 10 | Friend declarations and elaborated type specifiers that declare into the nearest enclosing namespace | As for declarations | Entity set |
| 11 | Inline namespaces, anonymous namespaces, `extern "C"` entities inside `legacy` | Keys carry the markers; `extern "C"` entities are one entity across namespaces | Keys |
| 12 | Macro bodies that spell `legacy::`, defined in owned code | Edit the definition if all expansions agree ([04](04-macros-and-configurations.md)) | Unanimity + digest |
| 13 | Macro bodies that spell `legacy::`, defined in dependency code, expanded on moved entities | Cannot edit. Report; needs a compatibility declaration | Digest |
| 14 | Names in strings, stringised macro arguments, `typeid().name()`, `__PRETTY_FUNCTION__`, serialisation tags, `dlsym` | Out of semantic scope. Symbolic in the digest; listed in the report. Mangled names change by design | Report |
| 15 | Comments, documentation, build files, other languages | Optional textual pass, reported separately, never mixed with verified edits | — |
| 16 | Code in inactive `#if` branches, uninstantiated templates | Other configurations as observers; otherwise uncovered | Coverage report |

The column that matters is the last. The "proposed by" logic is library code
at L2 that will be incomplete on first contact with a real codebase; every
incompleteness appears as a located mismatch, not as a silent miscompile, and
the catalogue (kept in the spec with one test per row, each including a
deliberately wrong edit that must be caught) grows from those reports.

**How a migration actually runs.** The first run on a new codebase ends "not
verified" with a report grouped by cause. The operator resolves classes of
problems by policy (allow compatibility using-declarations; leave these hooks
in `legacy`; exclude this directory) and reruns; unchanged observers come from
the shard cache. The run that ends "verified" is the one whose patch is
applied.

## 8. Checks that do not depend on Kiln

Pre- and post-image are analysed by the same front end, so a Kiln bug that
misresolves a name the same way twice is invisible to the digest. Independent
tiers, run by Slip's harness on its corpora and available to users:

| Tier | Check | Catches |
|---|---|---|
| T1 | Semantic digest (always) | Everything in §3, at source precision |
| T2 | Production compiler accepts the post-image (`-fsyntax-only`) | Kiln accepting invalid code (Kiln currently misses about 12% of clang's expected-error lines on its conformance corpus) |
| T3 | Symbol sets of the built objects, before vs after, demangled and mapped through `M` | Wrong callee anywhere code is generated; independent of Kiln |
| T4 | Object code equal up to symbol names; the project's own tests | Everything else |

## 9. Limits, stated plainly

- The guarantee is per analysed observer. Unanalysed configurations and
  uninstantiated templates are enumerated, not covered.
- The digest is as complete as its specification. The specification is tied to
  Kiln's AST field by field and mutation-tested, and T3/T4 exist because that
  can still miss something.
- A project whose behaviour depends on the textual form of names (§7 row 14)
  changes behaviour under any rename. Slip reports where; it cannot decide.
