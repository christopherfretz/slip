# 01 — The transform API

## 1. The decision: intrinsic header or the C++29 code-generation track?

**Recommendation: a Slip-owned header over `__slip_*` intrinsics, borrowing the
standard track's vocabulary and value model, not its effect model.**

| Question | Standard code-generation track (token injection, fragments) | Slip needs |
|---|---|---|
| Where does code land? | In the TU being compiled, at the point of evaluation | Arbitrary byte ranges of existing files, including headers shared by thousands of TUs |
| Does the program see the change? | Yes; that is the purpose | No; the analysed program must stay frozen so every TU observes the same input |
| What can be inspected? | Declarations and types (C++26); expression reflection is an early-stage paper | Use sites, as-written qualifiers, how lookup found a name, macro expansions, source ranges, templates as written |
| Status | Not in C++26; live papers are at R0/R1, with name collisions between them, and their own authors doubt C++29 | Needed now, stable for years |

No existing language ecosystem uses its generation channel for editing: Rust
has proc-macros *and* compiler suggestions applied by `rustfix`; Roslyn has
additive-only source generators *and* code fixes; Swift forbids macros from
changing the declaration they attach to. Editing is a different channel
everywhere, and its payload is always (range, replacement, metadata).

What Slip does take from the standard track:

- **One opaque handle type plus free functions**, the `std::meta::info`
  design. The rationale (language vocabulary changes; value semantics; scalars
  are cheap in a constant evaluator; heterogeneous collections) applies to
  Slip with more force, because Slip models the language *and* the
  preprocessor.
- **Names.** Where a `std::meta` function or an in-flight proposal has the
  meaning Slip needs, Slip uses the same name in `namespace slip`
  (`identifier_of`, `parent_of`, `members_of`, `callee_of`, `source_text_of`).
  Names from papers that have not survived a committee poll are provisional
  and listed as such in the spec.
- **Code as a value, with interpolation.** Replacement text is a `slip::code`
  value built from literal text, *entities* (spelled by Slip at the
  destination) and *verbatim source ranges*. This is the token-sequence idea
  with the one change refactoring forces: whitespace and comments are
  significant.
- **Errors** as `std::meta::exception`-style exceptions (constexpr exceptions
  are C++26 and Kiln implements them).

## 2. Layers

| Layer | What | Stability |
|---|---|---|
| L0 | `<slip/__intrinsics.h>` | Private. One generic host-call primitive (see §8). A host accepts only the copy of the header it was built with. May change in any release. |
| L1 | `<slip/meta.h>`: `namespace slip { inline namespace v0 { … } }` (`v0` while the contract is a draft; `spec/api.md` §8.1) | **The stable contract.** Opaque values and free functions; option structs extended by adding members; open enumerations; feature-test macros `__slip_lib_*`. Specified in `spec/api.md` with behavioural tests per function. |
| L2 | `<slip/rules.h>`, `<slip/refactor.h>`, `<slip/view.h>` | Ordinary constexpr C++ written only against L1: rules, typed views, reusable refactorings (rename, relocate namespace, change signature). Versioned as a library; forkable. |
| L3 | User transforms | Source files. Compose by `#include`. |

Transforms are distributed as source and compiled by the Slip that ships the
header, so there is no binary interface to keep stable (contrast Rust
proc-macro dylibs and Swift macro plugins). A transform states the API level
it needs (`#if __slip_lib_meta < 202611L`), and L1 never removes a name within
a major version.

**Version 0** of L1 is specified in `spec/api.md` and is a deliberate
subset of what this document describes. Sections 4 to 9 use version 0's
names and forms wherever version 0 has fixed them, and mark what lies
beyond it as *later*; the names of later features are reserved
(`spec/api.md` §14) and their design here is illustrative.

## 3. Two universes

