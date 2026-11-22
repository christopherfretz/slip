# 04 — Macros and conditional compilation

Status: the tiers in §2 and the accounting in §5 are design. §6 is research,
labelled as such, with the order in which to attempt it.

## 1. The problem, precisely

Refactoring edits bytes in files. The compiler analyses tokens after
preprocessing. Macros break the correspondence between the two in three ways:

1. **A node's tokens may not exist in any file** (`PREFIX##_init`), or exist
   somewhere other than where the node appears (a macro body).
2. **One spelling, many meanings.** A token in a macro body is spelled once and
   compiled at every expansion, in every TU, possibly resolving differently
   each time. A macro argument used twice in the body is compiled twice.
3. **Bytes that were not compiled.** Inactive `#if` branches and unexpanded
   macro bodies have no semantics in this observer at all.

The second and third are the shared-header problem again (one piece of text,
several observers that must agree, some of whom did not look), and Slip
handles them with the same machinery: site keys, unanimity over coverage, and
verification. The first is a question of provenance, which is where owning the
preprocessor matters.

## 2. Token origin and the tiers

Because Kiln's preprocessor is ours, Slip can ask it to record, for every
token delivered to the parser, exactly where it came from. Every node handle
answers `slip::origin_of`, and every edit is classified by the origin of the
bytes it touches:

| Tier | Bytes being edited | Example | Rule |
|---|---|---|---|
| 0 | Ordinary file text | `legacy::f(x);` | Normal path |
| 1 | Inside one **macro argument**, at the expansion site | `CHECK(legacy::f(x))` | Editable. Every substitution of that argument in the expansion is a vote; all must agree. See stringising below. |
| 2 | Inside a **macro body** (the `#define`), owned file | `#define LOG(m) legacy::log(m)` | Editable only if every expansion in every observer agrees. Site key is the spelling range in the definition; expansions are the voters. A definition never expanded is uncovered. |
| 2u | Macro body in read-only code | A dependency's `DEP_REGISTER(T)` that spells `legacy::` | Never edited. Reported with the expansions affected; usually needs a compatibility declaration. |
| 3 | Tokens made by `##` or `#` | `NS##_init`, `#T` | Refused and reported. The identifier the compiler resolved has no bytes. Research track R1. |
| 4 | Preprocessor entities: macro names, `#if` conditions, `defined(X)`, `#include` | Renaming a macro | A different kind of transform, over the preprocessing record instead of the AST (§4). |
| X | A range that crosses an expansion boundary | Starts in file text, ends inside an argument | Refused unless it covers whole expansions end to end, or lies inside a single argument. A macro invocation may be replaced or removed as a unit. |

Tier 1 is the common case in real code (`assert`, `EXPECT_EQ`, `CHECK`,
logging macros) and it must work or nothing inside a test file can be renamed.

**Stringised arguments.** `assert(x)` uses its argument as code and also
stringises it, so renaming inside `x` changes a string. The default is to
allow the edit when the argument is also used as code, and to record the
string change in the report at note severity; `--strict-strings` turns it into
a rejection. An argument that is *only* stringised is tier 3.

