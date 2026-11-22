# What landed in Kiln for the transform universe (2026-11)

A reading of three Kiln work packages against what Slip asked for. Nothing
here was run: every statement is from the code and Kiln's specifications at
Kiln main `0295475a3`, and each one carries a place to check it. Paths are
relative to the Kiln repository root, written `kiln/...`.

Not normative. The API specification that depends on these facts is
[spec/api.md](../../../spec/api.md).

## 1. Summary for a reader outside the project

A Slip transform is a C++ program that the tool runs at compile time, inside
Kiln's constant evaluator, once for every source file being refactored. Three
things were needed from Kiln before any of that could be built, and all three
are now on Kiln's main branch.

1. **A way for the running C++ program to ask the tool a question**
   (WP-1990). Kiln added one built-in function. The program passes a number
   that says which question, some integers and some bytes; the tool answers
   with integers and bytes. It is what Slip asked for, including the three
   corrections Slip made to the first proposal. Kiln also drew a careful line
   around where such a call may appear: only in code that really is being
   executed, never in the places where a compiler quietly evaluates something
   "to see whether it works". That line is the right one, and the Slip API
   adopts it as a rule for transform authors.

2. **A way to compile the transform once and run it many times**
   (WP-1995). A compiled transform can be run again and again, each run with
   its own limits on work and memory, its own answers from the tool, and no
   memory of the previous run. Kiln measured an empty run at about 75
   nanoseconds and a question to the tool at about 90 nanoseconds. Both are
   small enough that Slip does not have to be clever to be fast: a transform
   that asks fifteen questions about every call site of a function costs about
   two microseconds per site.

3. **A record of every name as the programmer wrote it** (WP-1930, and its
   second stage WP-1931). For each name in a source file Kiln now records
   which characters are the name and which declaration it resolved to. Names
   in expressions, member accesses, qualifiers, types, templates, base classes
   and constructor initialisers are covered. Three later stages are not built:
   names in `using` declarations and similar places, names inside templates
   that only resolve when the template is used, and names that arrive through
   precompiled headers.

