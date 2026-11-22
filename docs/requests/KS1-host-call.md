# KS1 — the host-call builtin: Slip's constraints and acceptance tests

Sent to the Kiln agent 2026-11-22, in answer to its design for Kiln
WP-1990 (`__builtin_kiln_host_call`, the nine-argument signature of
[05](../design/05-kiln-interface.md) §3; `trait HostCalls`; never memoised;
an error under speculative evaluation; nothing carried across translation
units). Kiln owns the design; these are what Slip needs from it.

## Requested changes

1. **Errors without a library type.** `<slip/meta.h>` has to work on a
   standard library without `<meta>` (design question 2: it is tested
   against libc++ too), so it cannot depend on the builtin throwing
   `std::meta::exception`. Wanted: `Err(HostError)` **ends the evaluation**
   with the message as the diagnostic, and cannot be caught. A failure a
   transform may handle is Slip's own protocol (a result code; the message
   fetched with another operation), and `<slip/meta.h>` throws its own
   type. If Kiln keeps a throwing form, it should be chosen per error
   (`HostError::Throw` against `HostError::Abort`) and be an ordinary
   diagnostic, not a crash, when `std::meta::exception` is not declared.
2. **The return value is the handler's and is not interpreted.**
   Count-then-fill is Slip's convention over two buffers; one number
   cannot be "the count" for both. `HostOut` gives the handler the two
   capacities and takes back how many integers and how many bytes it
   wrote (each at most the capacity; more is a handler bug and ends the
   evaluation). Exactly those elements become initialised; the rest of
   each buffer is untouched.
3. **Check before calling.** Every pointer range is checked before the
   handler runs: `in` and `bin` readable and initialised for their
   lengths, `out` and `bout` writable objects of the evaluation for their
   capacities, null allowed exactly when the length is 0. A call that
   fails a check is not a constant expression and the handler is not
   called: handlers have effects (they emit edits), so a call must not
   half-happen.

## Acceptance tests

With a test handler: op 1 returns a counter that it increments; op 2
echoes `ints` and `bytes` to the outputs and returns
`(ints.len() << 32) | bytes.len()`; op 3 returns `Err("boom")`; op 4
returns the two capacities it was given.

| # | Source (inside a `consteval` function unless said) | Expected |
|---|---|---|
| 1 | no handler registered: `__has_builtin(__builtin_kiln_host_call)`; a call | 0; an ordinary "undeclared" error |
| 2 | op 1 three times in one function | 0, 1, 2: evaluated once each, in order |
| 3 | `constexpr u64 next() { return call(1); }` called twice with the same arguments, and from two different callers | different values each time: no frame on the stack is memoised |
| 4 | one top-level evaluation run twice (KS2 will do this) | the handler is called again each time |
| 5 | op 2 with `in` and `bin` from a `std::vector`, a `std::string`, a string literal with an embedded NUL, and a pointer into the middle of an array | the bytes and integers arrive exactly; lengths are the arguments, never a terminator |
| 6 | op 2 with capacities 0 and null outputs, then with exact capacities | first call: the return value and nothing written; second: the data; reading an element past what the handler wrote is the usual uninitialised-read error |
| 7 | op 2 where `out` overlaps `in` | defined: inputs are read before outputs are written |
| 8 | op 4 with `cap_out` 3, `cap_bout` 5 | 3 and 5 |
| 9 | `in` one element short of `n_in`; `out` pointing at a `const` object, at a string literal, at an object outside the evaluation | not a constant expression, with a diagnostic naming the argument; handler not called (the counter of op 1 does not move) |
| 10 | op 3 | the evaluation fails with "boom" in the diagnostic (or, in the throwing form, `catch` sees it) |
| 11 | a function whose body holds `const unsigned long long n = call(1);` and uses `n` only as a value | parsing the definition gives no error and calls nothing; evaluating it calls once |
| 12 | the same with `int a[n];`, and a namespace-scope `const unsigned long long g = call(1);` | the hard error for speculative evaluation; handler not called |
| 13 | a `constexpr` function that holds a call, called at run time | an error at the call or at code generation saying a host call is outside constant evaluation; never an undefined symbol |
| 14 | `noexcept(call(1))`, `sizeof(call(1))`, `decltype(call(1))` | fine: unevaluated, nothing called |
| 15 | 1,000,000 calls of op 2 with one integer in and out, in a loop | measured and reported (calls per second). Slip's hot paths assume a few million per second; if it is far below, the integer-only second builtin of 05 §3 is wanted |
| 16 | a handler that returns `Err` after a cancellation flag is set, inside a deep call stack | the evaluation ends promptly with that message; destructors of the evaluation's objects are not observable to the handler |

## Not needed from KS1

Restricting where the builtin may be written (05 says the `__slip_*` layer
is refused outside Slip's resource directory): Slip can check that itself
from the reference table, if at all.
