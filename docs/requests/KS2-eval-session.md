# KS2 — the retained evaluation session: Slip's constraints and acceptance tests

Sent to the Kiln agent 2026-11-22, in answer to the defaults of Kiln
WP-1995 (a session opened from a finished translation unit; a named
`consteval` entry evaluated many times, each run isolated from the
post-TU state with template instantiations kept as caches; an entry
with no parameters returning `void` or an integral type; per-run step
and Oven-heap budgets with a program-sized default; a `Send + Sync`
cancel token checked at back-edges and calls; host calls allowed in
runs; errors as structured diagnostics; the session `Send`). Kiln owns
the design ([05](../design/05-kiln-interface.md) §3, KS2); these are
what Slip needs from it.

How Slip uses it ([01](../design/01-transform-api.md) §2): a worker
compiles the transform once, then evaluates an entry once per observer,
tens of thousands of times in a worker's life. The run's inputs are the
host calls of that observer. Which worker gets which observer, and in
what order, is scheduling: **nothing a run reports may depend on the
runs before it.**

## Agreed as proposed

The entry contract (no parameters; `void` or an integral result;
everything else through host calls: subject entities live in another
universe and are integers to the transform, so `std::meta::info`
arguments are not wanted). The cancel token. The session being `Send`
(it need not be `Sync`: one per worker).

## Requested

1. **The handler is per run.** `evaluate(entry, &mut dyn HostCalls,
   budget, &cancel)`, the handler borrowed for the call and not
   `'static`: Slip's handler borrows the subject of one observer (another
   front-end state, shorter-lived than the session) and collects that
   observer's edits. A handler registered per session would force
   interior mutability and a lifetime the session cannot name.
2. **Several entries in one session**, each named by a `DeclId` of the
   session's AST (Slip finds them itself), with a helper from a
   qualified name that reports "not found", "overloaded", "not
   `consteval`", "has parameters", "wrong return type" as distinct
   errors. A transform has a map entry and may have a reduce entry.
3. **History-independent accounting.** The step count a run reports,
   and whether it exhausts its budget, are functions of the run, not of
   the session's past: a call answered from a memo entry is charged what
   it cost when it was computed (or memo entries are not kept across
   runs; Kiln's choice). Slip gates on evaluator steps as a
   deterministic counter, and a transform must not succeed on one worker
   and exhaust its budget on another.
4. **History-independent diagnostics.** A run that would instantiate a
   template whose instantiation failed (or warned) in an earlier run
   gets the same diagnostics the earlier run got.
5. **A run that fails leaves the session usable**: an error, a budget
   exhausted, a cancellation, a host error, an uncaught C++ exception.
   The next run behaves as in a fresh session.
6. **The run's report**: the result; steps; host calls made; peak Oven
   heap of the run; the diagnostics, each with its location in the
   transform's source and the evaluation's call stack as notes; for an
   uncaught exception its type and, if it derives from `std::exception`,
   `what()`. (Steps per function are KS3 and not needed here.)
7. **Memory is observable and bounded.** After a run, the Oven heap the
   run allocated is released. What the session keeps (instantiations,
   memo entries) has a size Slip can ask for, and a call that drops the
   memo entries, so that a worker can hold its footprint flat over
   fifty thousand runs.
8. **Opened from a command line.** The transform's translation unit
   needs the driver's include paths, predefines and resource directory
   (`<slip/meta.h>`, the pinned libc++). Slip's KF1 prototype (branch
   `slip/kf1`, sha to follow) is `run_front_end(argv0, command_line,
   options, f)`: a closure over the finished front end, with
   `FrontEnd` `#[non_exhaustive]` so that a session can be added. KS2
   touches the same function (`SyntaxJob::parse`). Wanted: a way to get
   a session from the same entry (the closure receiving the session, or
   a sibling entry that returns one), not only from parts assembled by
   hand.
9. **No session from a translation unit with errors**: refused with a
   diagnostic.

## Acceptance tests

With the test handler of KS1's table (op 1 returns a counter it
increments; op 2 echoes; op 3 is `Err("boom")`).

| # | Case | Expected |
|---|---|---|
| 1 | an entry that calls op 1 three times, run twice with two handlers | each handler sees 0, 1, 2; nothing of the first run reaches the second |
| 2 | an entry that fills a `std::vector` with 10⁶ elements and returns its size, run 1000 times | every run the same result and the same reported steps; the session's kept size after run 1000 equals that after run 2 |
| 3 | a pure `constexpr` helper called by the entry (fib(25)), run twice | the same reported steps both times, whether or not the second run used a memo entry |
| 4 | a step budget one below what case 3 reports, then exactly it | exhausted; succeeds; the same on a fresh session and on one that ran case 3 before |
| 5 | an entry that instantiates a template whose body has an error (and one that only warns), run twice | the same diagnostics both times |
| 6 | runs that fail by: `throw` uncaught; op 3; budget; cancellation set by the handler during op 1; an out-of-bounds read | each a structured error naming its kind; after each, case 1 gives its expected result |
| 7 | `for(;;){}` with the token cancelled from another thread after 10 ms | the run ends within a few milliseconds of it; the session is usable |
| 8 | a heap budget smaller than the vector of case 2 | a "heap budget" error, not an abort of the process |
| 9 | two entries in one translation unit, run alternately | independent |
| 10 | the entry reads a namespace-scope `constexpr` variable and a `constinit` variable with a mutable member | the first is readable; modifying anything with static storage is not a constant expression, as in a compile |
| 11 | lookup by name of: a missing function, an overload set, a non-`consteval` function, one with a parameter, one returning a class | five distinct errors |
| 12 | a session moved to another thread between runs | works |
| 13 | 50,000 runs of an entry that makes 100 host calls of op 2 | measured and reported: runs per second, and the fixed cost of an empty run. Slip's budget assumes an empty run costs microseconds, not a re-parse |

## Not needed from KS2

Entry parameters. A handler per session. `Sync`. Carrying anything
across sessions. Evaluating in parallel inside one session.
