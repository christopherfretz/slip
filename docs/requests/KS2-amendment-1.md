# KS2 amendment 1 — sessions as Slip will use them

A request from Slip to Kiln. Status: **sent 2026-11-22.**
Amends [KS2](KS2-eval-session.md). Found while specifying `spec/api.md` §2
against Kiln main `0295475a3`; the reading is in
[kiln-landed-2026-11.md](../design/notes/kiln-landed-2026-11.md) §4.

## Design or implementation

| Part | Kind |
|---|---|
| §1 Why a session was refused | **Implementation.** More cases of an existing enumeration. |
| §2 A fixed `__TIMESTAMP__` | **Implementation.** One more pinned value beside two that exist. |
| §3 Rendering a run's diagnostics | **Implementation.** Making public what the driver already does. |
| §4 Nested evaluations and the host | **Design without prior art.** It is the question Kiln's own WP-2073 leaves open, and Slip's API has taken a position on it. Hand to whoever designs WP-2073. |
| §5 Reflection replay keyed by content | **Design**, already open in Kiln as WP-2074; listed so that the dependency is on record. Slip asks for nothing new. |

## 1. Why a session was refused

`with_eval_session` reports `SessionRefused::CommandLine` both for a bad
command line and when the front end stopped for another reason
(`kiln/crates/kiln_driver/src/session.rs:143-146`). The author of a
transform is then told that a command line Slip wrote is wrong.

Requested: distinguish, in `SessionRefused`,

- the command line was refused (with the driver's message);
- the unit has errors (exists: `TuHasErrors { errors }`), **with the
  diagnostics**, as records and not only a count, so that Slip can show
  them the way it shows a run's;
- the front end stopped for another reason (a fatal diagnostic, an
  internal limit), with what it reported.

A panic inside the front end should reach the caller as a panic or as a
fourth case; either is fine, provided it is stated.

## 2. A fixed `__TIMESTAMP__`

`-fkiln-reproducible-time` pins `__DATE__` and `__TIME__`
(`kiln/crates/kiln_driver/src/time.rs:4-15`,
`kiln/crates/kiln_pp/src/options.rs:118-120`). `__TIMESTAMP__` is the
file's modification time unless `PpOptions::timestamp` is set
(`options.rs:121-123`), and the driver flag does not set it.

Requested: under `-fkiln-reproducible-time`, `__TIMESTAMP__` is the fixed
spelling for the same epoch. Slip's rule is that a transform's result is a
function of file contents (`spec/api.md` §2.7); a modification time is not
content.

## 3. Rendering a run's diagnostics

`RunReport::diagnostics` is a list of `kiln_diag::Diagnostic`
(`kiln/crates/kiln_sema/src/session.rs:140-153`). Kiln's tests turn them
into text with a private helper
(`kiln/crates/kiln_parse/tests/eval_session.rs:82`).

Requested: a public function that renders those records against the
session's source manager exactly as the driver renders diagnostics to
standard error (text and JSON forms), notes and call stack included. Slip
must show a transform's failure to its author in the form the author's
editor already understands, and must not grow a second renderer.

Also requested, smaller: when an exception leaves the entry,
`RunError::Uncaught` gives the type's name and `what()` for a type
derived from `std::exception`
(`kiln/crates/kiln_oven/src/session.rs:138-143`). `slip::error` is
deliberately not derived from it, and carries an enumeration and one
integer: the ordinal of the host's failure. Let `Uncaught` also carry the
values of the object's first non-static data members of integral or
enumeration type, in declaration order, at most four
(`scalars: Vec<u64>`). With it Slip names the exact
refusal; without it Slip reports the most recent one, which is wrong for
a transform that stored an error and threw it later.

## 4. Nested evaluations and the host (design)

Today a constant expression that template instantiation evaluates during a
run has no handler; a host call there fails with "no host-call handler is
registered" (`kiln/spec/eval-session.md` §6). Kiln's open WP-2073 proposes
to run such evaluations "under the run's handler and limits".

Slip's API depends on the present behaviour and states it as a rule for
authors (`spec/api.md` §2.6): the subject is run-time data of the
transform, never an input to a constant expression of the transform. The
reason is isolation. An instantiation is kept across runs. If its
evaluation could ask the host about the subject of the run that happened
to trigger it, the kept result would carry one subject unit's answer into
every later run, and which run came first is scheduling.

Requested of the design of WP-2073:

1. Whatever it does about limits and replay, **a nested evaluation must
   still be unable to reach a run's handler**, or a session must be able
   to say so when it is opened and have that be the default.
2. The diagnostic for the attempt should say what happened in the
   author's terms. Today's text is about handler registration. Slip
   proposes a note of the form "a host call cannot be evaluated here: this
   expression is evaluated when the code is instantiated, not when it
   runs", with the instantiation context.
3. The failure must be the same in every run that reaches the
   instantiation (it is today, by replay). Keep that as a stated
   requirement, with a test: N runs, the instantiation first reached in
   run 1, each run fails identically.

What Slip does not ask: that nested evaluations be charged to the run or
be cancellable. Either is fine.

## 5. Reflection replay keyed by content (already open)

Kiln's WP-2074. Until it lands, `spec/api.md` §2.7 rule 6 puts transforms
that use `<meta>` on their own types outside the determinism guarantee.
`<slip/meta.h>` itself does not use `<meta>`.

## 6. Acceptance

One test per section in Kiln's session tests: each refusal reason; a unit
that prints `__TIMESTAMP__` twice from files touched at different times;
the renderer's output equal to the driver's for the same failure, and
the scalars of an uncaught class object; the three points of §4.
