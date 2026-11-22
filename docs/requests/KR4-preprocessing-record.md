# KR4 — the preprocessing record

A request from Slip to Kiln, sent 2026-11-22.
Context: [04](../design/04-macros-and-configurations.md) (this is its
Kiln half) and [05](../design/05-kiln-interface.md) §3, row KR4. Kiln
owns the design of everything inside Kiln; this states what Slip needs,
proposes a shape, and gives acceptance cases. Names of types and flags
are placeholders. File references are to Kiln `4bfd999da`.

## 1. Need

KR1 gives Slip the tokens of every written name as Kiln `Loc`s. Before
Slip edits one it must answer, from the `Loc`:

1. **Which file characters are these, and what else compiles them?**
   File text (tier 0); a macro argument at an invocation (tier 1: every
   substitution of that parameter is a vote, and it may also be
   stringised or pasted); a macro body (tier 2: every expansion of that
   definition, in every observer, is a voter); a token made by `##`, `#`
   or a builtin (tier 3: refused, with the operands named).
2. **Is it the same thing in another translation unit?** Votes from
   thousands of observers are merged per file in reduce
   ([02](../design/02-pipeline.md) §3), so a definition, an expansion,
   a conditional and an inclusion need a key that is equal in every
   translation unit that sees the same file text.
3. **What could not be edited and why**, by macro definition, and
   **which bytes no configuration compiled**, by deciding directive.

## 2. Kiln today

| Area | What exists, and what does not |
|---|---|
| Locations | One expansion entry per replacement list (`kiln_pp/src/expand.rs:255-260`), per run of argument tokens substituted for a parameter (`:797-844`), and per `##`, `#` or builtin result, spelled in a scratch buffer (`:924-929`, `:997-1013`). An entry has a spelling, the range it replaced and `Macro { name }` or `MacroArg` (`kiln_source/src/manager.rs:131-165`), in 24 bytes (`:171-186`). Body or argument, the bytes, and the parameter an argument token replaced are answerable now. |
| Missing on an entry | No definition. An empty expansion makes no entry (`expand.rs:226-230`). Paste, stringise and builtin results are all `Macro` entries in scratch space (`PpFlags::STRINGIFIED`, `kiln_pp/src/tok.rs:22`, is crate-private). Argument boundaries are dropped with the buffers (`expand.rs:309-321`). |
| Definitions | `#define` replaces the table entry (`kiln_pp/src/directive.rs:368`), `#undef` removes it (`:430`); only attribute macros keep a history (`kiln_pp/src/macros.rs:153-173`). `#pragma pop_macro` restores a definition without a callback (`kiln_pp/src/pragma.rs:430-446`). `-D` and `-U` are lines appended to the predefines in one virtual buffer, `<built-in>` (`kiln_pp/src/preprocessor.rs:591-611`). |
| Callbacks | One boxed observer (`preprocessor.rs:303`, `:531`), taken by `-M` or coverage in the driver (`kiln_driver/src/pp_cmd.rs:753-754`). `macro_expands` gets the name token, the definition and the invocation range (`kiln_pp/src/callbacks.rs:163-167`) before the body's entry exists (`expand.rs:223`, then `:255`). |
| Conditionals | `Cond` is a transient stack (`preprocessor.rs:145-156`). `source_range_skipped` fires once per *run* of skipping, which may cross several `#elif` groups (`directive.rs:1052-1146`), not once per group as 05 says. A condition after a taken group is never evaluated (`:1000-1021`, `:1164-1167`). In a skipped group a line is read only as far as a directive name (`:1064-1099`; `spec/pp.md:1250-1257`). |
| Guards | Detected when the file ends (`preprocessor.rs:1172-1181`), kept crate-private (`:321-323`). `#ifndef X` with `X` already defined is not a candidate (`directive.rs:977`, `:925-927`). |
| Inclusions | `inclusion_directive` and `file_skipped` (`directive.rs:1400`, `:1436`; `kiln_pp/src/include.rs:31-78`). Nothing is reported at the two depth-limit exits (`directive.rs:1428-1430`, `:1440-1444`), for `__has_include` (`:1500-1534`) or for `#embed` (a marker, `:1869`). |

