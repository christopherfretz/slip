# WP-091 — the conformance suite, second brief: effects and examples

The second of the two briefs of `docs/wp/WP-091.md`: the script tests of
`spec/api.md` §7.1 and §7.2 (code values), §7.3 to §7.7 (effects, output,
limits), §5.4 rule 4 (an edit of a tier 1 range) and the two examples of
§10, written from the specification alone in the format of
`spec/landed/api/WP-090.md`, and the two test files `tests/isolation.rs`
and `tests/examples.rs` of the package's "Tests". The first brief's
directories are in `spec/landed/api/WP-091-queries.md`.

## What was built

- 64 script test directories under `crates/slip_script/tests/api/`:
  `code/` 21 (§7.1: 12; §7.2: 9), `effects/` 23 (§5.4 rule 4: 1; §7.3: 6;
  §7.4: 4; §7.5: 4; §7.6: 4; §7.7: 4), `examples/swap/` 12 (§10.1) and
  `examples/rename/` 8 (§10.2). One is an xfail. The 20 example
  directories are two transforms (one compile each, two with the
  `// owned: no` line for §10.1), so the 64 directories are 47 compiles.
- Each `transform.cpp` of `examples/` is its header lines and one line,
  `#include <../examples/swap_arguments.cpp>` or
  `#include <../examples/rename_function.cpp>`: the files of
  `resource/examples/` themselves, found from the host's include
  directory `resource/include` (§2.1), never copied.
- `crates/slip_script/tests/examples.rs`: for each of the two
  directories, every `transform.cpp` is exactly that (header lines, the
  include), and loading each distinct one reads the example file with
  the digest of its bytes on disk (`Loaded::files`).
- `crates/slip_script/tests/isolation.rs` (§2.4 rule 3, §2.7): every
  directory of the whole suite whose expectation is a completed run and
  not an xfail runs in its group's session (one compile per command
  line, `script_test::run_group`), N = 3 times in suite order, then 3
  times in reverse order with a failing run (the group's first script
  with a step budget of 1) before each run and once at the end. Every
  completed run of a directory renders what its first did, with the
  counters `api.steps`, `api.host_calls` and `api.peak_bytes` added to
  its own; every failing run renders the same failure. Groups run on at
  most four threads, each holding a jobserver token.

## Decisions

1. **A `// spec:` line names the rule** as `§7.N rule K`; the rows of
   §7.7 as `§7.7` (the table has no numbered rules; the directory name
   says which rows); the examples as `§10.1` and `§10.2`. The §5.4 rule
   4 directory is in `effects/`, so the first brief's §5.4 table (in
   `WP-091-queries.md`) gained its row; that block was regenerated, not
   edited, and its prose still lists §5.4 rule 4 as "the second
   brief's".
2. **A transform reports what it observed** with `report(note, …)`,
   unplaced, in a fixed sentence; the outcome of a call that may throw
   is `returned` or the name of the `errc` it threw (a local `OUTCOME`
   macro over `try`/`catch (const error&)`), never `message()` (§4
   rule 6), except where §7.7's kept-text row is the subject. No test
   uses `static_assert` on a host result.