**Nodes and ranges.** A node whose tokens all come from one argument has an
ordinary file range (the argument's spelling). A node assembled from body
tokens and argument tokens has none: `extent_of` throws and `is_editable`
returns false. The children of such a node may still be individually editable,
which is why rules should edit the smallest thing they can.

The tier is not what makes an edit safe. A tier-2 edit that every expansion
voted for is still verified by digest in every observer, like any other. The
tiers decide what Slip will *attempt* and how it explains a refusal.

## 3. What the transform sees

```cpp
origin            origin_of(info);              // file_text | macro_argument | macro_body | pasted | …
info              expansion_of(info);           // innermost expansion producing this node/token, or null
info              macro_of(info expansion);
std::vector<info> expansions_of(info macro);    // in this observer
range             invocation_range_of(info expansion);          // `NAME(args)` at the expansion site
range             argument_range_of(info expansion, int i);
range             definition_range_of(info macro);
bool              is_editable(range);
```

Most transforms never call these. `slip::on` skips matches that are not
editable and reports them by tier, so a rule written without macros in mind
degrades into "N sites need manual attention, grouped by macro", which is the
actionable form: one macro usually accounts for thousands of sites.

## 4. Transforming macros themselves

The preprocessing record (definitions, undefinitions, every expansion with its
argument ranges, every conditional directive with its outcome, every
inclusion) is part of the subject model and queryable. That supports
preprocessor-level transforms later: renaming a macro (definition, expansions,
`#ifdef` mentions), include rewriting, converting a function-like macro to a
`constexpr` function where every expansion's arguments are well-formed
expressions (the result is checked as a declared change:
[03](03-equivalence.md) §4). None of this is in the first milestones; the
record is designed so that they are library work, not redesign.

## 5. Conditional compilation and coverage

A run sees what its observers compile. Platform branches, feature toggles and
debug-only code are compiled only if some configuration enables them, so
**configurations are inputs**: supply a compilation database per configuration
and each (TU, configuration) votes and verifies independently. Bytes two
configurations both compile need both to agree; bytes only one compiles need
only that one.

For everything else, Slip accounts instead of guessing. Each observer records
the owned byte ranges it compiled. After reduce, every owned byte is in one of
the states of [02](02-pipeline.md) §9, and the uncovered ones carry a reason:

| Reason | Example | What the user can do |
|---|---|---|
| Inactive conditional in every configuration | `#ifdef _WIN32` on a Linux-only run | Add a configuration |
| Macro defined, never expanded | An unused helper macro | Review by hand |
| Template never instantiated; member of a class template never used | Header-only library code | Add a TU that instantiates it (tests usually do) |
| File in no TU | Orphaned header | Synthesised TU, or exclude |
| Only observer failed | Parse error under Kiln | Fix, or report to Kiln |

The headline number, **owned bytes covered / owned bytes**, is printed by every
run and tracked by the harness for libc++ and LLVM. A refactoring tool that
silently skips the Windows half of a file is the failure this exists to
prevent.

For uncovered regions that mention a renamed name, Slip can offer *textual
proposals*: lex the region, match identifiers against the relocation map, and
list candidate edits. They are never applied with verified edits, never
counted as verified, and are emitted as a separate patch for review.

## 6. Research tracks

In the order they should be attempted. Each is independent of the core
pipeline and can be prototyped once the preprocessing record exists.

**R0. Macro audit (cheap, high value, do first).** Before any migration, list
every macro whose body tokens resolved, in some expansion, to an entity that
would move, with expansion counts, whether expansions agree, and whether the
definition is owned. This is a pure query over the model, and it tells the
operator how hard the migration will be before anything is edited.

**R1. Token equivalence classes for pasted and stringised names.** An
identifier formed as `NS##_init` is determined by fragments that do exist as
bytes. Tracking, for each pasted token, the fragments that produced it gives
equivalence classes of byte ranges that must change together (the approach
CScout used for C). A rename is then possible if every member of the class is
in owned code, with the rule that one read-only member poisons the class.
Needs paste provenance from the preprocessor. Open question: how often this
matters outside C-style codebases.

**R2. Token-level replay check.** Preprocess the post-image and compare its
token stream with the pre-image's: the two must differ exactly where edits
intended and nowhere else. This is far cheaper than a parse, independent of
semantic analysis, and catches macro-edit mistakes (an edited body changing an
expansion nobody voted on) with a precise location. A candidate for a fast
pre-check before the full verify pass.

**R3. Expansion-site alternatives.** When a body cannot be edited (read-only,
or expansions disagree), sometimes the fix belongs at the expansion: passing a
qualified name as the argument, or wrapping the invocation in a scope where a
using-declaration restores the old lookup. Whether such rewrites can be
proposed mechanically, and verified, is open.

**R4. Macro to language construct.** Converting function-like macros to
`constexpr` functions or templates, and object-like macros to constants, with
verification per expansion. Valuable in its own right and reduces the macro
surface for later transforms.

**R5. Variability-aware analysis.** Parsing all configurations at once
(TypeChef, SuperC) is the complete answer to §5 and is out of scope: it does
not scale to C++ at this size in any published system. Slip's position is to
make multi-configuration runs cheap and coverage explicit. Revisit only if
coverage numbers on real codebases show a gap that configurations cannot
close.

## 7. What is needed from Kiln

Kiln already tracks, per token, both where it was spelled and where it was
expanded, and has the fix-it logic that maps a location to an editable file
range. That is enough for tiers 0, 1 and X. Tier 2 voting, the audit, coverage
of conditionals and all of §6 need the richer preprocessing record requested
in [05](05-kiln-interface.md) (argument ranges, conditional structure, a
stable identity per macro definition, paste and stringise provenance).

## 8. Corrections from the study behind KR4 (2026-11-22)

Writing [KR4](../requests/KR4-preprocessing-record.md) meant reading
Kiln's preprocessor line by line. Where this document says otherwise,
the request is right; the sections above are corrected when
`spec/macros.md` is written.

- §7 overclaims tier 1. Kiln's fix-it logic maps a range inside one
  argument to its spelling and knows nothing of the parameter's other
  substitutions, of a `#` use (raw tokens, no expansion entry) or of a
  `##` use; argument boundaries are dropped; an entry names no
  definition and only the final macro table is kept. Tier 1 as §2
  defines it needs KR4 stage 1. Tiers 0 and X stand.
- A paste operand does get a macro-argument entry. "Has an argument
  entry" does not mean "used as code": the parameter's uses decide.
- "Each conditional with its outcome" (§5): a condition after a taken
  group is never evaluated and a directive inside a skipped group is
  read to its name only. The outcomes are taken, not taken, not
  evaluated, dead; the last two have no reads.
- §4, renaming a macro: mentions in skipped groups and unevaluated
  `#elif`s are never lexed, and a function-like macro's name without
  `(` produces no event. Those sites are textual proposals, never
  verified edits.
- §2 tier X, "an invocation may be replaced as a unit", holds only
  when the name token is in a file: an invocation's name can come from
  a body while its `)` is in the file.
- §2 and [02](02-pipeline.md) §2, a token-origin map "for every token
  delivered to the parser": origin is derived on demand from the
  source manager's expansion entries plus a small table of made tokens
  (`##`, `#`, builtins), not stored per token.
- [02](02-pipeline.md) §2 and [05](05-kiln-interface.md) §1 principle
  3, "Slip's observer attached to the preprocessor": the record is
  retained by the preprocessor and read in the front-end closure. The
  callback slot is single and taken, and several events fire none.
- [01](01-transform-api.md), `origin::command_line`: Kiln does not yet
  tell `-D` lines from predefines (KR4 asks it to), and the
  enumeration needs a `predefined` member.
- §5's table of reasons lacks one: a header whose guard macro another
  file defined is entered and wholly dead, and its `#ifndef` is not a
  guard.