Where the study (`research/kiln-pp-source.md`) and the source differ:
its §1.5 item 4 says an argument used only under `##` gets no `MacroArg`
entry. It does: paste operands are relocated like any substituted
argument (`expand.rs:716-737`, `:574-584`); only `#` reads the raw
tokens (`:665`). So some `MacroArg` entries hold a token that never
reaches the parser, and "has a `MacroArg` entry" does not mean "used as
code". Also, `def_start` is the first body token
(`directive.rs:484-497`), not the name token as `macros.rs:57` says;
the keys below use `name_span` (`macros.rs:45`).

## 3. Proposed shape

### 3.1 Delivery

A retained record owned by the `Preprocessor`, off by default, reached
as `pp.record() -> Option<&PpRecord>` and switched on by a field of
`FrontEndOptions` beside `record_lookups` (branch `slip/kf1`,
`kiln_driver/src/front_end.rs:46`). Slip's closure holds `&Preprocessor`
and `&SourceManager` there (`:73-105`). Rejected: more `PpCallbacks`
methods. The slot is single and taken, the entry does not exist when
`macro_expands` runs, `pop_macro` and `__has_include` fire nothing, and
a restored PCH calls no callback (`spec/pp.md:1197-1200`). This departs
from 05 §1 principle 3, for the reasons 05 §4 gives for KR1.

### 3.2 Keys that are equal across translation units

The record stores TU-local `Loc`s and dense indices (`DefIx`, `ExpIx`).
A key is a function of an entry, computed by Slip when it writes its
shard. `P(loc)` is (file identity from KF2, byte offset).

| Thing | Key |
|---|---|
| Definition in a file | `P` of the macro-name token of the `#define` |
| Builtin macro | `builtin(name)` |
| Predefined or `-D` macro | (`predefined` or `command-line`, name, hash of parameters and replacement text). Not an offset: the text of `<built-in>` differs between two commands that differ in one unrelated `-D`. Kiln says which lines are which. |
| `#undef`, `push_macro`, `pop_macro` | `P` of the directive's `#`; `-U name` |
| Expansion whose name token is in a file | `P` of the name token |
| Any other expansion (name token from a body or a substituted argument) | (key of the nearest ancestor whose name token is in a file, ordinal among the expansions under that ancestor, in preprocessing order) |
| Conditional or inclusion directive | `P` of the `#`; (`command-line`, index) for `-include` |
| Include context | the chain of inclusion keys from the main file |

Each is fixed by the file's text, and for nested expansions by the text
plus the definitions in effect, which every claim about an expansion
names. A header entered twice has the same keys both times; the
instances differ in include context, and every entry's `Loc` lies in
one inclusion's range (`manager.rs:997-1012`). Slip counts each
(observer, include context) as a voter, so two inclusions with
different results disagree at one key. Rejected: keys by path alone,
which split one file reached through two spellings; and a chain of
spelling positions for nested expansions, which still needs an ordinal
(case 12).

### 3.3 Definitions

Every definition is kept, in order, including those later undefined or
replaced (`Rc<MacroDef>`, `macros.rs:119`). Per definition: the
`MacroDef`; its source (file, builtin, predefines, command line); the
directive's range from `#` to the end of its last line; how it ended
(`#undef` at a location, replaced by `DefIx`, or not). `push_macro` and
`pop_macro` are events naming the definition saved or restored: a pop
makes an earlier `DefIx` current again. A function gives the uses of
parameter `i` in the body (plain, operand of `#`, operand of `##`,
inside `__VA_OPT__`), so that `expand.rs:653-680` is the only classifier.

### 3.4 Expansions and the origin of a token