3. **The expectation was written from the text before the run.** Where
   the first run differed, the difference was traced to the text: my
   misreadings were corrected (the direction of a recorded rename is the
   rule that undoes it, `spec/verify.md` §2.1; key names are
   `spec/verify.md` §6.1's), and the one place the host departs from the
   text is the xfail below.
4. **Edits in the tests are structural.** Every piece is `copy`,
   `spell` or a literal identifier or punctuation token; no test builds
   C++ from strings beyond what §7.1 offers (see "Disagreements").
5. **Each example subject's `t.cpp` opens with comment lines** saying
   which row of WP-091's table it is; ranges are counted below them.
6. **The examples include from `<../examples/…>`**, not by a relative
   `"…"` path from the test directory, because the runner refuses a
   transform that reads a file whose path starts with its directory's,
   and compares un-normalised paths (`script_test.rs`, `run_loaded`):
   `tests/api/examples/swap/x/../../../../../../resource/examples/…`
   starts with `tests/api/examples/swap/x`. See "Items for the runner".

## The table

<!-- api-table §7 §10 -->
| Rule | Directory | xfail |
|---|---|---|
| §7.1 rule 1 | `code/s7.1-r1-text-any-bytes` |  |
| §7.1 rule 2 | `code/s7.1-r2-copy` |  |
| §7.1 rule 3 | `code/s7.1-r3-copy-is-by-reference` |  |
| §7.1 rule 3 | `code/s7.1-r3a-two-edits-of-one-range` |  |
| §7.1 rule 3 | `code/s7.1-r3b-an-edit-that-draws-on-itself` |  |
| §7.1 rule 3 | `code/s7.1-r3c-a-nested-edit` |  |
| §7.1 rule 4 | `code/s7.1-r4-spell-supported` |  |
| §7.1 rule 4 | `code/s7.1-r4-spell-unsupported` |  |
| §7.1 rule 4 | `code/s7.1-r4-spell-wrong-kind` |  |
| §7.1 rule 5 | `code/s7.1-r5-cat-pieces` |  |
| §7.1 rule 5 | `code/s7.1-r5-cat-refuses-an-info` |  |
| §7.1 rule 6 | `code/s7.1-r6-immutable-and-merged` |  |
| §7.2 rule 1 | `code/s7.2-r1-another-scope` |  |
| §7.2 rule 1 | `code/s7.2-r1-exactly-one-spelled-piece` |  |
| §7.2 rule 1 | `code/s7.2-r1-no-name-range` |  |
| §7.2 rule 1 | `code/s7.2-r1-only-at-the-extent` |  |
| §7.2 rule 1 | `code/s7.2-r1-several-references-one-extent` |  |
| §7.2 rule 1 | `code/s7.2-r1-sibling` |  |
| §7.2 rule 1 | `code/s7.2-r1-through-a-using-declaration` |  |
| §7.2 rule 2 | `code/s7.2-r2-fully-qualified` |  |
| §7.2 rule 2 | `code/s7.2-r2-unnamed-enumeration` | the text of the spelling is `::n::A` as rule 2 says, but the host names the key of the enumerator `n::A`, where spec/verify.md §6.1 names the members of an unnamed enumeration under it, `n::(anonymous enum 0)::A` |
| §7.3 rule 1 | `effects/s7.3-r1-replace-and-delete` |  |
| §7.3 rule 2 | `effects/s7.3-r2-not-editable` |  |
| §7.3 rule 2 | `effects/s7.3-r2-refusals` |  |
| §7.3 rule 3 | `effects/s7.3-r3-overlapping-requests` |  |
| §7.3 rule 4 | `effects/s7.3-r4-groups` |  |
| §7.3 rule 5 | `effects/s7.3-r5-notes` |  |
| §7.4 rule 1 | `effects/s7.4-r1-places` |  |
| §7.4 rule 2 | `effects/s7.4-r2-severities` |  |
| §7.4 rule 3 | `effects/s7.4-r3-the-place-of-a-value` |  |
| §7.4 rule 4 | `effects/s7.4-r4-severity-is-open` |  |
| §7.5 rule 1 | `effects/s7.5-r1-expect-changed` |  |
| §7.5 rule 2 | `effects/s7.5-r2-expect-changed-refusals` |  |
| §7.5 rule 3 | `effects/s7.5-r3-expect-renamed` |  |
| §7.5 rule 4 | `effects/s7.5-r4-idempotent` |  |
| §7.6 rule 1 | `effects/s7.6-r1-identity-is-content` |  |
| §7.6 rule 1 | `effects/s7.6-r1-tiers-are-a-set` |  |
| §7.6 rule 2 | `effects/s7.6-r2-groups-are-joined` |  |
| §7.6 rule 6 | `effects/s7.6-r6-counters` |  |
| §7.7 | `effects/s7.7-bytes-of-one-argument` |  |
| §7.7 | `effects/s7.7-failures-whose-text-is-kept` |  |
| §7.7 | `effects/s7.7-pieces-in-one-code-value` |  |
| §7.7 | `effects/s7.7-pieces-in-total` |  |
| §10.1 | `examples/swap/call-in-a-macro-argument` |  |
| §10.1 | `examples/swap/call-made-by-a-macro` |  |
| §10.1 | `examples/swap/file-not-owned` |  |
| §10.1 | `examples/swap/named-and-not-called` |  |
| §10.1 | `examples/swap/nested-call` |  |
| §10.1 | `examples/swap/one-and-three-arguments` |  |
| §10.1 | `examples/swap/parentheses-from-a-macro` |  |
| §10.1 | `examples/swap/plain-call` |  |
| §10.1 | `examples/swap/qualified-and-member-of-namespace` |  |
| §10.1 | `examples/swap/replacement-not-declared` |  |
| §10.1 | `examples/swap/same-spelling-another-entity` |  |
| §10.1 | `examples/swap/two-overloads-with-two-parameters` |  |
| §10.2 | `examples/rename/a-class-of-the-same-name` |  |
| §10.2 | `examples/rename/declarations-in-two-headers` |  |
| §10.2 | `examples/rename/name-in-a-macro-body` |  |
| §10.2 | `examples/rename/new-name-taken` |  |
| §10.2 | `examples/rename/overload-set` |  |
| §10.2 | `examples/rename/qualified-and-unqualified-uses` |  |
| §10.2 | `examples/rename/use-through-a-using-declaration` |  |
| §10.2 | `examples/rename/variable-of-the-same-name` |  |

63 directories, 1 xfail.
<!-- /api-table -->

`effects/s5.4-r4-an-edit-of-a-tier-1-range` is in the §5.4 table of
`spec/landed/api/WP-091-queries.md`.

## Rules with no directory

None of these can be observed in one run's output; none is an xfail.

- §7.2 rules 3, 4 and 5 (where the spelling is resolved, that the
  sibling form needs no lookup, the `api.unresolved_spelling_matches`
  check): what a run records is the key and the text (`spell(<key>
  "<text>")`); rule 5's counter is shown as 0 in
  `effects/s7.6-r6-counters`, but no subject makes it non-zero, because
  the text gives no case in which a fully qualified name resolves
  elsewhere.
- §7.3 rules 6 and 7 (an edit is applied by apply; the transform cannot
  read its edits back): apply's, and a statement of absence.
- §7.5 rules 5 to 8 (what verification does with `changed` and
  `renamed`): `spec/verify.md`'s, outside a run.
- §7.6 rules 3 and 4 (the output is the run's result; reduce and apply
  read it): no run-level behaviour. Rule 5 (a failed run has no output
  but its failure) is exercised by `tests/isolation.rs` only as "the
  failure renders alike", because the rendering of a failure holds
  diagnostic text (WP-129 item 3).
- §7.7, rows tested: bytes of one note, reason or message; bytes of one
  qualified name or identifier argument; pieces in one code value;
  pieces in total; failures whose text is kept. Rows not tested,
  because reaching them costs too much time or memory for a debug-build
  test: code values (2^22), bytes of text (2^28), edits (2^20), groups
  (2^20), reports (2^16), declared changes (2^20), bytes of text
  arguments (2^26), integers in one answer.

## The xfails

1. `code/s7.2-r2-unnamed-enumeration` (§7.2 rule 2). Subject:

   ```cpp
   namespace n {
   enum { A, B };
   }
   int a = n::A;
   ```

   `replace(extent_of(R), spell(A))` for the reference `n::A`. The text of
   the spelling is `::n::A`, as rule 2 says ("a member of an unnamed
   enumeration is named as a member of the enclosing scope"), and the
   host agrees. The host renders the piece's key name `n::A`, where
   `spec/verify.md` §6.1 names the members of an unnamed enumeration
   under it: `n::(anonymous enum 0)::A`. Expected
   `spell("n::(anonymous enum 0)::A" "::n::A")`, got
   `spell("n::A" "::n::A")`. Either `spec/verify.md` §6.1 or the host's
   key names is wrong; the text of the spelling is right.

## Silences in the text

Each: the section and rule, what is not said, and what the test does.
None was blessed; where a test was possible without the question, the
question was left out.

1. §7.1 rule 2: `copy(r)` of an empty range. No query of version 0
   returns an empty range, and a forged one is not a range a query
   returned; untested.
2. §7.1 rule 3 (a), (b), (c): the rejections are reduce's. The suite
   tests only "the host records the requests" (`code/s7.1-r3a`, `r3b`,
   `r3c`).
3. §7.1 rule 4: "an entity declared in a function body" is unreachable:
   no query of version 0 returns one. The other kinds are in
   `code/s7.1-r4-spell-unsupported`. Whether a class named only by a
   typedef for linkage purposes is "an unnamed class" is not said; left
   out.
4. §7.1 rule 5: `cat` does "not accept" an `info`, but whether that is a
   compile error or a throw is not said. `meta.h` declares `cat` over an
   unconstrained pack, so the refusal is an error inside the template
   and a `requires` test cannot see it; the test expects
   `error transform-compile`, the only outcome that is not acceptance.
5. §7.2 rule 1, condition 1 ("R is a reference") cannot fail: `extent_of`
   of anything else is not a reference's extent. Not separately tested.
6. §7.3 rule 2: an empty range forged from a live one is both "an empty
   range" (invalid_argument) and not a returned range (invalid_handle);
   which is thrown is not said. Left out of `effects/s7.3-r2-refusals`.
7. §7.4: the place of `report(info{}, …)` and `report(range{}, …)` (a
   null value) is not said, nor whether either throws; untested. How a
   report "says so" when attached to an invocation (rule 3) is only the
   `attached` flag of the `Report` grammar; the test expects that flag.
8. §7.5 rule 1: the derived units of a unit (`spec/digest.md` §2.2) are
   not tested, because whether a unit has them depends on resolved
   properties the text does not let a test predict; a file entered in
   two units is not tested either. The order of `changed` records is by
   key digest (§7.6), which the text does not let a test compute: the
   expectation has the host's order of two keys.
9. §7.5 rule 4: the same reference declared changed with two different
   reasons is not said to be one record or two; untested.
10. §7.6 rule 1: the order of edits with the same place is "by pieces",
    and the order of two pieces of different kinds (a copy, a literal, a
    spelling) is not defined. Every test with two edits of one range uses
    two pieces of one kind.
11. §7.6 rule 6: the order of the `counter` lines is not fixed (§11 rule
    3 says only that they are part of the output); the expectation has
    the host's (by name). Whether `api.edits` counts requests or distinct
    edits is not said; the test requests no edit twice. What
    `api.host_calls` counts per L1 function is not fixed; it is only
    checked to be the same on every run (`tests/isolation.rs`).

## Disagreements I have

- **The fully qualified spelling at a name range.** §7.2 rule 1's sibling
  form applies only to `replace(extent_of(R), c)`. A transform that
  respells the name range of a qualified reference (the most natural
  thing to do: `replace(name_range_of(R), spell(E))` for `n::f`) gets
  `::n::g` inside `n::`, so `n::::n::g`, which does not compile
  (`code/s7.2-r1-only-at-the-extent`; the host does what the text says).
  The text should give a name range the identifier alone (the qualifier
  that is written stays), or make `spell` at a name range an error.
- **§7 has no piece for an identifier.** §10.2 respells a declaration's
  name with `text(to)`, a literal; there is no structural way to say
  "this identifier", so nothing checks that `to` is one, nor keeps a
  later version from needing to know it is a name. A wrap
  (`f(x)` to `g(f(x))`, §7.1 rule 3 (a)'s example) also needs literal
  punctuation. This is §7's text escape hatch, and the suite uses it
  only for identifiers and single punctuation tokens.
- **§10.2 misses a use through a using-declaration without a report.**
  `using lib::old_name;` names the function, but §6.5 rule 4 says the
  name in a using-declaration is not a reference in version 0, so the
  example neither edits nor reports it, while §10.2 says every place the
  rename cannot reach is an error report
  (`examples/rename/use-through-a-using-declaration`, which passes: the
  call through it is edited, the using-declaration is not). Only
  verification would catch it. The example, or §6.5, should make it
  visible.
- The rendering of a failed run depends on unspecified diagnostic text
  (the first brief's objection), which also limits `tests/isolation.rs`
  to "every failing run renders alike".

## Items for the runner

- `crates/slip_script/src/script_test.rs`, `run_loaded` (the check at
  `p.starts_with(first)`): the paths are compared without normalising
  `..`, so a transform that includes a file outside its directory by a
  relative path from inside it is refused as reading "a file of its
  directory". It should compare canonical paths (or the paths Kiln
  recorded after resolution).

## Items for Kiln

None.
