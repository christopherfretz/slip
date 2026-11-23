# WP-091 — the conformance suite, first brief: the queries

The first of the two briefs of `docs/wp/WP-091.md`: the script tests of
`spec/api.md` §4 (errors), §5.4 and §5.5 (ranges, the origin of a
specialisation) and §6.1 to §6.7 (the queries), written from the
specification alone in the format of `spec/landed/api/WP-090.md`. The
directories are `errors/`, `lookup/`, `structure/`, `references/`,
`ranges/` and `calls/` under `crates/slip_script/tests/api/`. The second
brief (`code/`, `effects/`, `examples/`, `tests/isolation.rs`,
`tests/examples.rs`) has its own file.

## What was built

- 74 script test directories: `errors/` 12 (§4), `lookup/` 14 (§6.1),
  `structure/` 14 (§6.2 to §6.4: 10; §5.5: 4), `references/` 12 (§6.5),
  `ranges/` 14 (§5.4: 7; §6.6: 7), `calls/` 8 (§6.7). Two are xfails.
  The seven `references/r3-*` directories share one transform, so the 74
  directories are 68 compiles.
- `cargo xtask api-table [§N[.M]…]` (`xtask/src/api_table.rs`) prints
  the table below from the `// spec:` and `// xfail:` header lines; the
  table is never written by hand. The unit test
  `api_table::tests::the_landed_tables_are_current` checks that every
  block between `<!-- api-table … -->` and `<!-- /api-table -->` in a
  file under `spec/landed/` is what the command prints for the sections
  its opening line names, so the table below cannot go stale.
- The `landed` lint (`spec/harness.md` §3) accepts `WP-NNN-<part>.md`,
  one file per brief of a package the supervisor split, as this file's
  name requires.

## Decisions

1. **A `// spec:` line names the rule as `§N.M rule K`**, the table rows
   of §4 as `§4 table <errc>`, and the unnumbered paragraph of §5.4 as
   `§5.4 directives`. §5.5 has no numbered rules; its four directories
   are `§5.5`.
2. **Each test reports what it observed** with `report(note, …)`, at the
   range when there is one and without a place otherwise; the
   expectation was written from the text before the test was run. No
   test uses `static_assert` on a host result.