One entry per expansion, the empty and the builtin ones included, in
preprocessing order: an invocation is entered after its arguments are
collected and before they are pre-expanded (`expand.rs:210-223`,
`:480-515`), so an outer expansion precedes those inside its arguments.

```rust
// A proposal. Kiln owns the final form.
pub struct Expansion {
    pub def: DefIx,            // the definition expanded
    pub name: Loc,             // the invocation's name token
    pub end: Loc,              // past `)` or past the name; may be in
                               // another entry than `name` (case 14)
    pub entry: Loc,            // base of the body's entry, or invalid
    pub parent: Option<ExpIx>, // whose argument or result held the name
    pub args: Range<u32>,      // one span (lo, hi: Loc) per parameter
    pub flags: u8,             // in a directive; elided; regrouped
}
```

An argument span is the argument as written, before pre-expansion, in
the locations its tokens had when collected. An empty argument is an
empty span at the `,` or `)` that ended it (`expand.rs:431`). The
variadic argument is one span, commas included; an omitted one is
flagged and has no bytes (`:446-466`). An invocation regrouped by error
recovery (`:328-365`) is flagged, and Slip does not edit in it.

**Token origin is not stored per token.** Kiln provides one function
from a `Loc` to its origin, computed from the entry the `Loc` is in:

| Origin | Found from | Says |
|---|---|---|
| file | not a macro location | file and offset |
| body | a `Macro` entry spelled in a `#define` | the expansion, and body token `k` of its definition |
| argument | a `MacroArg` entry; its `expansion` is the parameter token in the body's entry | the expansion, argument `i`, which use of the parameter, and the token's location before substitution: a file location in the argument's span, or a macro location to ask again (a forwarded argument, or the result of an expansion inside the argument) |
| pasted | the table below | the expansion and each operand's location before pasting |
| stringised | the table below | the expansion and the argument (or `__VA_OPT__`) |
| builtin | the table below | which builtin |

The one addition to what the source manager keeps is a **table of made
tokens**: one row per `##`, `#` or builtin result, keyed by the base of
its entry, with the kind and the operand locations. The three look
alike today, and a paste entry's `expansion` gives its operands only as
positions in the body (`expand.rs:919-929`).

Cost: about 24 bytes per expansion, 8 per argument, 16 plus 4 per
operand per made token, nothing per delivered token. The source manager
already spends 24 bytes per replacement list and per argument run, so
the record is of the order of the expansion entries, not of the token
count. Slip asks for its size as a counter and will gate on it.
Rejected: an origin stored per token of a retained buffer, 20 bytes or
more for each of the millions of tokens of an LLVM translation unit,
when Slip asks only about tokens KR1 names. Rejected: Slip finding
argument boundaries by lexing the invocation, a second `read_args`
that is wrong when the tokens come from another expansion.

Queries: the origin of a `Loc`; an expansion by index; the expansions
of a definition; the expansions whose name token lies in a file range.

### 3.5 Conditionals

One entry per conditional directive: kind; the `#`; the directive's
range; the condition's range; its opening `#if`; the range of the group
it controls; and an outcome:

| Outcome | Meaning |
|---|---|
| taken, not taken | the condition was evaluated (`#else`: decided by its siblings) |
| not evaluated | it follows a taken group; Kiln never evaluates it |
| dead | it is inside a skipped group; nesting only |

