# KS2 amendment 2 — what the review of Slip's session found

A request from Slip to Kiln. Status: **sent 2026-11-22 and scheduled by
Kiln the same day** (its package numbers are in the table).
Amends [KS2](KS2-eval-session.md). Found by the independent review of
`docs/wp/WP-080.md` (`slip_kiln::session`) against Kiln main `83566cc47`
and `913a490bb`.

## Design or implementation

| Part | Kind | Kiln |
|---|---|---|
| §1 No response-file expansion of a client's words | **Implementation.** | WP-2411 |
| §2 A run uses the calling thread's stack guard | **Implementation**; a defect against Kiln's `spec/eval-session.md` §9. | WP-2410, first |
| §3 A session lists its consulted resources | **Implementation.** What `FrontEnd::consulted` does, for a session. | WP-2412 |
| §4 The call stack of an uncaught exception | **Design, small.** It departs from the shape of clang's P3068 note. | WP-2413 |

## 1. No response-file expansion of a client's words

`with_eval_session` expands every `@file` word
(`kiln/crates/kiln_driver/src/session.rs:81`, `rsp.rs:15-41`). A client
that has validated its words one by one then does not know which
options and files reach the driver: the value of `-D @opts.rsp` became a
word `@opts.rsp`, an option `-std=c++20` inside the file took effect,
and the file is in no buffer of the session.

Requested: the command line of a session is words already split, with
no `@file` expansion, or an option that turns it off. A word `@x` is
then an ordinary word.

Until then Slip refuses every word, value and path that begins with `@`
(`docs/wp/WP-080.md`).

## 2. A run uses the calling thread's stack guard

Sema copies the stack guard when it is created
(`kiln_sema/src/templ/mod.rs:617`, through `StackGuard::current()`,
`stack_guard.rs:71-75`) and the check reads the copy (`:163`). A guard
installed on one thread and checked on another reads as exhausted. So a
run on a thread other than the compiling one fails "stack exhausted" at
the first stack check it reaches (instantiation during a run,
`templ/stack.rs:496`; a nested evaluation, `oven_bridge.rs:636`, `:740`,
`:776`), and the flag `inst.stack_exhausted` stays set, so every later
run of the session fails too.

Slip compiles a transform once in a worker and runs it from the
front-end thread of each subject unit, so this is its ordinary case.
The review showed the arithmetic of the guard; it reached no stack
check in a run end to end, so the reproducer is Kiln's to write.

Requested: during `EvalSession::run`, Sema's guard is the calling
thread's, restored afterwards. Accepted when a session compiled on
thread A whose run instantiates a template on thread B succeeds, and a
run on A succeeds after it.

Until then: a hazard recorded in `spec/landed/api/WP-080.md`.

## 3. A session lists its consulted resources

The resources of `#embed` and the hits of `__has_embed` reach a run's
bytes and are in no buffer of the session, so a client cannot put them
in a transform's identity (`spec/api.md` §2.2).
`FrontEnd::consulted` has them for a subject unit
(`kiln_driver/src/front_end.rs:120`).

Requested: `SessionFrontEnd` exposes the same list, paths with their
content digests.

Until then Slip refuses `#embed` and `__has_embed` in a transform.

## 4. The call stack of an uncaught exception

The diagnostic of an uncaught exception has no call stack
(`kiln_oven/src/notes.rs:336-341`, clang's shape). The author's first
question about a failed run is where it was thrown. The frames are
intact when `Fault::Uncaught` is raised (`kiln_oven/src/vm/exc.rs:218-220`).

Requested: in a run of an evaluation session, the diagnostic carries the
call stack at the throw as "in call to" notes, under the backtrace
limit of other faults. Kiln's decision: in runs of a session only; a
compile's output keeps clang's shape.

## Housekeeping

- [KS2 amendment 1](KS2-amendment-1.md) §2 (`__TIMESTAMP__`) is met at
  the pin (`kiln_driver/src/pp_cmd.rs:309-312`).
- A handler that need not be `Send` (Kiln WP-2321) is no longer waited
  for: Slip's subject is `Sync` (`docs/wp/WP-083.md`, `docs/wp/WP-089.md`).
  That rests on `Ast: Sync`, which Kiln asserts at compile time from its
  WP-2320.