3. **`errors/` reports an outcome as `<errc name> = <value>`** and a
   fixed message, never the text of `message()` (§4 rule 6: "Its text is
   for a person").
4. **A null range is made with syntax that is only part of a macro's
   expansion.** A name or a call that is the whole expansion of an
   invocation (`#define CALL f(1)`) has the invocation as its range, at
   tier 0 (§5.4 rule 6, §6.6 rule 2), so the tests of a null range use
   `#define CALL f(1) + 0` and `#define F(x) (f(x))`.

## The table

<!-- api-table §4 §5.4 §5.5 §6.1 §6.2 §6.3 §6.4 §6.5 §6.6 §6.7 -->
| Rule | Directory | xfail |
|---|---|---|
| §4 rule 1 | `errors/r1-absence-is-a-value` |  |
| §4 rule 2 | `errors/r2-catch-and-continue` |  |
| §4 rule 3 | `errors/r3-no-effect-after-a-throw` |  |
| §4 rule 5 | `errors/r5-not-a-std-exception` |  |
| §4 rule 6 | `errors/r6-message` |  |
| §4 rule 7 | `errors/r7-predicates-never-wrong-kind` |  |
| §4 table invalid_argument | `errors/invalid-argument` |  |
| §4 table invalid_handle | `errors/invalid-handle` |  |
| §4 table limit_exceeded | `errors/limit-exceeded` |  |
| §4 table not_editable | `errors/not-editable` |  |
| §4 table unsupported | `errors/unsupported` |  |
| §4 table wrong_kind | `errors/wrong-kind` |  |
| §5.4 directives | `ranges/s5.4-no-directive` |  |
| §5.4 directives | `ranges/s5.4-no-directive-argument-in-group` | the host gives a null extent to the argument `3` inside the `#else` group, because spec/landed/api/WP-086.md decision 9 nulls an argument whose range is not all that is written between its `,` and `)` (here the directive lines too); spec/api.md §5.4 says "A range inside one group of a conditional is a range" |
| §5.4 rule 1 | `ranges/s5.4-r1-tiers` |  |
| §5.4 rule 2 | `ranges/s5.4-r2-invocation-inside-tier-0` |  |
| §5.4 rule 3 | `ranges/s5.4-r3-argument-used-twice` |  |
| §5.4 rule 4 | `effects/s5.4-r4-an-edit-of-a-tier-1-range` |  |
| §5.4 rule 5 | `ranges/s5.4-r5-null-is-not-an-error` |  |
| §5.4 rule 6 | `ranges/s5.4-r6-first-to-last-token` |  |
| §5.5 | `structure/s5.5-explicit-specialisation` |  |
| §5.5 | `structure/s5.5-function-template-specialisation` |  |
| §5.5 | `structure/s5.5-member-of-instantiation` |  |
| §5.5 | `structure/s5.5-resolved-through-using-declaration` |  |
| §6.1 rule 1 | `lookup/r1-grammar` |  |
| §6.1 rule 2 | `lookup/r2-global` |  |
| §6.1 rule 3 | `lookup/r3-anonymous-union` |  |
| §6.1 rule 3 | `lookup/r3-friend` |  |
| §6.1 rule 3 | `lookup/r3-inline-namespace` |  |
| §6.1 rule 3 | `lookup/r3-unnamed-namespace` |  |
| §6.1 rule 3 | `lookup/r3-unscoped-enumeration` |  |
| §6.1 rule 4 | `lookup/r4-class-scopes` |  |
| §6.1 rule 4 | `lookup/r4-class-template` |  |
| §6.1 rule 4 | `lookup/r4-typedef-for-linkage` |  |
| §6.1 rule 5 | `lookup/r5-not-followed` |  |
| §6.1 rule 5 | `lookup/r5-not-found` |  |
| §6.1 rule 6 | `lookup/r6-overload-set` |  |
| §6.1 rule 7 | `lookup/r7-nothing-matches` |  |
| §6.1 rule 8 | `lookup/r8-in-a-class` |  |
| §6.1 rule 8 | `lookup/r8-in-a-namespace` |  |
| §6.1 rule 8 | `lookup/r8-in-a-specialisation` |  |
| §6.1 rule 8 | `lookup/r8-in-an-enumeration` |  |
| §6.1 rule 8 | `lookup/r8-in-an-unnamed-namespace` |  |
| §6.1 rule 8 | `lookup/r8-not-an-identifier` |  |
| §6.1 rule 8 | `lookup/r8-wrong-sort` |  |
| §6.2 rule 1 | `structure/s6.2-r1-null` |  |
| §6.2 rule 2 | `structure/s6.2-r2-one-sort` |  |
| §6.2 rule 3 | `structure/s6.2-r3-kinds` |  |
| §6.2 rule 4 | `structure/s6.2-r4-definition-and-call` |  |
| §6.3 rule 1 | `structure/s6.3-r1-identifier` |  |
| §6.3 rule 2 | `structure/s6.3-r2-display-string` |  |
| §6.4 rule 1 | `structure/s6.4-r1-parent` |  |
| §6.4 rule 2 | `structure/s6.4-r2-parameters` |  |
| §6.4 rule 3 | `structure/s6.4-r3-declarations` |  |
| §6.4 rule 4 | `structure/s6.4-r4-entity` |  |
| §6.5 rule 1 | `references/r1-unit-order` |  |
| §6.5 rule 2 | `references/r2-referent` |  |
| §6.5 rule 3 | `references/r3-base-specifier` |  |
| §6.5 rule 3 | `references/r3-expression-name` |  |
| §6.5 rule 3 | `references/r3-member-initialiser` |  |
| §6.5 rule 3 | `references/r3-member-name` |  |
| §6.5 rule 3 | `references/r3-qualifier` |  |
| §6.5 rule 3 | `references/r3-template-name` |  |
| §6.5 rule 3 | `references/r3-type-name` |  |
| §6.5 rule 4 | `references/r4-declarative-contexts` |  |
| §6.5 rule 4 | `references/r4-dependent-names` |  |
| §6.5 rule 6 | `references/r6-unresolved-counter` |  |
| §6.6 rule 1 | `ranges/s6.6-r1-name-range` |  |
| §6.6 rule 2 | `ranges/s6.6-r2-extent-of-reference` |  |
| §6.6 rule 3 | `ranges/s6.6-r3-extent-of-expression` |  |
| §6.6 rule 4 | `ranges/s6.6-r4-null-and-refusals` |  |
| §6.6 rule 5 | `ranges/s6.6-r5-source-text` |  |
| §6.6 rule 6 | `ranges/s6.6-r6-editable` |  |
| §6.6 rule 6 | `ranges/s6.6-r6-not-owned` |  |
| §6.7 rule 1 | `calls/r1-shapes` |  |
| §6.7 rule 2 | `calls/r2-not-calls` |  |
| §6.7 rule 3 | `calls/r3-calls-to` |  |
| §6.7 rule 4 | `calls/r4-callee` |  |
| §6.7 rule 5 | `calls/r5-arguments` |  |
| §6.7 rule 6 | `calls/r6-not-a-call` |  |
| §6.7 rule 7 | `calls/r7-uses-that-are-not-calls` |  |
| §6.7 rule 8 | `calls/r8-every-call-is-returned-or-counted` | the host does not put api.calls_without_reference in the run output (spec/landed/api/WP-086.md decision 7 makes it a method of the subject); spec/api.md §6.7 rule 8 names the counter, and §7.6 rule 6 does not list it |

82 directories, 2 xfail.
<!-- /api-table -->

## Rules with no directory

Each of these is a rule the queries cannot observe; none is an xfail,
because no host could pass or fail a test of it.

- §4 rule 4: "`errc` is open: a transform must not assume that a value
  it does not know cannot occur." A statement about later versions.
- §5.4 rule 4: "An edit to a tier 1 range also changes every other use
  of the argument". It needs an edit; it belongs to the second brief
  (`effects/`).
- §5.4 rule 7: "`spec/macros.md` will define tiers for all of Slip." Not
  behaviour.
- §6.5 rule 5: "Coverage only grows. A later version returns everything
  this one returns, and more." A statement about later versions.

## The xfails

1. `calls/r8-every-call-is-returned-or-counted` (§6.7 rule 8). The
   subject holds expressions a test of position could drop: an explicit
   destructor call, `d.template t<int>()`, calls inside `decltype`,
   `noexcept`, a template argument and `alignas`. The header asks for
   `// counters: api.calls_without_reference` and expects 1 (the
   destructor call). The host renders
   `counter api.calls_without_reference absent`: `WP-086.md` decision 7
   made it the method `KilnSubject::calls_without_reference()`, not a
   counter of the run. §6.7 rule 8 names it as a counter, and §7.6 rule
   6's list of counters does not have it, so the text disagrees with
   itself and with the host.
2. `ranges/s5.4-no-directive-argument-in-group` (§5.4, "A range holds no
   directive"). Subject:

   ```cpp
   int f(int, int);
   int y = f(1,
   #ifdef X
             2
   #else
             3
   #endif
   );
   ```

   The text: "A range inside one group of a conditional is a range." The
   argument `3` is one token inside the `#else` group, so its extent is
   `t.cpp:6:11-6:12`. The host gives null, because `WP-086.md` decision 9
   makes an argument null when its range is not all that is written
   between its `,` and `)`, and here the directive lines lie there too.
   The same happens with `#if 0` … `#endif` before the argument on its own
   line (`f(1,` newline `#if 0` newline `2` newline `#endif` newline
   `3);`), where no rule of the text makes `3` null. §6.6 rule 4 lets
   the host give null "when the host cannot establish the first or last
   token exactly", which it can here.
   `ranges/s5.4-no-directive` keeps the rest of the paragraph tested
   and passes.

## Silences in the text

Each item gives the section and rule, the smallest subject, what the text
says, and what the host does. None was blessed into an `expect.txt`;
where a test was possible without the question, the question was left out
of the test.

1. §4 rule 2, "an error not caught fails the run". The rendering of a
   failed run (`spec/landed/api/WP-090.md`) holds the first line of the
   diagnostic, which no spec fixes. A transform `consteval void
   transform() { (void)identifier_of(info{}); }` fails, but its `expect.txt`
   would hold host text. No directory; the format needs an expectation
   that names the kind of failure only.
2. §5.4 rule 3, a call in a macro argument the replacement list uses
   twice (`#define TWICE(e) ((e) + (e))`, `TWICE(g(1))`): the text does
   not say whether that is one call or two. The test expects two calls
   (rule 3's "each use is its own expression"), one reference and one
   callee reference; the host agrees.
3. §5.5, "a member only an explicit specialisation declares is its own
   origin". No query reaches such a member (`template<> struct S<int> {
   int k; };`): §6.1 rule 1's grammar takes no template arguments, so
   `S<int>::k` cannot be written, and the text does not say whether
   `S::k` names it. With no handle there is nothing to give
   `references_to` or `calls_to`. Untested.
4. §6.2 rule 3, the kind table: `is_template` is true of "a template of
   any kind", and "an entity may satisfy none of them (an enumerator, a
   concept)". A concept is a template in C++ ([temp.concept]). The test
   follows the second sentence (a concept satisfies none); the host
   agrees. Whether an alias template is also `is_type` is not said; the
   test expects `is_template` only, and the host agrees.
5. §6.3 rule 1: whether a class named only by a typedef for linkage
   purposes (`typedef struct { int m; } T;`) has the identifier `T`. Not
   said; left out of `s6.3-r1-identifier`. `lookup/r4-typedef-for-linkage`
   finds it by `T`, as §6.1 rule 4 says.
6. §6.4 rule 3, `declarations_of`: of the global namespace (no
   declaration is written) and whether `template<> struct S<int> {};`
   is a declaration of `S`. Neither is said; both are left out.
7. §6.5 rule 4: whether the qualifier `n` of a qualified concept name
   (`template <n::C T>`) is a reference to `n`. The table lists "a
   concept name" as declarative but says nothing of its qualifier; left
   out.
8. §6.5 rule 4, the row of references imported from a precompiled header
   or a module: the script format cannot build either. Untested.
9. §6.7 rule 5: "A pack expansion `args...` is one written argument."
   Every call with a pack expansion as written argument is in a template,
   so it is dependent and not a call (`WP-086.md`); the sentence has
   nothing to apply to in version 0. Untested.
10. §6.7 rule 8: "and the count of those not written `callee ( … )`"
    names no counter. Untested.
11. §6.1: whether `lookup("S::S")` finds a constructor. An entity with no
    identifier is out of reach of lookup in version 0, but the injected
    class name makes `S::S` a name. Not said; left out.

## Disagreements I have

- §6.6 rule 4's escape, "when the host cannot establish the first or
  last token exactly", lets a host give null for any range, so no test
  can require a range. The xfail above stands on the claim that the host
  could establish the tokens; the text should say which shapes may be
  null (as `WP-084.md` decision 3 does for macros) rather than leave it
  to the host.
- The rendering of a failed run depends on unspecified diagnostic text,
  which keeps §4 rule 2's failing half out of the suite.

## Items for Kiln

None. The one candidate, the null argument inside a conditional group,
is the host's own decision (`WP-086.md` decision 9).