For an evaluated condition, its **reads**: every macro name tested by
`defined`, `#ifdef` and the like, every macro expanded, and every
identifier left after expansion (it evaluates to 0), each with the
definition bound at that point or "undefined"; operands that
short-circuit are included. Each `__has_include` with the name, the
file found or none, and the search-path index. Each feature query
(`__has_cpp_attribute` and its relatives) with its value. The opener of
a file's include guard is flagged once Kiln recognises it; `#pragma
once` is recorded with its location.

From this Slip states, per region, "dead in every observer, decided by
this directive reading these names", and per edit, "verified where `A`
is this definition and `B` is undefined". It cannot prove an edit safe
under a configuration nobody ran. Skipped text stays unlexed: Slip
lexes the ranges itself (`kiln_lex/src/lexer.rs:134`) for proposals.

### 3.6 Inclusions

One entry per `#include`, `#include_next`, `#import`, header-unit
`import`, `#embed` and `-include`: the directive's range; the name's
range, and whether the name came from a macro (with the expansion); the
name as written and its form; the file found and the search-path index;
and an outcome: entered (with the base of the new inclusion range),
skipped by `#pragma once` or `#import`, skipped by a guard (naming the
guard's current definition), not found, depth limit. KF1's `consulted`
and lookup log (`front_end.rs:100-104`) say which paths a result depends
on, for cache keys. New here is position: which directive, what was
written, what happened there, and the tree of instances.

## 4. Behaviour

1. **Opt-in.** Off: one predictable branch per site, performance gates
   unmoved. On or off, tokens, diagnostics, `-E` output and every `Loc`
   are identical: recording adds no expansion entry and no buffer.
2. **Deterministic.** The same input gives the same record, index for
   index. `__DATE__` and `__TIME__` are recorded by kind, not by text.
3. **Complete over what was preprocessed.** Every macro location has an
   origin; every expansion has an entry; every skipped byte lies in one
   innermost group. After a fatal error it holds what came before.
4. **PCH and modules** may be left out at first: Slip parses without a
   PCH. From the start the record says which locations it does not
   cover (a restored prefix, an imported image); it is never silently
   partial. Carriage in the snapshot and the image is the last stage.

## 5. Acceptance cases

A dump (`-fkiln-dump-pp-record`), one line per entry with keys printed
as `file:offset`, so a test is one file. `L` is `legacy`.

| # | Source | The record must say |
|---|---|---|
| 1 | `#define CHECK(c) ((c) ? 0 : fail(#c))` then `CHECK(L::ok(x));` | one expansion; argument 0 is the file range of `L::ok(x)`; `L` has origin argument 0, located in that range; parameter `c`: one plain use, one `#`; the string token is stringised from argument 0 |
| 2 | `#define TWICE(x) x + x` then `TWICE(L::v)` | two tokens `L` with origin argument 0, different uses, the same file location |
| 3 | `#define LOG(m) L::log(m)` and 500 invocations | one definition key; 500 expansions listed for it; each `L` is body token 0 of that definition; all share one spelling position |
| 4 | `#define DECLARE(ns, T) namespace ns { struct T; }` then `DECLARE(L, Buf)` | `namespace` is body token 0; `L` is argument 0 and `Buf` argument 1, each with its file range |
| 5 | `#define INIT(ns) ns##_init()` then `INIT(L)` | `L_init` is pasted from argument 0 (the file location of `L`) and body token `_init`; `ns` has no plain use; no delivered token has origin argument |
| 6 | `#define STR(x) #x` then `STR(L::T)` | one stringised token from argument 0, whose span covers `L::T`; `x` is used only by `#` |
| 7 | `k.def`: `#ifndef K` / `#define K(n)` / `#endif` / `K(Add) K(Sub)` / `#undef K`; included twice, after `#define K(n) n,` and after `#define K(n) case n:` (as `llvm/IR/Instruction.h:946-985` does) | two inclusions, both entered, two include contexts; `K(Add)` has the same expansion key in both and a different definition key in each; `#ifndef K` not taken twice, each reading its own definition, not a guard; each `#undef` ends its own definition |
| 8 | `#define V a` / `V` / `#undef V` / `#define V b` / `V`; and the same without `#undef` | two definitions, two keys; the first ended by `#undef` (or replaced); each expansion names its own |
| 9 | `-DNS=L`, then `NS::f();`, in two commands that differ in another `-D` | the expansion's definition key is (`command-line`, `NS`, hash), equal in both; `L` is a body token of a definition with no file |
| 10 | `#define F(a, ...) f(a __VA_OPT__(,) __VA_ARGS__)` with `F(1)`, `F(1,)`, `F(1, x, y)` | variadic elided; an empty span at `)`; one span `x, y`; the `,` from `__VA_OPT__` is a body token |
| 11 | `#define EAT(x)` then `EAT(L::T)`; `#define NOTHING` then `NOTHING` | two expansions with no entry; argument 0 spans `L::T`; `x` has no use at all |
| 12 | `#define G(n) (n)` / `#define CALL(f) f(1) f(2)` then `CALL(G)` | three expansions; both `G` have parent `CALL` and keys (key of `CALL`, 1) and (key of `CALL`, 2) |
| 13 | `#define ID(x) x` then `ID(ID(L::v))` | two expansions, the inner one's name in the file, its parent the outer; `L` is argument 0 of the outer, located at argument 0 of the inner, located in the file |
| 14 | `#define F(x) x` / `#define APPLY F` then `APPLY(L::v)` | `F`'s expansion has `name` in `APPLY`'s body entry and `end` in the file; its argument span is in the file |
| 15 | `#if 0` / `LOG(L::x)` / `#define Z 1` / `#ifdef Q` / `#endif` / `#endif` | no expansion and no definition from the group; `#if` not taken with its group's range; `#ifdef Q` dead |
| 16 | `#if A` / `#elif B` / `#else` / `#endif` with `-DA=1` | `#if` taken, reading `A` with its key; `#elif` not evaluated, no reads; `#else` not taken |
| 17 | `#if defined(A) && B > 2`, neither defined | not taken; reads: `A` tested, undefined; `B` left as an identifier, undefined |
| 18 | `a.h` defines `B_H`; `b.h` is `#ifndef B_H` / `#define B_H` / … / `#endif`; both included | `b.h` entered; its `#ifndef` not taken, reading `B_H` bound to the definition in `a.h`; not flagged as a guard; no definition from `b.h` |
| 19 | a guarded header and a `#pragma once` header, each included twice | first: entered, the `#ifndef` taken and flagged as guard; second: skipped by guard (naming the definition), skipped by once |
| 20 | `#if __has_include(<optional>)`, `#if __has_include("nope.h")`, `#if __has_cpp_attribute(nodiscard) >= 201907L` | found with the file and path index; not found; the query's value |
| 21 | `#define H "x.h"` then `#include H` | the name's range is the token `H`; from a macro, with its expansion |
| 22 | `#define M 1`, `push_macro("M")`, `#undef M`, `#define M 2`, `M`, `pop_macro("M")`, `M` | the second expansion names the first definition again: same index, same key |
| 23 | `__LINE__`, `__FILE__`, `__COUNTER__` | three expansions of builtin definitions; three made tokens of kind builtin |
| 24 | one header included by two main files with different text before it | every key of an entry in the header is equal in the two dumps |
| 25 | any of the above, recording on and off, run twice | `-E` output and AST locations identical on and off; the two dumps identical |

## 6. Suggested staging

1. Definitions with their history, parameter uses, expansions with
   argument spans, and the origin function with the three made kinds
   reported as one (from scratch-space spelling, `manager.rs:1080`).
   Slip builds: the tier of any token; tier 1 edits with their votes
   and the string note; tier 2 voting keyed by definition; the macro
   audit; "defined, never expanded". This is what M4 waits for.
2. Conditionals (outcome, group range, guard, once), dead directives
   included (case 15 needs them), and inclusions.
   Slip builds: coverage with reasons; include contexts for `.def` files.
3. The table of made tokens, condition reads, `__has_include` and
   feature queries, push and pop events. Slip builds:
   equivalence classes for pasted names (04 R1), exact tier 3 reports,
   the configuration statement of an edit, macro renaming.
4. Carriage in PCH snapshots and module images; the completeness check
   and the size counter run on libc++ and an LLVM slice.

## 7. Not needed

Lexing skipped groups. Comments. A retained token stream. New callbacks
or several observers (KF3 asks for the latter on its own account). Keys
computed by Kiln: `Loc`s and KF2's file identity are enough. `Send` or
`Sync`. `#line`, `#error`, and pragmas other than `once` and push/pop.

## 8. Open questions for Kiln

1. Is a name token at a file location expanded at most once per
   inclusion? The expansion key of §3.2 relies on it.
2. Is a binary search by `entry` sound (bases grow in expansion order,
   `manager.rs:1273-1320`), or is an index on the entry cheaper? Could
   made tokens be a new `ExpansionKind`, given the snapshot and
   `image_locs` formats, or must they be a side table (scratch buffers
   are shared per distinct string, `preprocessor.rs:1240-1252`)?
3. A `MacroArg` entry covers a run of tokens up to 50 bytes apart
   (`expand.rs:823`). Can the origin function give the token's index in
   its argument without lexing again?
4. Counts of expansions, arguments and made tokens on an LLVM
   translation unit (`exp_macro`, `exp_macro_arg`, `expand.rs:111-112`),
   to turn §3.4's cost into megabytes before anything is built.
5. The skipping loop passes each `#elif` and `#else` it does not enter
   (`directive.rs:1111-1133`). Is noting them there cheap enough? Can
   `IfCx` (`:799-827`) log the identifiers left after expansion without
   help from the pluggable evaluator?
6. For `#include MACRO`, what does `name_span` cover after
   `lex_header_name` glues tokens (`directive.rs:1209`)? Parameter
   names have no locations (`macros.rs:46`): what would keeping them
   cost (parameter renaming, 04 R4)?

## 9. Kiln's phase 0 (2026-11-22, Kiln WP-2110)

Kiln measured before building; stage 1 started the same day. What
changes in the text above:

- **§8 Q1** yes: no file-located name token is expanded twice per
  inclusion (`<bits/stdc++.h>`, two LLVM units). Exhausted location
  space sets a flag `LOC_SPACE_EXHAUSTED`; Slip treats a flagged record
  as incomplete.
- **Q2** search by entry base is sound; the record keeps a side index
  (8 bytes per macro entry). No new `ExpansionKind`.
- **Q3** the record stores the start `Loc` of every raw argument token
  (4 bytes each); `origin()` derives a token's index from it.
- **Q4** size: about 9.7 MB for `SemaExpr.cpp` (131k expansions), 3.5
  MB for `Instructions.cpp`, 2.6 MB for `<bits/stdc++.h>`; the source
  manager's own expansion tables are about the same size.
- **Q5** skipped `#elif`/`#else` and every read, short-circuited
  operands included, are cheap to note.
- **Q6** under `#include MACRO` the name span is one macro entry or a
  glued `<…>` token (then `name` is the `<`); parameter-name locations
  are kept.
- **§2** is wrong in places: `-D`/`-U` are written in command-line
  order after a `# 1 "<command line>"` marker, not appended; a macro
  entry is 36 bytes; a replacement list can give more than one kind of
  entry (single token, `_Pragma` text, glued header name, the `##` of
  an invalid paste); `inclusion_directive` does fire at the depth
  limit, only the outcome is missing.

Stage 1 as Kiln builds it (`kiln_pp::record`): the switch is
`PpOptions::pp_record`; `pp.record() -> Option<&PpRecord>`;
`origin(sm, loc)` gives `File | Body{exp, tok} | Arg{exp, arg,
param_tok, from, index} | Made{exp, kind, from} | Unknown`; paste,
stringise and builtin are separate `MadeKind`s; an expansion's parent
is the one it was dynamically nested in (case 13); definitions met from
a PCH or an imported unit are registered when first met and the record
is flagged incomplete; `DefIx` and `ExpIx` are equal across two runs
of one input; Kiln computes no keys. Asked of Kiln in return: the
switch reachable through the front-end entry (`FrontEndOptions`), with
the record alive when the entry's closure runs.

## 10. Kiln's phase 0 for stage 2 (2026-11-22, Kiln WP-2111)

Kiln's design for conditionals, guards, inclusions and include
contexts, reviewed by Slip and accepted the same day; implementation
started on it. What changes in the text above:

- **§2** again wrong in places:
  - `#include` has three more silent exits: no name, an empty name,
    and a directive inside macro arguments.
  - The two exits at the depth limit differ: one is the limit; the
    other never re-enters a file already entered once the limit was
    hit.
  - A guard is the controlling macro as Clang means it: `#ifndef X …
    #endif` around a file is a guard even if the file never defines
    `X`, and the file is skipped again while anyone's `X` is defined;
    `#if !defined(X)` opens one too. None is recorded for the main
    file.
  - `#embed` resolves to bytes only: no file, no search index.
  - `-include` is an `#include "f"` line of the predefines buffer.
  - A header-unit import enters the buffer's own range, or nothing
    when its macros come from a module file.
- **§3.5** One row per directive (`#if`, `#ifdef`, `#ifndef`, the
  `#elif` forms, `#else`, `#endif`, stray ones too), keyed by its `#`,
  with the condition range (from the end of the directive name to the
  end of the line, untrimmed, the same for live and dead rows), a link
  to the opening directive and one to the next directive of the same
  conditional. Outcome: taken, not taken, not evaluated, dead, or
  closes (`#endif`). **The group range is not stored**: it runs from
  the end of the directive's line to the `#` of the next directive of
  the conditional; an unterminated conditional has its opener flagged
  and its last group runs to the end of the file. Reads (stage 3) come
  in a side table and do not change the row.
- **The guard** is a flag on the opener's row, set when the file is
  left, once per instance of the file. It says what Kiln decided, not
  that the file defines `X`: Slip finds the definition of `X` in the
  group through stage 1, and `X` by lexing the condition range. Case 18
  is a row not taken and not flagged. `#pragma once` is a list of the
  `#` of each.
- **§3.6** One row per `#include`, `#include_next`, `#import`, header
  import, `#embed` and `-include`: `#`, end, the name span as written
  in the file (case 21: the token `H`), the spelling after expansion,
  the first stage-1 expansion of a macro name, file, search-path index,
  parent instance, and where the directive is (file, command line,
  predefines). Outcomes: entered, skipped once, skipped guard (with the
  definition current at the directive), not found, depth limit, and
  four more: after the depth limit, malformed, embedded, imported.
  `#embed`'s file and index are a later row.
- **§3.2** The include context is an index of an entered inclusion;
  the key equal across units is the chain of `#` keys through the
  parent links, computed by Slip, which also maps a `Loc` to the
  identity of its file. `inclusion_of(sm, loc)` gives the instance a
  stage-1 or conditional row is in (a search over the entered bases;
  nothing is stored per row). `-include` is keyed (command line,
  ordinal).
- **Cost** on `<bits/stdc++.h>`: 115,915 conditional rows (48,536 of
  them dead, from 221 re-entries), 2.32 MB; 1,970 inclusions, about
  0.09 MB; stage 1 was 5.0 MB.

What Slip added, taken by Kiln as required items of the specification
and its tests:

1. Skipped once, skipped guard, depth limit and after the depth limit
   still carry the file found and the search index (the include graph
   has every resolved directive, entered or not); not found and
   malformed carry none. Which outcome fills which field is stated.
2. The row has the kind of the directive apart from where it is.
3. Lines and groups tile a conditional: the end of a directive's line
   is the end of its logical line, past splices and past a block
   comment that opens on it and closes later, so that from an opener's
   `#` to the end of its `#endif` line every byte is in one directive
   line or one group, live or dead.
4. `inclusion_of` is total and documented: a file `Loc` gives its
   instance (the root for the main file), a `Loc` of the predefines or
   the command line gives a value of its own, and for a macro `Loc`
   either it resolves through the expansion site or the documentation
   says the caller must.
5. Case 24 compares keys of two main files; instance numbers are local
   to a unit and are not part of what is compared.
6. A header entered twice has its rows twice, each instance's rows
   together in directive order; row indices are equal across two runs
   of one input.
7. A header-unit import whose macros come from a module file flags
   the record incomplete, as stage 1 does for definitions met from an
   image.

## 11. Kiln's phase 0 for stage 3 (2026-11-22, Kiln WP-2112)

Kiln's design for §6 item 3, reviewed and accepted by Slip. It corrects
§2 and §3.4:

- A failed paste makes two entries that are not in scratch space: the
  `##` (`InvalidPaste`) and the left operand wrapped again.
- Stage 1 already tells paste, stringise, builtin, pragma text, header
  name and invalid paste apart, and hooks `pop_macro` (`DefEnd::Popped`,
  the same `DefIx` restored: case 22 holds since stage 1).
- Every builtin, `__has_include` and the feature queries included, has
  an expansion row since stage 1; only the operand, the file and the
  search index were missing.
- Operands that short-circuit are still parsed, so their reads come
  through with no change to the evaluator.

**Made tokens** (§8 question 2): a side table keyed by the base of the
entry, not a new `ExpansionKind` (which would change `Loc`s or cost
with recording off, and the snapshot and image formats). Rows only
where stage 1 cannot derive the answer: one per paste chain (`a##b##c`
is one row with three operand locations, each asked again through
`origin()`; the intermediate has no location), and one per
`#__VA_OPT__(…)` with its tokens. No row for `#x` (the argument index
is derived), for a builtin, or for a paste with a placemarker (the
surviving token keeps its ordinary origin). An invalid paste is a
flagged row with the rejected token last. `made(sm, loc)` sits beside
an unchanged `origin()`. For classes of pasted names: `mades()`,
`made_text`, `made_buffer`; across translation units the key is the
text.

**Reads**: `reads(cond)`, 16-byte rows in evaluation order: tested
(`defined` and the `#ifdef` family), expanded (an expansion index), or
left (an identifier evaluated to 0), each with the definition bound
there or "undefined"; a flag marks a condition cut off by an error.

**Queries**: `query(exp)` keyed by the builtin's expansion: operand
text, value, the name's range, and for `__has_include` and
`__has_include_next` the file and the lookup, in the vocabulary of
inclusion rows.

**Nesting**: `cond_at(loc)` and `enclosing(c)`; Slip composes the
configuration statement of an edit from them and the reads.

**Push and pop**: `stack_events()`: push, pop, unmatched pop, with the
definition saved or restored, the one it replaced, and the link between
a push and its pop. The key is the `#`, or through `_Pragma` the
`_Pragma`'s expansion key (libc++ has five `_Pragma` pushes in one
macro body).