```
   SUBJECT universe (per TU, per configuration)        TRANSFORM universe (once per worker)
   parsed as its build compiles it:                    C++26, Slip's pinned library headers
   own -std=, -D, -I, standard library                 <slip/meta.h>, L2, the user's transform
   ───────────────────────────────────────            ──────────────────────────────────────
   Kiln front end ──▶ subject model  ◀──── host calls ────  constexpr evaluation (the Oven)
                      (AST, references,                      slip::info handles are integers
                       macros, ranges)   ───── results ───▶  that mean something only to the host
```

- The subject is never modified by the transform and never sees it. Its
  semantic digest ([03](03-equivalence.md)) is computed before the transform
  runs.
- The transform is compiled once per worker and evaluated once per observer
  (or less, see memoisation in [02](02-pipeline.md) §6). A transform cannot
  carry state from one TU to the next: constant evaluation cannot mutate
  globals, and handles are invalid outside the evaluation that produced them.
- A transform is deterministic and sandboxed by construction: no files, clock,
  environment or network. Its inputs are host calls, run parameters
  (`slip::param("from")`) and `#embed`ded data.
- The standard `std::meta` facilities remain available to the transform for
  its *own* metaprogramming (reflecting over its own option structs, for
  example). They never denote subject entities.

Why not run the transform inside the subject TU, where `^^entity` would work?

1. Language mode. LLVM is built as C++17; much production code is older.
   Parsing the subject as C++26 to host the transform changes its meaning
   (comparison rewriting, `char8_t`, aggregate rules), which defeats "resolves
   exactly as before". A C++17 standard library is not usable in constant
   evaluation the way a transform needs (`std::vector` is not constexpr there).
2. Hygiene. The subject's macros would be live while the transform's headers
   are parsed; the transform's declarations and instantiations would enter the
   subject's AST and its digest.
3. Cost. The transform, `<slip/meta.h>` and the L2 library would be re-parsed
   in every one of tens of thousands of TUs.

An *in-TU bridge* for C++26 subjects (`slip::subject(^^ns::f)` converting a
native reflection to a Slip handle) is a possible later convenience and does
not change the model.

## 4. Values

```cpp
namespace slip { inline namespace v0 {
  class info;    // opaque, 64-bit, equality-comparable. Denotes a piece of the SUBJECT.
                 //   version 0: entity | declaration | reference (a written name) | expression
                 //   later:     statement | type as written | macro definition | macro expansion
                 //              | token | file inclusion
  class range;   // half-open byte range of spelled text within ONE file
  class code;    // replacement text: literal text, entities to spell, source ranges to copy
  class group;   // atomic edit group
  // later:
  class file;    // identity of a source file (canonical path + content digest)
  class guard;   // predicate over folded facts, evaluated in reduce
}}
```