What this means for the plan: the first end-to-end demonstration of a
programmable transform (replace `f(a, b)` by `g(b, a)` across LLVM) needs
nothing more from Kiln's design. It needs one piece of integration that is
already in progress (Slip builds against an older Kiln branch, and the entry
point Slip uses to parse a file has not yet been merged to Kiln's main branch)
and five small additions that are plain engineering: a switch to turn the
name record on from a library call, two position helpers, a thread-safety
guarantee, and a check that expression extents are reliable. A rename that is
correct in every corner (the namespace migration) does need the unbuilt
stages of the name record, as already planned.

Three findings deserve attention before code is written. First, the way the
tool must answer questions requires data from the file under refactoring to
be shared across a thread boundary, and Kiln does not yet promise that its
syntax tree may be shared that way. Second, a failure deep inside Kiln while
compiling a transform is reported to Slip as "bad command line", which would
send an author looking in the wrong place. Third, Kiln's open work package
WP-2073 would let template instantiation inside a run reach the tool; Slip
wants the opposite to stay true, because it is what keeps a transform's
result independent of what was evaluated before it.

## 2. The state of Slip's Kiln revision

(Since WP-044 Slip pins Kiln's `main` at `83566cc47`, which has WP-2120;
this section is the state when the note was written.)

Slip pinned Kiln at `62d190007`, the head of Kiln's branch `slip/kf1`. That
commit is not an ancestor of Kiln main. The front-end entry Slip uses
(`run_front_end`, request KF1) exists only on that branch:
`crates/kiln_driver/src/front_end.rs` is present at the pin and absent on
main. Kiln's WP-2120 is the package that lands it on main.

So "move Slip's Kiln revision to main" has a precondition, WP-2120, and until
then no Slip code can use the host call, the session or the stage 2 entries
together with `run_front_end`. Work that needs none of Kiln (the operation
table and wire encoding against a fake subject, the header text) is not
blocked.

## 3. WP-1990: the host call (`6d052b07a`)

### 3.1 Interface

Specified in `kiln/spec/host-calls.md`.

The built-in, visible to C++ only while a handler is registered
(`__has_builtin` answers 1 or 0 accordingly):

```cpp
unsigned long long __builtin_kiln_host_call(
    unsigned op,
    const unsigned long long *in, unsigned n_in,
    const char *bin, unsigned n_bin,
    unsigned long long *out, unsigned cap_out,
    char *bout, unsigned cap_bout);
```

The Rust side, `kiln/crates/kiln_oven/src/host.rs`:

| Item | Place |
|---|---|
| `pub trait HostCalls: Send` | `:13` |
| `fn call(&mut self, op: u32, ints: &[u64], bytes: &[u8], out: &mut HostOut<'_>) -> Result<u64, HostError>` | `:22-28` |
| `HostError` (`new`, `message`) | `:33`, `:39`, `:44` |
| `HostOut` (`new`, `int_capacity`, `byte_capacity`, `write_ints`, `write_bytes`, `ints_written`, `bytes_written`) | `:62`, `:73`-`:110` |
| `HostMode` (allowed, silent, speculative) | `:118` |
| `Oven::set_host_calls`, `take_host_calls`, `has_host_calls`, `host_call_count`, `host_enter` | `:169`, `:177`, `:182`, `:187`, `:196` |

The evaluator's side is `exec_host_call`,
`kiln/crates/kiln_oven/src/vm/host.rs:38`.

### 3.2 What Slip asked for and got

[KS1](../../requests/KS1-host-call.md) asked for one variadic-free builtin
with integer and byte arguments and results, a handler trait, and three
changes to Kiln's first proposal. All three are in the code:

| Asked | In the code |
|---|---|
| A handler `Err` cannot be caught by the C++ program | `kiln/crates/kiln_oven/src/vm/host.rs:136-138`: the evaluation fails with "host call failed: ..." |
| The return value is uninterpreted and the two output capacities are passed | `:150-158`; capacities reach the handler through `HostOut` |
| Every pointer and length is checked before the handler runs | `:82-84`, `:119-122`, `host_range` `:179`, `host_read` `:190`, `host_writable` `:223` |

Also there: evaluation steps are charged for a call (`1 + n_in + n_bin`,
`:112`) and for each stored result (`:158`), so a run's step count stays a
deterministic function of what the program did; the cancel token is checked
before every call (`:72`); inputs are read before outputs are written, so an
output may overlap an input (`:119-122`).

### 3.3 Limits

| Limit | Value | Place |
|---|---|---|
| Operation number | 32 bits | `vm/host.rs:82` |
| Each length and capacity | 32 bits | `vm/host.rs:82-84` |
| Integer arguments and results | `unsigned long long` only | builtin signature |
| Byte arguments and results | `char` only, one buffer each way | builtin signature |
| Output storage | not const, not a string literal, lifetime began inside the evaluation | `vm/host.rs:223-245` |
| Writing more than the capacity | the call fails (uncatchable) | `vm/host.rs:141-149` |
| No handler | "no host-call handler is registered" | `vm/host.rs:75-77` |
| Cost | about 88 ns per call measured in a session (50,000 runs of 100 calls in 0.44 s) | `kiln/spec/eval-session.md` §10 |

An output buffer may be an uninitialised local array, a `std::vector`'s
storage or a `std::string`'s
(`kiln/tests/conformance/ext.kiln.host_call/host-call-buffers.cpp`).

**Memoisation.** The Oven caches calls of functions with at most four
parameters of integral, boolean, floating or `std::meta::info` type and a
scalar result. A call during which a host call ran is never cached
(`kiln/spec/oven.md:2293-2332`). Two consequences for the API: a wrapper
function around a host call is re-evaluated every time (correct, and what
Slip needs), and a class that wraps a handle is not a scalar, so pure helper
functions over Slip handles do not get the cache either.

**Where a call may be evaluated** (`kiln/spec/host-calls.md` §4):

| Class | Contexts | Result |
|---|---|---|
| Runs | Manifestly constant-evaluated contexts that must produce a constant; the body of the session's entry and everything it calls | the handler is called |
| Refused silently | Trial folds (the evaluator trying an expression that need not be constant) | the trial fails quietly; `vm/host.rs:48-54` |
| Speculative error | The constant-initialisation check of a static-storage variable, the hidden second evaluation for `int a[n]`, `__builtin_constant_p`, substitution checks, `enable_if`-style conditions | hard error `oven_host_call_speculative`, once per call stack; `vm/host.rs:55-71` |

A block-scope `constexpr` variable whose initialiser makes a host call runs
the call when the definition is evaluated.

A translation unit with a handler registered cannot be written as a
precompiled header or module.

### 3.4 Missing, and hazards

1. **`HostCalls: Send` against a borrowed subject.** `EvalSession::run`
   takes `&mut dyn HostCalls` (`kiln/crates/kiln_sema/src/session.rs:486-492`)
   and the trait has `Send` as a supertrait (`host.rs:13`). Slip's handler
   for one run borrows the subject unit: `&Ast`, `&SourceManager`.
   `&T: Send` needs `T: Sync`. `SourceManager: Send + Sync` is asserted
   (`kiln/crates/kiln_source/src/manager.rs:1614-1615`); nothing asserts
   `Ast: Sync`. If `Ast` is not `Sync` today the handler does not compile; if
   it is, nothing keeps it so. Either Kiln asserts `Ast: Sync`, or the
   per-run handler loses the `Send` bound (it never leaves the thread).
   Drafted as [KS1 amendment 1](../../requests/KS1-amendment-1.md).
2. **A handler cannot hold the subject's Oven.** The Oven has interior
   mutability and is not `Sync`. No v0 query needs it. A later query that
   evaluates a subject constant (`constant_value_of`) needs a different
   arrangement: the handler would own the subject unit for the run rather
   than borrow it.
3. **Host mode after a contained panic** is not restored (Kiln's open
   WP-2001). Slip treats a panic in the worker as the failure of the observer
   and the worker (`spec/exec.md` §5), so this does not reach a result.
4. **Lazily evaluated constexpr variable templates** are not audited for a
   missing or doubled speculative-call error (Kiln's open WP-2000). The API
   forbids host calls outside the run's own evaluation, so a transform that
   follows the rule cannot hit it; a transform that breaks the rule may get a
   poorer diagnostic.
5. **Throughput ceiling.** About 7 to 11 million calls per second per core.
   Kiln keeps an integer-only fast path in reserve (open WP-2002) "only if a
   tooling client needs more". The v0 API is sized so that it does not
   (`spec/api.md` §9).

## 4. WP-1995: evaluation sessions (`e2c977409`)

### 4.1 Interface

Specified in `kiln/spec/eval-session.md`.

`kiln/crates/kiln_sema/src/session.rs`:

| Item | Place |
|---|---|
| `EvalSession<'a, 'sm>` | `:28` |
| `OpenError::TuHasErrors` | `:53` |
| `Entry` | `:76` |
| `EntryError`: `NotFound`, `Overloaded`, `NotAFunction`, `NotConsteval`, `HasParameters`, `WrongReturnType`, `Undefined` | `:91` |
| `RunReport { result, steps, host_calls, peak_heap, calls, diagnostics }` | `:140-153` |
| `KeptMemory` | `:157` |
| `EvalSession::open` | `:337` |
| `sema`, `take_tu_host_calls`, `into_sema` | `:350`, `:356`, `:362` |
| `entry(DeclId)`, `entry_named(&str)` | `:376`, `:426` |
| `pub fn run(&mut self, entry: Entry, host: &mut dyn HostCalls, budget: RunBudget, cancel: &CancelToken) -> RunReport` | `:486-492` |
| `kept`, `clear_memo` | `:548`, `:563` |

`kiln/crates/kiln_oven/src/session.rs`:

| Item | Place |
|---|---|
| `CANCEL_INTERVAL = 65_536` steps | `:26` |
| `RunBudget { steps, bytes, depth }`; `RunBudget::PROGRAM = { 1 << 34, 1 << 30, 16_384 }` | `:34`, `:45` |
| `CancelToken` | `:75` |
| `RunValue` | `:104` |
| `RunError`: `Failed`, `StepBudget`, `MemoryBudget`, `DepthLimit`, `Cancelled`, `Host { message }`, `Uncaught { type_name, what }` | `:120` |
| `session_run` | `:235` |

`kiln/crates/kiln_driver/src/session.rs`:

| Item | Place |
|---|---|
| `SessionFrontEnd { session, sources, main }` (non-exhaustive) | `:27` |
| `SessionRefused { CommandLine, TuHasErrors { errors } }` | `:39` |
| `pub fn with_eval_session<R>(argv0: &str, command_line: &[String], io: Io, f: impl for<'a, 'sm> FnOnce(Result<SessionFrontEnd<'a, 'sm>, SessionRefused>) -> R) -> R` | `:74-79` |

### 4.2 What Slip asked for and got

[KS2](../../requests/KS2-eval-session.md) made nine requests. All nine are
met:

| Asked | In the code or its tests |
|---|---|
| A handler per run, borrowed, not owned by the session | `run`'s `host: &mut dyn HostCalls` |
| Several entries in one unit, each failure distinguishable | `EntryError`, `session.rs:91` |
| A run's step count and diagnostics do not depend on earlier runs | `kiln/crates/kiln_parse/tests/eval_session.rs:199` (`steps_do_not_depend_on_memoized_results`), `:399` (`instantiation_diagnostics_are_reported_by_every_run`) |
| A failed run leaves the session usable | `:260` (`every_failure_kind_leaves_the_session_as_fresh`) |
| A report: result, steps, host calls, peak memory, diagnostics | `RunReport` |
| Budgets per run and cancellation from another thread | `RunBudget`, `CancelToken`; `:221`, `:320` |
| Memory kept by the session is observable and does not grow with runs | `kept()`; `:437` (`kept_memory_is_flat`) |
| Opened from a command line | `with_eval_session` |
| Refusal when the unit has errors | `OpenError::TuHasErrors`, `SessionRefused::TuHasErrors` |

### 4.3 What differs from the request

- **The entry's shape is narrower than the request allowed for.** An entry is
  a `consteval` function with no parameters returning `void` or an integral
  type (`EntryError::HasParameters`, `WrongReturnType`, `NotConsteval`).
  Parameters, class results and `constexpr` entries are Kiln's open WP-2070,
  WP-2071 and WP-2072. The API is designed so that it needs none of them.
- **The unit's own handler is a placeholder.** While the transform unit is
  compiled, every host call fails with "host calls are served only in
  evaluation-session runs" (`kiln/crates/kiln_driver/src/session.rs:54-66`).
  A transform therefore cannot query the subject from a namespace-scope
  initialiser or a `static_assert`. Slip wants exactly this.
- **Nested evaluations run outside the run.** A constant expression that
  template instantiation evaluates during a run runs under the translation
  unit's limits, counted from zero, cannot be cancelled, and has no handler
  (`kiln/crates/kiln_oven/src/session.rs:374`; `:334`).

### 4.4 Limits and measurements

From `kiln/spec/eval-session.md` §10 (Kiln's own measurements, release
build; the measuring test is `kiln/crates/kiln_driver/tests/eval_session.rs:322`):

| Quantity | Value |
|---|---|
| An empty run | 71 to 76 ns |
| The first run of an entry | 14 to 30 µs |
| Parse and open, a small transform | 49 ms |
| Parse and open, a transform including `<bits/stdc++.h>` | 1.16 s, 60 MiB kept |
| 50,000 runs of 100 host calls | 0.44 s |
| Cancellation latency | under 1 ms (checked every 65,536 steps and before each host call) |
| Memory kept across runs | flat |

`with_eval_session` runs the closure on the calling thread and installs no
stack guard. The guard is thread-local and starts disabled, "every check
passes" (`kiln/crates/kiln_sema/src/stack_guard.rs:50-60`); the installer is
public (`:92`). Slip must run the session on a thread it sized and guarded
itself, as it already does for subject units.

### 4.5 Missing, and hazards

1. **A failed parse is reported as a bad command line.** When the front end
   fails for a reason other than errors in the unit,
   `with_eval_session` returns `SessionRefused::CommandLine`
   (`kiln/crates/kiln_driver/src/session.rs:143-146`). An author whose
   transform hits an internal limit would be told the command line is wrong.
   Asked for in [KS2 amendment 1](../../requests/KS2-amendment-1.md).
2. **No way to list what the transform unit read.** The identity of a
   compiled transform must cover every header it included. `SessionFrontEnd`
   exposes `sources`; Slip can walk the source manager's files, as it does
   for subject units. To be confirmed in WP-080, not a request.
3. **`__DATE__`, `__TIME__` and `__TIMESTAMP__`** are live in a transform
   unit (`kiln/crates/kiln_pp/src/builtin.rs:59-60`). They break "the same
   transform gives the same edits". Kiln can pin the first two
   (`kiln/crates/kiln_pp/src/options.rs:118-120`; the driver reads
   `-fkiln-reproducible-time`, `kiln/crates/kiln_driver/src/time.rs:4-15`),
   so Slip passes that flag for the transform unit; `__TIMESTAMP__` then
   depends on a file's modification time (`options.rs:121-123`) and needs
   the amendment.
4. **Replay keys by address.** Reflection queries that take addresses are
   keyed by address when a run replays instantiation-time work (Kiln's open
   WP-2074). A transform that uses `<meta>` on its own types could see
   run-to-run differences. The v0 header does not use `<meta>`; the API marks
   its use in a transform as outside the determinism guarantee until WP-2074
   lands.
5. **WP-2073 would change a property Slip relies on.** It proposes to run
   nested evaluations under the run's handler and limits. Then a template
   instantiated in one run and reused in the next would have made its host
   calls in the first run only. Slip's rule (subject data is never reachable
   from a nested constant expression) is what makes every run independent of
   the runs before it. The amendment asks that this stay refusable.
6. **No diagnostic renderer in the public interface.** `RunReport.diagnostics`
   is a list of Kiln diagnostics; the tests render them with a private helper
   (`kiln/crates/kiln_parse/tests/eval_session.rs:82`). Slip must show them
   to the author with file, line and the call-stack notes.

## 5. WP-1930 and WP-1931: the written-reference table (`121dfb7cb`)

### 5.1 Interface

Specified in `kiln/spec/ast.md` §18 (`:1650-1919`).
`kiln/crates/kiln_ast/src/written_refs.rs`:

| Item | Place |
|---|---|
| `WrittenRefId` | `:24` |
| `RefRole` | `:44` |
| `RefQualifier` | `:103` |
| `REF_DEPENDENT`, `REF_UNRESOLVED` | `:138`, `:140` |
| `WrittenRef { range: Span, entity, found, enclosing: Option<DeclId>, qualifier, role, flags }`, 28 bytes | `:144-161` |
| `WrittenRefs`: `is_on`, `get`, `iter`, `enclosed_by`, `dump` | `:174`, `:185`, `:205`, `:210`, `:215`, `:324` |

Recording is switched by `ParseOptions::written_refs`
(`kiln/crates/kiln_parse/src/lib.rs:140`), which only the driver flag
`-fkiln-dump-written-refs` sets
(`kiln/crates/kiln_driver/src/args.rs:2032`,
`kiln/crates/kiln_driver/src/syntax_cmd.rs:383`).

### 5.2 What Slip asked for and got

[KR1](../../requests/KR1-written-reference-table.md) proposed five stages.

| Stage | Content | State |
|---|---|---|
| 1 | Table, iteration, dump; expression names, member names, qualifier components with the chain | on main (WP-1930) |
| 2 | Type names, template names, base specifiers, member-initialiser ids | on main (WP-1931) |
| 3 | Using-declarations and -directives, namespace alias targets, declarator-id qualifiers, elaborated and friend references, specialisation targets, concept names, designators | open (WP-1932) |
| 4 | Per-specialisation entries for dependent names | open (WP-1933) |
| 5 | Carriage in precompiled headers and modules; identifier accounting on the corpora | open (WP-1934) |

The agreed shape is what was built: `entity` is the function overload
resolution selected (a specialisation, for a template), `found` is what
lookup found when that differs (the template, or a using-shadow),
`enclosing` is the lexical declaration, `qualifier` chains to the entry on
the left or marks the global scope. A tentative parse that is abandoned
leaves no entry.

### 5.3 What differs, and what is missing

1. **No library switch that only records.** The one way to turn recording on
   is the dump flag, and that flag also appends the dump to the output
   (`kiln/crates/kiln_driver/src/syntax_cmd.rs:647-648`). `run_front_end`
   needs an option that records and prints nothing. Asked for in
   [KR1 amendment 1](../../requests/KR1-amendment-1.md).
2. **No link from an entry to the expression that owns it.** A call's callee
   name is a `DeclRefExpr`; the entry for the same name is found only by
   matching the expression's location to the entry's `range.lo`. That the
   two are equal is not stated in `kiln/spec/ast.md` §18. The amendment asks
   for the statement and a test, not for a new field.
3. **`enclosed_by` is a scan of the whole table**
   (`written_refs.rs:215`). Slip builds its own index once per unit.
4. **The end of a token is not available after the unit is built.**
   `Sema::token_end_of`
   (`kiln/crates/kiln_sema/src/written_refs.rs:284`) is a method of the
   semantic analyser. `WrittenRef.range` already holds both ends for names;
   for expressions Slip needs the end of the last token from the source
   manager alone.
5. **Expression extents are best effort.** `Ast::end_loc`
   (`kiln/crates/kiln_ast/src/ast.rs:122`) falls back to the node's own
   location when no closing token was stored (comment at `:116-121`). A call
   stores its `)` (`kiln/crates/kiln_sema/src/expr.rs:7486`), so a call's
   extent is exact; an argument's extent depends on the kind of expression
   it is. An extent that is too short would make a transform copy half an
   argument. Verification catches a wrong result, but the API should refuse
   instead of guessing: [KE1](../../requests/KE1-expression-extents.md) asks
   for an extent that is either exact or absent, with a differential test.
6. **Dependent names.** In a template pattern a dependent name has no entity
   and the `DEPENDENT` flag; instantiation adds nothing (stage 4). A
   transform sees no reference to `f` at `f(t)` inside a template. For
   example 9.1 this means call sites in template bodies are found only when
   the callee is not dependent. The API states the gap, reports a count of
   dependent names that match by spelling, and promises only that coverage
   grows.
7. **Names from precompiled headers and modules** are absent (stage 5).
   LLVM's build uses neither.
8. **Stage 3 roles matter for rename, not for the first demonstration.** A
   `using lib::foo;` is not a reference today, so a rename driven from C++
   misses it and verification fails the result (lookup of `foo` no longer
   finds anything). That is the correct outcome and it is the measurement of
   Demo 3's second part.

## 6. What each milestone needs from here

| Need | For | State |
|---|---|---|
| `run_front_end` on Kiln main | everything that touches a subject unit | Kiln WP-2120, in progress |
| Recording switch on `FrontEndOptions` | references, calls | KR1 amendment 1; implementation |
| Entry-to-expression location guarantee | calls | KR1 amendment 1; a sentence and a test |
| `Ast: Sync` or a non-`Send` handler | the host | KS1 amendment 1; implementation |
| Refusal reasons of `with_eval_session`; pinned `__TIMESTAMP__`; a diagnostic renderer | author diagnostics, transform identity | KS2 amendment 1; implementation |
| Nested evaluations stay away from the host | isolation | KS2 amendment 1; design, with Kiln's WP-2073 |
| Exact-or-absent expression extents | arguments of a call | KE1; implementation, with one design question |
| Token end and file range after the unit | every range | request KF3 (already planned); implementation |
| Macro provenance of a range: argument or body | tier 1 edits | request KR4 (sent); not needed for Demo 3's sites |
| Stages 3 to 5 of the table | rename completeness, namespace migration | Kiln WP-1932 to WP-1934 |
| Entries with parameters, class results | nothing in v0 | Kiln WP-2070, WP-2071; not asked |