Cost measured by Kiln: 1.56 MB more on `<bits/stdc++.h>` (18% of the
record); 1.97 MB more on clang's `SemaExpr.cpp` (33,000 pastes, 6,500
stringisings, 46,000 reads).

Decisions: `made()` beside `origin()`; no "short-circuited" bit on reads
for now (every read with its binding is the sound precondition; a spare
flag bit is kept); nesting from Kiln, not derived by Slip; a popped
definition's end carries the key of its pop event; expanded reads are
rows; a placemarker paste makes no row.

Slip's additions, accepted as items of the specification and its tests:
a tested or left read carries the location of the identifier it read
(macro renaming edits exactly those tokens, the ones bound to
"undefined" included); which expansions get an expanded row is stated;
one tested row for each of the `#ifdef` family; a made row is reachable
from its expansion with its position in the body, so that a site has a
key across translation units; an operand that is an argument is the one
token pasted, at its location before substitution; `cond_at` answers
for every file location, bytes of skipped groups included, per
inclusion instance; queries from a macro and outside a condition; a
push of an undefined name; a completeness check (every scratch-space
token has a derivation or a made row) on `<bits/stdc++.h>` and
`SemaExpr.cpp`; the acceptance cases 5, 6, 10, 17, 20, 22, 23 of §5 and
ten more.