`info` is a class wrapping one integer. It is a class type and not a scalar:
Kiln's cache of pure calls does not apply to functions that take or return
one, and nothing in the API depends on that cache (`spec/api.md` §5.1).
Entity handles are interned per TU by the host; syntax handles
(expressions, references, tokens) encode a kind tag and an index and cost nothing to
create — a TU has millions of them. Kind is queried, not typed: `is_reference(i)`,
`is_expression(i)`, `is_entity(i)`. Passing the wrong kind to a query throws
`slip::error` (catchable; uncaught, it fails that observer with a diagnostic
pointing into the transform's source).

Optional typed views live in L2 (`slip::view::call`, `slip::view::name_use`),
constructed from an `info` with a kind check. The core stays untyped so that
it can grow without breaking transforms, the same reasoning as
`std::meta::info`.

### Two views everywhere

Every piece of syntax has an **as-written** view (what to edit) and a
**semantic** view (what it means). The API names the view explicitly instead
of inheriting the collision between the two current standard-track papers:
`arguments_as_written(call)` omits defaulted arguments; `operands_of(call)` is
semantic and includes them.

## 5. Queries (L1 core, illustrative)

Final signatures belong to `spec/api.md`, which fixes version 0: `lookup`,
the kind predicates, `identifier_of`, `display_string_of`, `parent_of`,
`parameters_of`, `declarations_of`, `entity_of`, `references_to`,
`referent_of`, `name_range_of`, `extent_of`, `source_text_of`,
`is_editable`, `calls_to`, `callee_of`, `callee_reference_of`,
`arguments_as_written`. Everything else in the block below is *later*: the
direction, not a contract. In version 0 a written name is a **reference**
and the general `uses(filter)` below is `references_to(entity)`; text comes
back as `std::string`. Sequence-returning functions return
`std::vector<info>`; counting and index forms (`count_…`, `…_at`) may be
added so hot loops need not allocate.

```cpp
// --- finding things -----------------------------------------------------------------------------
std::vector<info> lookup(std::string_view qualified_name);    // empty if this TU never declares it
info              translation_unit();
std::string_view  key_of(info entity);          // stable across TUs and runs; see 03 §2

// --- entities: mirror std::meta ------------------------------------------------------------------
std::string_view  identifier_of(info);          info parent_of(info);          info type_of(info);
bool is_namespace(info); bool is_function(info); bool is_type(info); bool is_variable(info); /* … */
std::vector<info> members_of(info);   std::vector<info> bases_of(info);   std::vector<info> parameters_of(info);
info template_of(info);               std::vector<info> template_arguments_of(info);

// --- declarations as written -----------------------------------------------------------------------
std::vector<info> declarations_of(info entity);  // every declaration, redeclaration, namespace reopening
info              entity_of(info decl_site);
range             name_range_of(info);           range extent_of(info);       // whole construct
range             body_range_of(info decl_site);                              // between the braces

// --- names as written: uses ------------------------------------------------------------------------
struct use_filter { info within = {}; info referent = {}; info referent_within = {};
                    bool in_templates = true; bool implicit = false; };
std::vector<info> uses(use_filter = {});         // written occurrences of names: expressions, types,
                                                 // qualifier components, bases, using-declarations, …
info              referent_of(info use);         // the entity it resolved to
std::vector<info> referents_of(info use);        // dependent names: one per distinct instantiation result
enum class found_by { unqualified, qualified, member, adl, /* open */ };
found_by          how_found(info use);
info              found_through(info use);       // using-declaration / directive / alias, or null
range             qualifier_range_of(info use);  // `a::b::` as written; empty if unqualified
std::vector<info> qualifier_components_of(info use);          // each `a::` is itself a use
info              enclosing_declaration_of(info);             // lexical context of any node

// --- expressions and statements ----------------------------------------------------------------------
bool is_call(info);  info callee_of(info call);  std::vector<info> operands_of(info);
info              callee_reference_of(info call);             // the callee's name as written (a reference)
std::vector<info> arguments_as_written(info call);
std::vector<info> calls_to(info function);       // however spelled: alias, using, ADL, operator syntax
std::vector<info> children_of(info);             info parent_node_of(info);
bool is_implicit(info);                          std::string_view source_text_of(info);

// --- files, ranges, macros -----------------------------------------------------------------------------
file file_of(range);  std::string_view path_of(file);  bool is_owned(file);   // owned = editable by this run
range before(range);  range after(range);  range join(range, range);
enum class origin { file_text, macro_argument, macro_body, pasted, stringised, builtin, command_line };
origin            origin_of(info node_or_token);
info              expansion_of(info);            info macro_of(info expansion);
std::vector<info> expansions_of(info macro);     // in this TU
bool              is_editable(range);            // owned file, not virtual, origin allows it (04 §2)
```

Notes on semantics that matter for correctness:

- `uses()` enumerates *written* occurrences. A name in a template definition
  is one use with possibly several referents, one per distinct instantiation
  outcome observed in this TU. An edit is right only if it is right for all of
  them; the reducer and the verifier enforce that across TUs
  ([02](02-pipeline.md) §4).
- Ranges are always spelling ranges in one file. A node spelled across a macro
  boundary has no single range; `extent_of` returns the null range (absence
  is a value, not an error) and `is_editable` is the test to use first.
- Queries are answered from the subject model. Two classes exist: *static*
  queries (facts recorded while the subject was parsed; always available,
  cacheable, replayable from a stored model) and *probes*
  (`slip::probe::…`, which ask the live subject front end a what-if question,
  such as "would this expression compile with that argument type"). Probes
  are specified separately, may be unavailable when a transform runs from a
  cached model, and are not needed for the namespace migration.

## 6. Code values

```cpp
code text(std::string_view literal);       // literal source text
code spell(info entity);                   // "the name of this entity, correctly qualified where it lands"
code copy(range);                          // that text as it will be: comments, formatting, macros kept,
                                           // and any accepted edit inside the range applied (by reference)
code copy(info node);                      // = copy(extent_of(node))
code cat(auto&&... pieces);                // strings, ranges (copied), codes. Not an info: say spell or copy
```

`spell` is the requalification primitive. A transform never builds a
qualified name as a string. It says *which entity* it means; Slip chooses the
spelling at the destination, in the post-image, under a policy
(`minimal`, `relative`, `fully_qualified`), and the verifier proves the choice
resolves to that entity ([03](03-equivalence.md) §5). If the minimal spelling
fails verification, Slip falls back to a longer one. Version 0 has two
spellings (a sibling of the name that was written, else fully qualified),
no fallback, and the proof: verification checks every spelled name in the
post-image (`spec/api.md` §7.2).

Planned sugar, once Kiln has token-sequence literals (the P4380 value model
without injection): `^^{ \(spell(e)):: }` constructs the same `code` value.
The library form above is the durable interface; the literal is convenience.

### The rule: a transform does not manipulate code as strings

The operator's rule for this API (2026-11-22): any time a user's transform
performs string-level manipulation of C++ code, the API has failed. `spell`
is that rule applied to names. The M4 design applies it to everything else:

- **Every common edit has a structural primitive** that takes nodes, not
  text: remove a statement; remove an element of a list together with its
  separator (an argument, a parameter, a base, a mem-initialiser, an
  enumerator, a capture, a template argument); unwrap a block; replace a
  node by a node or by a copy of one; reorder the elements of a list.
  A primitive carries the checks its edit needs. Unwrapping a block: every
  name inside the block and after it in the enclosing scope resolves as
  before, and no object with a non-trivial destructor changes lifetime
  unless the block was the last statement of its scope; a failed check is
  a diagnostic at the site, not an edit.
- **New code is a parsed fragment with typed holes**, not concatenated
  text. The fragment is parsed as what it claims to be (an expression, a
  statement, a declaration, a type) when the transform is compiled, so a
  transform that would build ill-formed code does not compile. A hole
  takes an `info` (filled by `spell`), a `code` (filled by `copy`) or a
  value (written as a literal of its type).
- **`text()` and `cat()` remain as an escape hatch** for what no primitive
  covers yet. Each use is visible in the transform's source and counted in
  the run's report. A count above zero in a reference transform or in an
  example of this document is a defect of the API, and the list of uses
  is the backlog of primitives.
- **Acceptance test of the design**: the reference transforms are written
  with no string operation on code. They are the namespace migration of
  §9.2 and the removal of a feature flag in its three forms (a local
  switch object tested by `if (sw.isOn())`; the switch held in a data
  member set by a mem-initialiser and used across translation units; a
  `bool` member initialised from `sw.isOn()` and branched on in member
  functions, which needs a fold under an assumption, the classification
  of each reference as a read, a write or an escape, and a fact joined
  across translation units).

§9.2 as written predates the rule: `cat(spell(parent_of(target)), "::")`
builds a qualifier by concatenation. The M4 design replaces it with a
primitive that requalifies a use for its target.

## 7. Effects: edits, facts, reports

A transform's only effects are records appended to the observer's output
shard. They are write-only: nothing the transform can query changes.

```cpp
// ---- version 0 (spec/api.md §7) ------------------------------------------------------------------
struct edit_options {
  group        atomic = {};      // all edits of a group apply, or none
  std::string_view note = {};    // shown in reports
  // later:  guard when = {};    // apply only if this predicate over folded facts holds
  //         int   order = 0;    // tie-break for insertions at the same offset
};
group new_group();
void replace(range, code, edit_options = {});

// Declared semantic changes: what the verifier should expect to differ (03 §4).
void expect_changed(info reference_or_expression, std::string_view why);   // intentional change, at a place
void expect_renamed(info entity, std::string_view new_identifier);         // same entity, new name

enum class severity { note, review, error };
void report(severity, std::string_view message);
void report(info where, severity, std::string_view message);
void report(range where, severity, std::string_view message);

// ---- later (names reserved) ----------------------------------------------------------------------
void insert(range at, code, edit_options = {});     // `at` is empty: before(r) or after(r)
void remove(range, edit_options = {});
void create_file(std::string_view path, code, edit_options = {});   // whole-cloth generation
void relocate(info entity, std::string_view new_qualified_name, edit_options = {});

// Facts: contributions folded across all observers by a monoid, readable only through guards
// and in a reduce hook.
template<class T, class Fold> struct fact;       // e.g. fact<bool, all_of>, fact<long, sum>, fact<set, set_union>
void contribute(fact_key, value);
guard holds(fact_key);  guard operator&&(guard, guard);  guard operator!(guard);
```

A declared change is made at a place, the call or the name being edited, and
not at a declaration: the host knows which unit the digest uses, and the
declaration is in force only if an edit at that place is accepted
(`spec/api.md` §7.5).

Why guards and facts (later): many decisions need whole-program knowledge. "Does this
entity move?" is true only if *every* declaration of it, in every TU, is in
owned code. One TU cannot know. Instead of a scan pass followed by a second
full parse, each TU contributes `owned[E] &= is_owned(site)` for the
declarations it sees and emits its edits guarded by `holds(owned[E])`. Reduce
folds the facts and keeps or drops the guarded edits. A transform that needs
logic beyond guards may define a `reduce` entry point, evaluated in the same
transform universe with the folded facts bound instead of a subject TU.

## 8. Three granularities

| Level | Selector | Action | Use |
|---|---|---|---|
| A. Native | native (Rust) | native | Built-in transformations configured by data: `slip::refactor::rename`, `relocate_namespace`. Fastest; no per-site evaluation. |
| B. Rules | native | constexpr C++ per match | The host evaluates a selector (`calls_to(f)`, `references_to(e)`) natively over indexes of the subject model; the transform loops over the matches in the same evaluation. Version 0 has no callback registration and no isolation between matches (`spec/api.md` §2.4, §9). |
| C. Free-form | constexpr C++ | constexpr C++ | Walk the model with queries. Most flexible; slowest; opts out of memoisation. |

Kiln's evaluator runs roughly 65 M steps/s on scalar code but about 10 M on
string-heavy library code (Kiln's own baselines). The design keeps per-site
work native wherever possible: selectors, text assembly (`code` values are
host-side), spelling, emission. Level B is the expected default.

The L0 primitive under all of this is one generic host call (operation number,
integer arguments, caller-provided result buffer). All typing lives in L1.
Kiln therefore needs one small, closed extension rather than a growing table
of Slip-specific intrinsics ([05](05-kiln-interface.md) §3).

## 9. Examples (illustrative)

### 9.1 Replace `lib::foo(a, b)` with `lib::bar(b, a)` — Level B

```cpp
#include <slip/meta.h>
using namespace slip;

consteval void transform() {
  for (info foo : lookup("lib::foo")) for (info bar : lookup("lib::bar")) {
    if (!is_function(foo) || parameters_of(foo).size() != 2 || !is_function(bar)) continue;
    for (info call : calls_to(foo)) {
      auto args = arguments_as_written(call);
      if (args.size() != 2 || !is_editable(extent_of(call))) {
        report(call, severity::review, "not a plain two-argument call");
        continue;
      }
      group g = new_group();
      replace(extent_of(callee_reference_of(call)), spell(bar), {.atomic = g});
      replace(extent_of(args[0]), copy(args[1]), {.atomic = g});
      replace(extent_of(args[1]), copy(args[0]), {.atomic = g});
      expect_changed(call, "foo(a,b) -> bar(b,a)");
    }
  }
}
```

This is version 0 as specified; the complete transform, with the cases this
sketch skips, is `resource/examples/swap_arguments.cpp`. `callee_of(call)`
is the function as resolved; the name to edit is the callee's *reference*.
The edits are small and local (the callee name, each argument) so that
independent transforms compose; the call's own formatting and any macros in
the arguments are preserved because arguments are copied, not retyped.

### 9.2 Migrate root namespace `legacy` → `acme` — Level A, and what it expands to

What the user writes:

```cpp
#include <slip/refactor.h>
consteval void transform() {
  slip::refactor::relocate_namespace({ .from = "legacy", .to = "acme" });
}
```

Ownership (which files may be edited) comes from the run configuration, not
the transform. The core of the library function, at Level B. It is **not
version 0**: it uses facts, guards, `relocate`, `insert`, `uses` and
`how_found`, all later:

```cpp
consteval void relocate_namespace_impl(info old_ns, std::string_view to) {
  // An entity moves iff every declaration of it, in every TU, is in owned code. Global fact.
  auto moves = [](info e) { return holds(fact_key{"owned", key_of(e)}); };

  // 1. Each owned `namespace legacy {` is renamed; each entity declared there is declared relocated.
  //    (Simplified: a block that also holds members which must stay is split, not renamed. 03 §7.)
  for (info site : declarations_of(old_ns))
    if (is_owned(file_of(name_range_of(site))))
      replace(name_range_of(site), text(to));
  for (info site : declarations_within(old_ns)) {
    info e = entity_of(site);
    contribute(fact_key{"owned", key_of(e)}, is_owned(file_of(name_range_of(site))));
    relocate(e, relocated_name(e, to), {.when = moves(e)});
  }

  // 2. Every written name that denotes something inside `legacy`.
  for (info use : uses({.referent_within = old_ns})) {
    if (!is_owned(file_of(name_range_of(use)))) continue;        // never edit dependency sources
    info target = referent_of(use);
    switch (how_found(use)) {
      case found_by::qualified:        // `legacy::x::T`: respell the qualifier if the target moves
        replace(qualifier_range_of(use), cat(spell(parent_of(target)), "::"), {.when = moves(target)});
        break;
      case found_by::unqualified:      // found from inside `namespace legacy`; if the use site moves
      case found_by::adl:              // and the target stays, it must be qualified explicitly
        insert(before(name_range_of(use)), cat(spell(parent_of(target)), "::"), {.when = !moves(target)});
        break;
      default: break;
    }
  }
}
```

Three things this sketch is meant to show. The transform never spells a
qualified name itself. The decision that needs global knowledge (`moves`) is
a guard. And the case analysis is *not* what correctness rests on: if a case
is missing or wrong, the verifier reports the reference whose resolution
changed, and the repair hook ([03](03-equivalence.md) §6) gets a chance to fix
it. The complete treatment of the migration, including the hazards a simple
case analysis misses, is [03](03-equivalence.md) §7.

## 10. Whole-cloth generation

`create_file` and insertions with empty ranges (both later than version 0)
cover generating new code:
bindings, serialisers, forwarding headers. The payload is the same `code`
value, with entities spelled by Slip at the destination. Generated files are
parsed and verified like any other post-image file. If Kiln later implements
standard token injection, in-TU generation becomes available with the same
value types; Slip does not depend on it.

## 11. What is deliberately not in the API

- No way for a transform to observe its own or another transform's edits.
  Order of evaluation, of TUs and of transforms therefore cannot matter.
- No I/O, no clock, no randomness.
- No string-built qualified names in the L2 library; `spell` only.
- No string manipulation of code in a transform (§6 "The rule"): `text` and
  `cat` are a counted escape hatch, not the way to build code.
- No direct access to Kiln node ids or to L0.
