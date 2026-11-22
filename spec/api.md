# The transform API: `<slip/meta.h>`, version 0

A transform is a C++ program. Slip compiles it once and evaluates it at
compile time, inside Kiln's constant evaluator, once for every translation
unit being refactored. The program asks questions about that translation
unit and asks for edits. This specification defines what the program may
say and what Slip must answer.

Rationale is in `docs/design/01-transform-api.md`; where that document and
this one differ (§13), this one is the contract. What the API rests on in
Kiln is recorded in `docs/design/notes/kiln-landed-2026-11.md`.

Three texts are normative together:

| Text | What it fixes |
|---|---|
| This file | Meaning: what every name does, what every failure is |
| `resource/include/slip/meta.h` | The declarations of layer L1, exactly as shipped |
| `resource/include/slip/__intrinsics.h` | Layer L0: operation numbers and the result word |

Two transforms are part of the conformance suite and are referenced from
§10: `resource/examples/swap_arguments.cpp` and
`resource/examples/rename_function.cpp`.

## 1. Terms

- **Subject**: the code being refactored. A **subject unit** is one
  observer of `spec/model.md` §7: one translation unit under one command
  line, parsed by its own Kiln instance.
- **Transform unit**: the translation unit that holds the transform, with
  everything it includes. It is parsed by a different Kiln instance.
  Nothing of the subject is visible to it as C++: not a type, not a macro,
  not a header.
- **Host**: the part of Slip that answers the transform (`slip_script`).
- **Run**: one evaluation of an entry point (§2.3) against one subject unit.
- **Layers**: L0 is the private intrinsic layer (§3). L1 is
  `namespace slip`, the contract (§4 to §8). Libraries written against L1
  are L2 and are not specified here.
- **Unit order**: the order of the token sequence of the subject unit after
  translation phase 4. Every declaration, written name and expression has a
  first token in that sequence, and two of them are ordered by it. Two
  things with the same first token are ordered outermost first.

## 2. The transform universe

### 2.1 The transform unit

1. A transform is one source file. Slip compiles it as Kiln would compile

   ```
   kiln -std=c++26 -fsyntax-only -fkiln-reproducible-time
        -I <resource>/include  <author's options>  <transform file>
   ```

   where `<resource>` is the resource directory of the running Slip.
2. The author's options are `-I`, `-isystem`, `-D`, `-U` and `-include`,
   in the order given. Any other option is the error `transform-option`.
   `-D` is how a transform is given parameters.
3. The target is the one Kiln chooses by default on the machine that runs
   Slip, and the C++ standard library is the one Kiln finds for it. Both
   are inputs of the transform's identity (§2.2). Version 0 requires of
   the library only that `<string>`, `<string_view>` and `<vector>` are
   usable in constant evaluation.
4. `__DATE__` and `__TIME__` are those of the Unix epoch. `__TIMESTAMP__`
   must be the same on every compilation of the same file contents
   ("Open questions").
5. A transform unit with an error is the error `transform-compile`. Its
   diagnostics are shown to the author as Kiln renders them. No subject
   unit is parsed for a transform that does not compile.
6. `<slip/meta.h>` parses under any C++26 implementation. Outside a Slip
   transform unit the functions of §6 and §7 are declared and cannot be
   evaluated. This is so that an editor or another compiler can check a
   transform; it promises nothing else.

### 2.2 Identity

```
TransformId = H("slip.transform.v0",
                api level (§8.2, the value of __slip_lib_meta),
                the command line of §2.1 with <resource> and every path as given,
                for each file the unit read, in the order first read:
                    its path as given, the digest of its content)
```

`H` is the hash of `spec/model.md` §3. A run of Slip has one `TransformId`.
Every result of the map phase carries it, and reduce must refuse to combine
results that carry different ones.

The host accepts a transform unit only if the two headers it read under
`<resource>/include/slip/` are byte for byte the ones this Slip was built
with (error `transform-header`). Operation numbers are therefore private
to one build.

### 2.3 Entry points

1. An entry point is a function in the global namespace with a reserved
   name. Version 0 defines one:

   ```cpp
   consteval void transform();
   ```

2. It must be declared exactly once as a function, be `consteval`, take no
   parameter, return `void` and be defined. Each violation is its own
   error, checked before any subject unit is parsed:

   | Error | Condition |
   |---|---|
   | `entry-missing` | no declaration named `transform` in the global namespace |
   | `entry-overloaded` | more than one function of that name |
   | `entry-not-function` | the name is something else |
   | `entry-not-consteval` | not an immediate function |
   | `entry-parameters` | it has a parameter |
   | `entry-type` | it returns anything but `void` |
   | `entry-undefined` | declared and not defined |

3. The names `reduce`, `prepare` and `repair` are reserved for entry points
   of later versions. A version 0 host refuses a transform unit that
   declares a function of one of those names in the global namespace
   (error `entry-reserved`).

Rejected: marking entries with an annotation (it needs reflection over the
transform unit and buys nothing while there is one entry); a registration
macro (macros are the hardest part of the subject and should not be the
first thing an author meets); an entry with parameters (Kiln's sessions do
not pass arguments, and in version 0 the only possible argument, the
subject unit, is ambient).

### 2.4 Runs

1. **One run per subject unit.** For each subject unit the host evaluates
   `transform()` exactly once. The transform finds its own sites with the
   queries of §6 and loops over them.
2. A run sees exactly one subject unit. It cannot learn that others exist.
3. **Isolation.** A run starts from the state of the compiled transform
   unit. No object, no value and no host-side object (a `code`, a `group`,
   a recorded effect) of another run is reachable from it. Whether a
   template of the transform unit was instantiated by an earlier run is
   not observable, including through step counts and diagnostics.
4. A subject unit that Kiln could not parse without errors is not given to
   the transform; `spec/exec.md` says what happens to it.
5. The transform unit is compiled once in each worker process and kept
   for every job the process serves. Its compiled form is never shared
   between processes or written to disk in version 0.

Why per unit and not per site. An empty run costs about 75 ns, so the cost
of a run does not decide this. What decides it:

| | One run per unit (chosen) | One run per site |
|---|---|---|
| Finding sites | a query the transform writes | a selector language the host must define first |
| Work common to all sites of a unit (lookups, tables) | done once | repeated, or cached by the host between runs, which breaks isolation |
| What a failure discards | the unit's results, which is the unit `spec/exec.md` already retries and reports | one site, leaving a unit half transformed |
| Budget | one deterministic bound per unit | a bound per site and still one per unit |
| Needs from Kiln | nothing more | entries with arguments (not built), or a host call to fetch "the current site" |

Per-site evaluation remains possible later as a host-side loop that runs
an action entry once per match (§9); nothing here prevents it.

### 2.5 Budgets and cancellation

1. A run has three budgets, the same for every run of one Slip run:

   | Budget | Default | Counts |
   |---|---|---|
   | steps | 2^32 | evaluator steps, including the steps a host call is charged (Kiln `spec/host-calls.md` §2) |
   | bytes | 256 MiB | storage the evaluation allocates |
   | depth | 4096 | nested calls |

2. Budgets are deterministic: whether a run exhausts one depends only on
   the subject unit and the `TransformId`, never on the machine, on load,
   or on earlier runs. There is no wall-clock limit inside a run; the
   observer's time limit is `spec/exec.md`'s.
3. Host-side objects are bounded separately (§7.7), because the evaluator
   does not see the host's memory.
4. Cancellation is only for shutting a worker down. A cancelled run has no
   result at all.

### 2.6 Where the host may be reached

A function of §6 or §7 (every function of L1 declared `consteval`) is a
**host function**. The subject is run-time data of the transform program:

1. A host function may be evaluated only as part of a run: by the entry
   point and by what it calls.
2. It must not be evaluated while the transform unit is compiled: not in
   the initialiser of a variable at namespace scope, a `static_assert` at
   namespace or class scope, or a `consteval` block. The transform unit then fails to compile
   (`transform-compile`).
3. Inside a run it must not be evaluated by a constant expression that the
   run's code contains or instantiates: a template argument, an array
   bound, the initialiser of a `constexpr` variable, the condition of
   `if constexpr`, a `case` label, a `static_assert`, a constraint, a
   `noexcept` or `explicit` operand. Such an expression is evaluated when
   the code is compiled or instantiated, not when it runs. In a function
   that is not a template this is found when the transform unit is
   compiled; in a template it fails the run that first instantiates it,
   and every later run that reaches the same instantiation fails the same
   way.
4. Feature tests (§8.2) are macros, so that they can be used in all of
   those places.

Kiln enforces 2 and 3 (`spec/host-calls.md` §4, `spec/eval-session.md`
§6): the only handler that reaches the subject is the one given to a run,
and nested evaluations have none. Every case is a diagnosed failure,
never undefined behaviour.

### 2.7 Determinism

The results of a run (§2.8) are a function of the subject unit and the
`TransformId`. To that end:

1. Every sequence a query returns has a specified order (unit order unless
   stated). No order depends on a hash, an address or a thread.
2. The numeric value inside an `info`, `range`, `code` or `group` is not
   specified and not observable except through `==`.
3. No query returns the time, an environment variable, a host path, a
   process identifier or anything else that differs between two machines
   holding the same subject. Version 0 has no query that returns a path.
4. Effects are write-only. No query's answer changes because an effect was
   recorded.
5. The text of `display_string_of` and of `error::message` is not
   specified, and is still the same for the same subject unit and
   `TransformId`.
6. A transform that reflects on its own types with `<meta>` is outside
   this guarantee until Kiln keys reflection queries by content
   (`docs/design/notes/kiln-landed-2026-11.md` §4.5).

### 2.8 The result of a run

A run **completes** or **fails**.

| Failure | Cause |
|---|---|
| `uncaught` | an exception left `transform()`; for a `slip::error`, the failure it reports (§4) is part of the diagnostic |
| `not-constant` | the evaluation did something that is not a constant expression (a null dereference, an out-of-bounds read, a failed instantiation, §2.6 rule 3) |
| `step-budget`, `memory-budget`, `depth-limit` | §2.5 |
| `host-failure` | the host could not continue (an internal error). Never caused by a transform |

1. A failed run has no effects: every edit, report and declared change it
   requested is discarded. What is recorded for the subject unit is the
   failure and its diagnostics.
2. A completed run's effects are the **run output**, §7.6.
3. **Diagnostics to the author.** A failure is reported with the kind
   above, the location in the transform's source where it arose, the call
   stack of the evaluation as Kiln gives it, and the subject unit. For
   `uncaught` with a `slip::error` the report adds the function that
   refused, its `errc`, its message, and the place in the subject that the
   offending value denotes, if it denotes one. The same failure in many
   subject units is shown once with a count and one example unit.
4. A subject unit whose run failed has not been examined. Slip must not
   apply the edits of a Slip run in which a run failed unless told to;
   that policy is `spec/exec.md`'s and reduce's.

## 3. The intrinsic layer (L0)

L0 is one primitive and a table. It is private: this section binds a host
and the header shipped with it, and promises a transform nothing.

### 3.1 The primitive

Every host function reaches the host through Kiln's
`__builtin_kiln_host_call` (Kiln `spec/host-calls.md` §1):

```
result = call(op, in[0..n_in), bin[0..n_bin), out[0..cap_out), bout[0..cap_bout))
```

- `op`: the operation number (§3.2).
- `in`: unsigned 64-bit integers: handles (§3.4) and small numbers.
- `bin`: at most one text argument, as bytes. No terminator.
- `out`, `bout`: where the host writes integers and bytes.
- `result`: one unsigned 64-bit word (§3.3).

### 3.2 Operations

"V" and "B" are the result conventions of §3.3. An argument written `r` is
a range: two integers, the file handle and `lo << 32 | hi`.

| Op | Name | `in` | `bin` | Result |
|---|---|---|---|---|
| 2 | `error_message` | failure ordinal | | B: bytes, the message |
| 16 | `lookup` | | qualified name | B: integers, entities |
| 17 | `kind` | value | | V: kind bits (§3.4) |
| 18 | `identifier_of` | value | | B: bytes |
| 19 | `display_string_of` | value | | B: bytes |
| 20 | `parent_of` | entity | | V: entity or 0 |
| 21 | `parameters_of` | function | | B: integers, entities |
| 22 | `declarations_of` | entity | | B: integers, declarations |
| 23 | `entity_of` | declaration | | V: entity |
| 32 | `references_to` | entity | | B: integers, references |
| 33 | `referent_of` | reference | | V: entity |
| 34 | `name_range_of` | declaration or reference | | B: a range (two integers) or nothing |
| 35 | `extent_of` | reference or expression | | B: a range or nothing |
| 36 | `source_text_of` | `r` | | B: bytes |
| 37 | `is_editable` | `r` | | V: 0 or 1 |
| 48 | `calls_to` | function | | B: integers, expressions |
| 49 | `callee_of` | call | | V: entity |
| 50 | `callee_reference_of` | call | | V: reference |
| 51 | `arguments_as_written` | call | | B: integers, expressions |
| 64 | `code_text` | | text | V: code |
| 65 | `code_spell` | entity | | V: code |
| 66 | `code_copy` | `r` | | V: code |
| 67 | `code_cat` | codes, any number | | V: code |
| 80 | `replace` | `r`, code, group | note | V: 0 |
| 81 | `new_group` | | | V: group |
| 82 | `report` | where-kind, a, b, severity | message | V: 0 |
| 83 | `expect_changed` | value | reason | V: 0 |
| 84 | `expect_renamed` | entity | new identifier | V: 0 |

For `report`, where-kind 0 is "no place" (a and b are 0), 1 is a value in
`a`, 2 is a range in `a`, `b`.

Operation numbers 0, 1 and 3 to 15 are reserved to the layer itself.

### 3.3 The result word

```
status = result >> 56                         0: the operation succeeded
status != 0:   result & (2^56 - 1) = the ordinal of this failure in the run (§3.5)
convention V:  result & (2^56 - 1) = the value
convention B:  result = n_bytes << 28 | n_ints         each below 2^28
```

**Convention B is count, then fill.** The host computes the whole answer.
If `n_ints <= cap_out` and `n_bytes <= cap_bout` it writes the answer and
returns the counts. Otherwise it writes nothing and returns the same
counts; the caller calls again with room. Every operation with convention
B is a query, and a query repeated in one run gives the same answer (§2.7),
so the second call is the first one with more room.

### 3.4 Handles and kind bits

A handle is `tag << 48 | index` with an 8-bit tag and a 48-bit index, so it
fits the value of convention V. 0 is the null handle of every sort.

| Tag | Sort |
|---|---|
| 1 | entity |
| 2 | declaration |
| 3 | reference |
| 4 | expression |
| 5 | code |
| 6 | group |

A file handle is a nonzero 32-bit number; a range is a file handle and two
32-bit byte offsets.

The host validates every handle and range it is given: the tag, that the
index is one it issued in this run, and for a range that the file handle
is live and `lo <= hi <=` the file's length. It never trusts a value
because the header made it.

Kind bits (the value of `kind`): bits 0 to 3 are the sort (entity,
declaration, reference, expression); bits 8 to 13 are namespace, type,
function, variable, template, parameter; bit 16 is definition; bit 17 is
call. Other bits are 0.

### 3.5 Failures

1. A status is an `errc` of §4, or one of two values that only a malformed
   call can produce: 254, an operation number the host does not have, and
   255, arguments that do not fit the operation (a wrong `n_in`, text where
   none is taken).
2. A failed operation has no effect and writes nothing.
3. The host numbers the failures of a run from 1. `error_message` returns
   the text for an ordinal of this run, and fails with `invalid_argument`
   for any other number.
4. The host returns Kiln's handler error (which ends the evaluation and
   cannot be caught) only for `host-failure` (§2.8). Nothing a transform
   does, through L1 or by calling the primitive directly, may produce it.
5. A transform that calls the primitive directly is outside the contract
   of L1 and inside the guarantees of this section: it gets a status,
   never undefined behaviour and never access to anything L1 withholds.

### 3.6 Versioning the layer

1. An operation number is never given a different meaning, in any
   release. A retired operation's number stays retired.
2. An operation's `in`, `bin` and result convention never change. A
   change of shape is a new operation with a new number.
3. A new operation takes the next free number of its group (groups are
   the gaps in the table of §3.2 and have no meaning to the host).
4. Because the host accepts only its own headers (§2.2), rules 1 to 3 are
   not needed for correctness today. They are kept so that a host can
   later serve the headers of an earlier release, which is how a
   transform written for an old API level would keep its exact behaviour.

## 4. Errors

```cpp
enum class errc : unsigned {
  invalid_handle = 1, wrong_kind = 2, invalid_argument = 3,
  not_editable = 4, unsupported = 5, limit_exceeded = 6,
};
class error {
 public:
  constexpr errc code() const noexcept;
  consteval std::string message() const;
};
```

1. **Absence is a value.** A query that finds nothing returns a null
   `info`, a null `range` or an empty sequence. It does not throw.
2. **Misuse is an exception.** A host function that is given something it
   does not accept throws `slip::error`. The exception is an ordinary C++
   exception in constant evaluation: the transform may catch it and
   continue. Uncaught, it fails the run (`uncaught`, §2.8).
3. A function that throws has had no effect.

| `errc` | Thrown when |
|---|---|
| `invalid_handle` | an `info`, `code` or `group` that is null where a value is required, or was not produced by a host function in this run; a `range` that is null where one is required or does not lie in a file of the subject unit |
| `wrong_kind` | a live `info` of a sort or kind the function does not accept (§6 says which) |
| `invalid_argument` | text that is not what the function requires: a malformed qualified name, a new identifier that is not an identifier, an empty range given to `replace` |
| `not_editable` | `replace` on a range for which `is_editable` is false |
| `unsupported` | the request is meaningful and this version cannot serve it (each case is listed where it arises). A later version may serve it |
| `limit_exceeded` | a bound of §7.7 |

4. `errc` is open: a transform must not assume that a value it does not
   know cannot occur.
5. `error` is not derived from `std::exception`, so that the header needs
   nothing of the library's exception support in constant evaluation.
6. `message()` is a host function (§2.6). Its text is for a person.
7. The predicates of §6.2 accept every `info`, null included, and never
   throw `wrong_kind`.

Rejected: returning `std::expected` from every query (every call site pays
for an error that correct transforms never see, and absence already has a
value); a sticky error state that later calls inspect (it makes the
meaning of a call depend on earlier calls); `std::meta::exception` (it
ties the header to `<meta>` and to an error vocabulary Slip does not
control); ending the evaluation on misuse (a library of transforms could
not probe what a host supports).

## 5. Values

### 5.1 `info`

```cpp
class info {
 public:
  constexpr info() noexcept;                               // null
  friend constexpr bool operator==(info, info) noexcept;
  constexpr explicit operator bool() const noexcept;       // not null
};
```

An `info` denotes a piece of the subject unit. It has one of four
**sorts**:

| Sort | Denotes | Identity |
|---|---|---|
| entity | a namespace, type, function, variable, template, parameter, or another thing C++ gives a name or declares | the entity: all its declarations in the unit are one entity |
| declaration | one declaration of an entity, as written | that declaration |
| reference | one place where a name is written and denotes an entity; not a declaration's own name | that place |
| expression | one expression as written. Version 0 produces only call expressions and their arguments | that expression |

1. The sort is a property of the value, asked with §6.2. It is not in the
   C++ type, so that later versions can add sorts and kinds without
   changing the type of any function.
2. Two `info` are equal exactly when they denote the same thing of the
   same sort. An entity is never equal to one of its declarations.
3. `info` has no ordering and no hash.
4. `info` is a class type and not a scalar. A consequence in Kiln today:
   the evaluator's cache of pure calls does not apply to functions that
   take or return an `info` (`kiln-landed-2026-11.md` §3.3). Nothing in
   this specification depends on that cache.

Rejected: `enum class info : unsigned long long` (a scalar, so Kiln's
cache would apply; but any integer converts to it with a cast, and the
language would give it relational operators whose results would expose
handle values); one class per sort (the set of sorts must be able to
grow, and typed views can be built on top in L2).

### 5.2 Lifetime and forged values

1. An `info`, `range`, `code` or `group` is meaningful only in the run
   whose host function returned it. A run cannot obtain one from another
   run (§2.4).
2. C++ lets a program build any bit pattern. A value the host did not
   issue in this run is refused with `invalid_handle` wherever a live
   value is required. If the forged bits happen to equal a live value's,
   it is that value. Neither case reaches anything the API does not
   already expose.

### 5.3 Text and sequences

1. Text goes in as `std::string_view` and comes out as `std::string`. It
   is bytes: Slip does not transcode. Subject text is returned as the
   bytes of the file.
2. A sequence comes out as `std::vector<info>`, in unit order unless the
   function says otherwise, with no duplicate.

Rejected: returning `std::string_view` into storage the host promotes to
static (Kiln keeps such storage for the life of the session, so memory
would grow with the number of runs); a lazy range type (it would hold a
host-side cursor, and every `++` would be a host call).

### 5.4 `range`

```cpp
class range {
 public:
  constexpr range() noexcept;                               // null
  friend constexpr bool operator==(range, range) noexcept;
  constexpr explicit operator bool() const noexcept;        // not null
};
```

A non-null `range` is a file of the subject unit and a half-open run of
its bytes, as the file is spelled on disk.

A range is how the API connects syntax to text, and macros are why that
connection can fail. For a piece of syntax `X` (a name, an expression)
with token sequence `T(X)` after preprocessing:

| Tier | Condition | Range |
|---|---|---|
| 0 | there is a run of bytes `R` in a file, outside every macro argument, that translation turns into exactly `T(X)` | `R` |
| 1 | there is a run of bytes `R` inside one argument of one macro invocation written in a file, and one use of that argument in the expansion, such that `T(X)` is exactly what that use of `R` produced | `R` |
| higher | anything else: tokens from a macro's replacement list, tokens made by `##` or `#`, tokens that span an expansion boundary | none |

1. `name_range_of` and `extent_of` return `R` for tiers 0 and 1 and a null
   range otherwise. They never return an approximation.
2. In tier 0, `R` may contain whole macro invocations (the argument
   `MAX(a, b)` of a call written in file text has tier 0).
3. In tier 1, the same `R` is the range of each piece of syntax that each
   use of the argument produced. A transform that edits it requests the
   same edit once per use; identical requests are one edit (§7.6).
4. An edit to a tier 1 range also changes every other use of the
   argument, including a use by `#`. The API does not expose which uses
   exist. Verification sees the result.
5. A null range from these two functions means "not in tier 0 or 1" and
   nothing else; it is not an error.
6. `spec/macros.md` will define tiers for all of Slip. Until it is
   written this section is the definition for the API.

Version 0 has no operation that builds a range from offsets, widens one
or splits one.

### 5.5 As written and as resolved

Every piece of syntax has two views, and each function names the one it
gives:

| As written (what to edit) | As resolved (what it means) |
|---|---|
| a reference; `callee_reference_of`; `arguments_as_written`; `name_range_of`; `extent_of`; `source_text_of` | `referent_of`; `callee_of`; `entity_of`; `parent_of`; the kind predicates |

`referent_of` and `callee_of` return the entity after lookup and overload
resolution: for a call of a function template, the specialisation that was
called; for a member of a class template instantiation, the instantiated
member. Such an entity has no written declaration. Its **origin** is the
written entity it comes from: the template, or the member of the template.
`references_to(e)` and `calls_to(e)` match by origin: a reference whose
referent's origin is `e` is a reference to `e`.

## 6. Queries

In this section "throws `X`" means `slip::error` with that `errc`. Every
function throws `invalid_handle` for a null or forged argument unless it
says otherwise; that is not repeated.

### 6.1 `lookup`

```cpp
consteval std::vector<info> lookup(std::string_view qualified_name);
```

Returns the entities declared with that name in the subject unit, in unit
order of their first declarations.

1. Grammar: `::`, or an optional leading `::` followed by identifiers
   separated by `::`. An identifier is a C++ identifier in the encoding of
   the source. Anything else throws `invalid_argument`: template
   arguments, operator names, whitespace.
2. `::` alone names the global namespace.
3. Matching is by declaration, not by C++ name lookup. The name is walked
   from the global namespace. A component names every entity declared
   with that identifier as a member of a scope reached so far, where the
   members of a scope include the members of the scopes it contains
   transparently: its inline namespaces, its unnamed namespaces, its
   unscoped enumerations, its anonymous unions.
4. Scopes are namespaces, classes, enumerations, and class templates (the
   members of the primary template's definition).
5. Not followed: using-declarations, using-directives, namespace aliases,
   type aliases as scopes, base classes. Not found: specialisations,
   template parameters, entities declared in a function body, entities
   with no written declaration.
6. The result holds every entity so named: each function of an overload
   set is one entity, and a function template is one entity.
7. A name nothing matches returns an empty sequence.

This is decision D2 of the design: a transform names the subject by
strings, because the subject's names do not exist in the transform unit.

### 6.2 Sorts and kinds

```cpp
consteval bool is_entity(info);      consteval bool is_declaration(info);
consteval bool is_reference(info);   consteval bool is_expression(info);
consteval bool is_namespace(info);   consteval bool is_type(info);
consteval bool is_function(info);    consteval bool is_variable(info);
consteval bool is_template(info);    consteval bool is_parameter(info);
consteval bool is_definition(info);  consteval bool is_call(info);
```

1. Each returns false for a null `info`, and throws `invalid_handle` only
   for a forged one.
2. Exactly one of the four sort predicates is true of a live `info`.
3. The kind predicates are true only of entities:

   | Predicate | True of |
   |---|---|
   | `is_namespace` | a namespace, the global one included |
   | `is_type` | a class, an enumeration, a type alias |
   | `is_function` | a function, member functions included; not a function template |
   | `is_variable` | a variable or a data member; not a parameter |
   | `is_template` | a template of any kind |
   | `is_parameter` | a function parameter |

   An entity may satisfy none of them (an enumerator, a concept). Later
   versions add predicates; they do not change these.
4. `is_definition` is true of a declaration that is a definition
   ([basic.def]). `is_call` is true of a call expression in the sense of
   §6.7.

### 6.3 Names

```cpp
consteval std::string identifier_of(info);
consteval std::string display_string_of(info);
```

1. `identifier_of` of an entity or a declaration is the identifier the
   entity is declared with; of a reference, the identifier written there.
   It is empty when the name is not an identifier (an operator, a
   constructor, a destructor, a conversion function, an unnamed entity).
   It throws `wrong_kind` for an expression.
2. `display_string_of` accepts every live `info` and returns a description
   for a person. Its text is not specified. A transform must not parse it,
   compare it or put it into code.

### 6.4 Structure

```cpp
consteval info parent_of(info entity);
consteval std::vector<info> parameters_of(info function);
consteval std::vector<info> declarations_of(info entity);
consteval info entity_of(info declaration);
```

1. `parent_of`: the entity whose scope the entity is a member of: a
   namespace, a class, an enumeration (for an enumerator), a function (for
   a parameter). Inline and unnamed namespaces are parents like any other.
   Null for the global namespace. Throws `wrong_kind` for another sort.
2. `parameters_of`: the parameters, in order, of a function, a function
   template, or one declaration of either. An ellipsis is not a
   parameter. Throws `wrong_kind` otherwise. For an entity the parameters
   are those of its first declaration in unit order (provisional, §8.4);
   the number of parameters is not provisional.
3. `declarations_of`: the declarations of the entity that are written in
   the subject unit, in unit order: every redeclaration and the
   definition; for a namespace, each of its bodies. Empty for an entity
   with no written declaration. Throws `wrong_kind` for another sort and
   `unsupported` for a parameter.
4. `entity_of`: the entity a declaration declares. Throws `wrong_kind` for
   another sort.

### 6.5 References

```cpp
consteval std::vector<info> references_to(info entity);
consteval info referent_of(info reference);
```

1. `references_to(e)`: every reference in the subject unit whose
   referent's origin (§5.5) is `e`, in unit order. Throws `wrong_kind` for
   another sort.
2. `referent_of(r)`: the entity the reference denotes, as resolved. Never
   null. Throws `wrong_kind` for another sort.
3. **What is a reference in version 0.** A name written in one of these
   roles, when it was resolved while the unit was parsed:

   | Role | Example (the bracketed name) |
   |---|---|
   | expression name | `[f](x)`, `&[g]`, `[Enumerator]`, a lambda capture |
   | member name | `a.[m]`, `p->[m]()` |
   | qualifier component | `[a]::[b]::c` |
   | type name | `[T] x;`, `new [T]`, a return or parameter type |
   | template name | `[vector]<int>` |
   | base specifier | `struct D : [B]` |
   | member initialiser | `D() : [B](1), [m](2)` |

4. **What is not yet a reference.** These are written names that denote
   entities and that version 0 does not return:

   | Not returned | Feature test that will announce it |
   |---|---|
   | the name in a using-declaration or using-directive, a namespace alias's target, the qualifier of a declarator (`void [A]::f()`), an elaborated or friend reference, an explicit specialisation's target, a concept name, a designator | `__slip_lib_meta_declarative_references` |
   | a name in a template that is resolved only when the template is instantiated | `__slip_lib_meta_dependent_references` |
   | a name that reached the unit through a precompiled header or a module | `__slip_lib_meta_imported_references` |

5. **Coverage only grows.** A later version returns everything this one
   returns, and more. A transform that edits every reference it is given
   and declares the change (§7.5) is checked by verification, which is
   what catches the references it was not given: the post-image no longer
   resolves, or resolves differently.
6. The host counts, for each entity given to `references_to` or
   `calls_to`, the unresolved names in the unit that are spelled like it,
   and puts the count in the run output (§7.6), so that a report can say
   how much was not examined.

### 6.6 Ranges and text

```cpp
consteval range name_range_of(info declaration_or_reference);
consteval range extent_of(info reference_or_expression);
consteval std::string source_text_of(range);
consteval bool is_editable(range);
```

1. `name_range_of(declaration)`: the identifier token that is the
   declaration's own name. `name_range_of(reference)`: the identifier
   token of the name, without qualifier and without template arguments.
   Null if the name is not an identifier or is not in tier 0 or 1 (§5.4).
   Throws `wrong_kind` for an entity or an expression.
2. `extent_of(reference)`: from the first token of the reference's
   qualifier (a leading `::` included) to the last token of its name.
   Template arguments after the name are not part of it; `typename` and
   `template` keywords before it are not part of it. For an unqualified
   reference it equals `name_range_of`, except that it is also non-null
   for a name that is not an identifier.
3. `extent_of(expression)`: from the first token of the expression to its
   last.
4. `extent_of` returns null when the tokens are not in tier 0 or 1, and
   also when the host cannot establish the first or last token exactly. It
   never returns a range that might be too short or too long. It throws
   `wrong_kind` for an entity and `unsupported` for a declaration.
5. `source_text_of(r)`: the bytes of `r`. Throws `invalid_handle` for a
   null range.
6. `is_editable(r)`: true exactly when `r` is not null and its file is one
   the Slip run may edit (the run's ownership, defined by the pipeline). It
   says nothing about whether an edit there would be accepted by reduce.

### 6.7 Calls

```cpp
consteval std::vector<info> calls_to(info function);
consteval info callee_of(info call);
consteval info callee_reference_of(info call);
consteval std::vector<info> arguments_as_written(info call);
```

1. A **call** is a written postfix expression `callee ( arguments )` whose
   callee, with any parentheses around it removed, is a reference in the
   role of an expression name or a member name: `f(x)`, `N::f(x)`,
   `(f)(x)`, `a.f(x)`, `p->f(x)`, `C::f(x)`.
2. Not calls in version 0: a use of an overloaded operator written as an
   operator; a construction (`T(x)`, `T{x}`, `T v(x)`); a call through a
   pointer, a reference or an object with `operator()`; a call whose
   callee is a dependent name; a call inside an instantiation (the call
   written in the template is the call).
3. `calls_to(f)`: every call in the subject unit whose `callee_of`'s
   origin is `f`, in unit order. `f` is a function or a function
   template; otherwise `wrong_kind`.
4. `callee_of(call)`: the function called, as resolved.
   `callee_reference_of(call)`: the reference of rule 1; never null.
5. `arguments_as_written(call)`: the expressions written between the
   parentheses, in order. A default argument is not written and is not
   included. The object of `a.f(x)` is not an argument. A pack expansion
   `args...` is one written argument.
6. These three throw `wrong_kind` for an `info` that is not a call.
7. A reference to a function that is not the callee of a call (`&f`, `f`
   passed as an argument) is in `references_to(f)` and in no call. A
   transform that must handle every use compares the two (§10.1).

### 6.8 What Kiln provides for each query

Not normative; `kiln-landed-2026-11.md` has the detail and the places.

| Query | Answered from, on Kiln main | Missing |
|---|---|---|
| `lookup`, kinds, `identifier_of`, `parent_of`, `parameters_of`, `declarations_of`, `entity_of` | the declaration arena and its redeclaration chains | nothing |
| `display_string_of` | the names `slip_digest` already renders | nothing |
| `references_to`, `referent_of` | the written-reference table, stages 1 and 2 | a switch that records without printing; stages 3 to 5 for the rows of §6.5 rule 4 |
| `name_range_of`, `extent_of` of a reference | the entry's `range` and `qualifier` chain; the source manager's expansion records | the place of a leading `::`; the end of a token after the unit is built |
| `calls_to`, `callee_of`, `arguments_as_written` | a walk of the written declarations' bodies | nothing |
| `callee_reference_of` | joining the callee's location to an entry's `range` | a statement in Kiln's specification that the two are equal |
| `extent_of` of an expression | `Ast::begin_loc`, `Ast::end_loc` | an end that is exact or absent for every expression kind |
| `source_text_of`, `is_editable` | the source manager; Slip's own ownership | nothing |
| tier 1 ranges | the source manager's expansion records (argument or body) | which other uses an argument has (not needed by version 0) |

## 7. Effects

An effect is a request. It is recorded in the run output (§7.6) and has no
other consequence in the run. What Slip does with a request is decided by
reduce, apply and verification.

### 7.1 Code values

```cpp
class code { public: constexpr code() noexcept; };      // the empty text

consteval code text(std::string_view);
consteval code spell(info entity);
consteval code copy(range);
consteval code copy(info reference_or_expression);
template <class... Pieces> consteval code cat(const Pieces&... pieces);
```

A `code` is a sequence of **pieces** held by the host. There are three
kinds of piece:

| Piece | Made by | Means |
|---|---|---|
| literal | `text(s)` | the bytes of `s` |
| copy | `copy(r)` | the text of `r` in the result of the Slip run: the bytes of `r` with every accepted edit that lies properly inside `r` applied |
| spelled entity | `spell(e)` | a name that denotes `e` at the place the code is put (§7.2) |

1. `text` accepts any bytes.
2. `copy(r)` throws `invalid_handle` for a null range. `copy(x)` is
   `copy(extent_of(x))` and so throws `invalid_handle` when the extent is
   null.
3. **Copy is by reference, not by value.** Two transforms, or two parts of
   one, compose because of this: swapping two arguments does not undo a
   rename inside one of them. An edit lies **properly inside** `r` if its
   range is within `r` and is not the whole of `r`. An edit of exactly `r`
   says what goes where `r` was, and a copy of `r` says where the text of
   `r` goes; neither is applied to the other, which is what lets two
   ranges be exchanged. An accepted edit that shares a byte with `r` and
   is neither within it nor contains it conflicts with the copy. A version
   of Slip that cannot apply an edit properly inside a copied range must
   reject the edits involved as a conflict; it must never apply the stale
   bytes.
4. `spell(e)` throws `wrong_kind` if `e` is not an entity, and
   `unsupported` if `e` cannot be named by a qualified name without
   template arguments: a specialisation or a member of one, a member of a
   class template, a parameter, an entity with no identifier, an entity
   declared in an unnamed class.
5. `cat` concatenates. A piece is a `code`, a `range` (as by `copy`) or a
   value convertible to `std::string_view` (as by `text`). An `info` is
   not accepted: the caller says `spell` or `copy`.
6. Code values are immutable. Adjacent literal pieces are one piece; an
   empty literal is no piece. Two code values with the same pieces are
   interchangeable.

Why code values live in the host: a host call costs about as much as
eight evaluator steps, and building the same text with `std::string` in
the evaluator costs hundreds. More important, a string cannot say "copy"
or "this entity"; a code value can, and those two are what make edits
composable and checkable.

### 7.2 The spelling of an entity

A spelled-entity piece is turned into text by the host when the edit is
recorded. Version 0 has two spellings.

1. **Sibling.** If the edit is `replace(extent_of(R), c)`, `c` is exactly
   one spelled-entity piece for an entity `E`, and all of
   - `E` has an identifier,
   - `parent_of(E)` equals the parent of the origin of `referent_of(R)`,
   - lookup at `R` found the referent directly, not through a
     using-declaration,
   - `name_range_of(R)` is not null,

   hold, then the recorded edit replaces `name_range_of(R)` with the
   identifier of `E`. The qualifier that was written is left as written.
   If several references have that extent (§5.4 rule 3), the conditions
   must hold for each of them.
2. **Fully qualified.** Otherwise the text is `::` followed by the
   identifiers of `E`'s enclosing scopes and of `E`, separated by `::`.
   Unnamed namespaces are skipped; inline namespaces are named.
3. The rule depends only on the subject unit at the place of the edit, so
   every subject unit that sees the same place records the same edit.
4. The API promises that the text is the host's best spelling of `E` and
   that the record says which entity was meant (§7.6). It does not
   promise that the spelling resolves; verification decides. Later
   versions may choose shorter spellings in more places. The text is not
   observable by the transform.

### 7.3 `replace` and groups

```cpp
struct edit_options {
  group atomic = {};
  std::string_view note = {};
};
consteval group new_group();
consteval void replace(range, code, edit_options = {});
```

1. `replace(r, c, o)` requests that the bytes of `r` be replaced by the
   text of `c`. `c` may be empty: that requests deletion.
2. It throws `invalid_handle` for a null range, `invalid_argument` for an
   empty range (insertion is not in version 0), and `not_editable` when
   `is_editable(r)` is false.
3. The host does not check that requests are compatible with each other.
   Two requests of one run for overlapping ranges are both recorded.
4. `new_group()` returns a group with no member. `o.atomic`, if not the
   default, makes the edit a member of that group. Slip applies all
   members of a group or none. A group is local to its run: version 0
   cannot make edits from different subject units atomic.
5. `o.note` is shown beside the edit in reports. It does not affect
   whether two requests are the same edit.
6. `edit_options` gains members over time. A transform must initialise it
   with designated initialisers; other forms are outside the stability
   promise.

### 7.4 Reports

```cpp
enum class severity : unsigned { note = 0, review = 1, error = 2 };
consteval void report(severity, std::string_view message);
consteval void report(info where, severity, std::string_view message);
consteval void report(range where, severity, std::string_view message);
```

1. A report is a message for the person running Slip, attached to a place
   in the subject, or to the subject unit when no place is given.
2. `note` is information. `review` marks a place a person should look at;
   the Slip run is still usable. `error` says the transform could not do
   its job in this subject unit: Slip must not apply the edits of a Slip
   run that has an `error` report unless told to.
3. The place of an `info` is: for a reference or an expression, its
   extent; for a declaration, its name; for an entity, the name of its
   first written declaration. If that is not a range of tier 0 or 1, the
   report is attached to the nearest place the host can name (the macro
   invocation that produced it) and says so. An entity with no written
   declaration has no place.
4. `severity` is open. A value the host does not know throws
   `invalid_argument`.

### 7.5 Declared changes

A transform cannot switch verification off, weaken it, or tell it what to
conclude. It can declare that a specific change is intended; the
declaration is recorded, reported, and checked as far as it can be.

```cpp
consteval void expect_changed(info where, std::string_view why);
consteval void expect_renamed(info entity, std::string_view new_identifier);
```

1. `expect_changed(x, why)` declares that the meaning of the digest unit
   that holds `x` is intended to change. The unit is the entity itself if
   `x` is an entity that `spec/digest.md` §2.1 makes a unit; otherwise the
   innermost such entity whose declaration contains `x`. The host records
   the key of that unit, and the keys of the units of this subject unit
   that `spec/digest.md` §2.2 derives from it (its resolved properties,
   its instantiations).
2. It throws `unsupported` when the unit is keyed by content
   (`spec/digest.md` §3.3) or has no key the digest can compute.
3. `expect_renamed(e, n)` declares that entity `e` is intended to be
   declared with identifier `n` and to be otherwise unchanged, in the same
   scope. The host records the rule of `spec/verify.md` §2.1 that undoes
   it. It throws `wrong_kind` if `e` is not an entity, `invalid_argument`
   if `n` is not an identifier or if this run already declared a different
   name for `e`, and `unsupported` if `e` is not a unit keyed by name.
4. Both are idempotent: declaring the same thing twice is declaring it
   once.
5. **What a declaration does.** `expect_changed` becomes an entry of
   `changed` in `spec/verify.md` §2: verification does not compare that
   unit, requires that it still exists, and compares every other unit.
   `expect_renamed` becomes a rule: verification compares everything,
   under the renaming.
6. **What keeps a declaration honest.** Every declared unit is listed in
   the report of the Slip run with its reason, and the summary states how
   many units were exempt out of how many compared. A declaration that no
   subject unit needed is reported (`spec/verify.md` §2.2). A wrong rule
   cannot make two different programs compare equal (`spec/verify.md`
   §2.1).
7. **A declaration made at a place holds only with an edit there.** When
   `x` is a reference or an expression, the host records its extent with
   the declaration. Slip puts the declaration in force only if it accepts
   an edit of this Slip run that shares a byte with that extent in some
   subject unit; a declaration whose edits were all rejected exempts
   nothing. A declaration made with an entity or a declaration as `x`, or
   with an `x` that has no extent, has no place and is in force whenever
   the run's output is used; the report marks it so.
8. Version 0 cannot declare that an entity is created, removed, or moved
   to another scope.

### 7.6 The run output

This is what a completed run hands to the rest of Slip. The encoding in a
shard is `spec/edits.md`'s; this section fixes the content.

```
RunOutput :=
  transform:  TransformId
  edits:      list<Edit>          sorted by (file, lo, hi, pieces); no two equal
  groups:     list<list<index>>   each a sorted list of two or more edit indices; sorted; no two equal
  reports:    list<Report>        sorted by (file, lo, hi, severity, message); no two equal
  changed:    list<Changed>       sorted by (key, place, reason); no two equal
  renamed:    list<Rule>          spec/verify.md §2.1; sorted as a rule table
  counters:   list<Counter>

Edit :=
  file:    the file's identity (spec/model.md §6): path and content digest
  lo, hi:  byte offsets, lo < hi
  pieces:  list<Piece>
  tier:    0 | 1                  §5.4
  notes:   list<text>             sorted, distinct

Piece :=
    literal(bytes)
  | copy(file, lo, hi)
  | spelled(key: KeyId, text: bytes)      the entity meant, and the text chosen (§7.2)

Report := place: none | (file, lo, hi);  severity;  message;  attached: bool   (§7.4 rule 3)

Changed := key: KeyId;  place: none | (file, lo, hi);  reason: text             (§7.5 rules 1 and 7)
```

1. **Identity is content.** Two requests with the same file, range and
   pieces are one edit, within a run and across subject units. A group's
   identity is the set of its members. Nothing in the output depends on
   the order in which the transform made its requests or on a number the
   host assigned during the run.
2. An edit is in at most one group: groups of one run that share a member
   are one group (all or none is transitive). A group with one member is
   not a group and is dropped; the edit stays.
3. `KeyId` values are in the key space of the pre-image
   (`spec/verify.md` §2).
4. Counters (`spec/model.md`'s `Counter`): `api.host_calls`,
   `api.steps`, `api.peak_bytes`, `api.edits`, `api.reports`,
   `api.unresolved_spelling_matches` (§6.5 rule 6), and one
   `api.op.<name>` per operation used.

### 7.7 Limits of a run

| Limit | Value |
|---|---|
| code values | 2^22 |
| bytes in literal pieces, in total | 2^28 |
| pieces in one code value | 2^16 |
| edits requested | 2^20 |
| groups | 2^20 |
| reports | 2^16 |
| bytes in one note, reason or message | 2^12 |
| bytes in one qualified name or identifier argument | 2^12 |

Exceeding one throws `limit_exceeded`. The limits are part of the API
level and are the same on every machine.

## 8. Stability

### 8.1 The versioned namespace

```cpp
namespace slip { inline namespace v0 { /* every name of this specification */ } }
```

1. A transform writes `slip::lookup`. The inline namespace is the major
   version of the API.
2. While this specification is `draft`, `v0` may change between Slip
   releases; every change raises a feature-test value.
3. When this specification becomes `stable`, the following hold for `v0`
   for as long as a Slip release ships it:
   - no name is removed and no declared signature changes in a way that
     makes a conforming call ill-formed or changes its meaning;
   - a function's documented results do not change, except that sequences
     whose coverage is stated to grow (§6.5 rule 5, §6.7 rule 2) grow,
     and that `unsupported` may become an answer;
   - enumerations marked open gain values; structures marked extensible
     gain members; no other type changes.
4. A change that cannot keep those promises is made in a new inline
   namespace (`v1`). A Slip that makes `v1` inline still ships `v0` as a
   non-inline namespace, and a transform selects it with
   `namespace slip { using namespace v0; }` or by qualifying. `v0` is
   withdrawn only by a release that says so, after one that warned.

### 8.2 Feature tests

| Macro | Value | Covers |
|---|---|---|
| `__slip_lib_meta` | `202611L` | §4, §5, §6.1 to §6.6; also the API level |
| `__slip_lib_meta_calls` | `202611L` | §6.7 |
| `__slip_lib_meta_code` | `202611L` | §7.1, §7.2 |
| `__slip_lib_meta_edits` | `202611L` | §7.3 to §7.5 |

1. A macro is defined by `<slip/meta.h>` exactly when the feature is
   present. Its value is the year and month in which the feature's
   specification last changed, and never decreases.
2. A feature added later gets its own macro. The three named in §6.5
   rule 4 are reserved for the coverage they announce.
3. A transform states what it needs with `#if` and `#error`. There is no
   host function that reports features (§2.6 rule 4).

### 8.3 Adding and retiring

1. **Adding a function** adds a declaration to the header, an operation to
   §3.2 if it needs one, a section here, a conformance test, and a
   feature-test macro or a raised value.
2. **Retiring a function** within a major version is not possible. It is
   marked deprecated in the header (`[[deprecated]]`), keeps working, and
   is absent from the next major version.
3. **Retiring an operation** removes it from the header; the host stops
   serving it when it no longer serves any header that uses it (§3.6).

### 8.4 Provisional names

A provisional name is specified and tested like any other, and is exempt
from §8.1 rule 3: it may be renamed or change shape when the paper it
borrows from settles or when use shows the shape is wrong. Each has a
feature-test macro of its own or is listed here.

| Name | Why provisional |
|---|---|
| `calls_to`, `callee_of`, `callee_reference_of`, `arguments_as_written`, `is_call` | the split between a call as written and as resolved is expected to gain an "operands" view, and member calls an object view |
| `text`, `spell`, `copy`, `cat`, `class code` | expected to be joined, and perhaps renamed, by a token-sequence literal once the C++ code-generation proposals settle |
| `parameters_of` applied to an entity | which declaration's parameters an entity has is unsettled in the reflection papers too |
| `extent_of`, `name_range_of`, `source_text_of` | the names may follow whatever the standard adopts for source ranges |
| `declarations_of` | no counterpart in standard reflection yet |

Everything else in this specification is intended to be carried into the
stable `v0` unchanged.

## 9. Granularities and cost

The design names three granularities: A, a built-in transformation driven
by data; B, native selection with C++ actions; C, C++ throughout.

1. **A is not in this API.** A native rename is a command of Slip, not a
   function a transform calls. Its results and those of a transform meet
   in reduce.
2. **B, in version 0, is the selectors.** `calls_to` and `references_to`
   are evaluated natively over indexes the host builds; the transform
   loops over the result in the same run. There is no callback
   registration and no isolation between sites.
3. **C is everything else** in §6.

Cost model, from Kiln's measurements (`kiln-landed-2026-11.md` §4.4):

| Quantity | Cost |
|---|---|
| a run that does nothing | about 75 ns |
| a host call | about 90 ns plus the host's own work, and one step per integer or byte passed |
| compiling the transform unit, once per worker process | 50 ms to 1.2 s, by how much of the standard library it includes |
| `swap_arguments.cpp` (§10.1), per call site | 15 host calls and a few hundred steps: about 2 µs |
| the same, per subject unit with no site | one run and two `lookup`s: under 1 µs when the name is absent |
| the host's indexes | built on first use in a run: one pass over the reference table for `references_to`, one walk of the written bodies for `calls_to` |

4. **What is memoised.** Nothing, by Slip, in version 0. The host builds
   each index at most once per run. Kiln never caches a call that reached
   the host.
5. The cost of a Slip run with a transform is therefore the cost of
   parsing the subject, plus the indexes a transform asks for. The
   harness records both (`spec/harness.md` §7).

## 10. Examples

The two files are complete programs and are part of the conformance suite
(`## Tests`). They use nothing outside this specification.

### 10.1 Replace `lib::foo(a, b)` with `lib::bar(b, a)`

`resource/examples/swap_arguments.cpp`. For each pair of its table (the
macro `SWAP_PAIRS`, given with `-D`), every call of the first function with two written arguments has its
callee respelled as the second function and its arguments exchanged, as
one atomic group, and the enclosing declaration is declared changed. Every
call it cannot rewrite and every use of the function that is not a call is
reported for review.

### 10.2 Rename a free function

`resource/examples/rename_function.cpp`. Every function named
`RENAME_FROM` at namespace scope is declared renamed to `RENAME_TO`, and
the name is replaced at every declaration and every reference. A place
that cannot be edited is an `error` report, because a rename that misses a
place does not compile.

## 11. The script test format

A script test checks the host against this specification with one subject
unit and one transform. It is a directory:

| File | Content |
|---|---|
| `transform.cpp` | the transform |
| `t.cpp` | the subject's main file |
| any other file | a file of the subject, visible to `#include "..."` |
| `expect.txt` | the expected result |

1. Both units are parsed from memory, as `spec/harness.md` §6.1 describes
   for `slip_kiln::snippet`. The subject's header lines (`// std:`,
   `// target:`, `// args:`) are those of §6.1 there.
2. `transform.cpp` may begin with header lines of the same form:

   | Line | Meaning |
   |---|---|
   | `// spec: §N.M` | required: the section the test covers |
   | `// args: ...` | author's options (§2.1 rule 2) |
   | `// steps: N`, `// bytes: N`, `// depth: N` | budgets (§2.5) |
   | `// owned: no` | no file of the subject is editable |
   | `// xfail: reason` | as `spec/harness.md` §6 |

3. `expect.txt` is the rendering of the result: for a completed run, the
   run output of §7.6, one item per line, places as `file:line:col`,
   keys as the names `spec/verify.md` §6 gives them, counters omitted
   except those the test lists with `// counters: a, b`; for a failed
   run, the failure kind and the diagnostic's first line; for a transform
   that does not compile, the error name. The exact grammar is fixed by
   the work package that builds it and recorded under `spec/landed/api/`.
4. A transform test of `spec/harness.md` §6, which runs the whole
   pipeline, names a programmable transform as a `.cpp` file with the same
   header lines.

## 12. Errors

Errors of the tool, as opposed to `slip::error` in a transform (§4).

| Error | Raised when | Section |
|---|---|---|
| `transform-option` | an option the transform command line may not have | §2.1 |
| `transform-compile` | the transform unit has an error | §2.1 |
| `transform-header` | the unit did not read this build's `<slip/meta.h>` | §2.2 |
| `entry-missing`, `entry-overloaded`, `entry-not-function`, `entry-not-consteval`, `entry-parameters`, `entry-type`, `entry-undefined` | the condition in the table of §2.3 rule 2 | §2.3 |
| `entry-reserved` | a function with a reserved entry name | §2.3 |

Failures of a run are in §2.8.

## 13. Differences from the design document

`docs/design/01-transform-api.md` is illustrative; these are the places
version 0 differs from it, to be folded back there.

| Design | Version 0 | Why |
|---|---|---|
| `inline namespace v1` | `v0` | the draft is not yet the stable contract |
| `on(selector, action)` | a loop over `calls_to(...)` | §2.4, §9 |
| `enclosing_declaration_of(call)` passed to `expect_changed` | `expect_changed(call, why)` | the host knows which unit the digest uses; the transform should not have to |
| `name_range_of(callee_of(call))` | `extent_of(callee_reference_of(call))` | `callee_of` is the resolved entity; the written name is a reference |
| `cat` accepts `info` | it does not | "spell" and "copy" are different requests |
| `info` "is treated as a scalar" | it is not, in Kiln's cache | §5.1 |
| `insert`, `remove`, `create_file`, guards, facts, `relocate`, `class file` | absent | §14 |

## 14. Not in version 0

Reserved for later versions; none of these names may be used for anything
else: `insert`, `remove`, `create_file`, `before`, `after`, `relocate`,
`contribute`, `holds`, `guard`, `fact`, `on`, `file`, `file_of`,
`path_of`, `type_of`, `operands_of`, `object_of`, `origin_of`,
`template_of`, `enclosing_declaration_of`, `statements_of`,
`constant_value_of`, `expansions_of`, `tokens_of`.

## Tests

To be created by the work packages named; none exists yet.

| Path | Covers | Work package |
|---|---|---|
| `crates/slip_kiln/tests/session.rs` | §2.1, §2.2, §2.5: compile once and run many; the files read; each failure of a run as Kiln reports it; a session and a subject alive at once | WP-080 |
| `crates/slip_script/tests/wire.rs` | §3: every operation's shape, both result conventions, count then fill, statuses 254 and 255, validation of forged handles and ranges; §7.6 canonical form | WP-081 |
| `crates/slip_script/tests/header.rs` | the two headers compile in the transform universe; every name of §4 to §7 is declared with the signature given; the feature-test macros; the header compiles with a second compiler when one is installed; `transform-header` | WP-082 |
| `crates/slip_script/tests/subject_entities.rs` | §5.5, §6.1 to §6.4 against Kiln, rule by rule | WP-083 |
| `crates/slip_kiln/tests/ranges.rs` | §1 unit order; §5.4 tiers 0 and 1 and each case that is neither | WP-084 |
| `crates/slip_script/tests/subject_references.rs` | §6.5, §6.6: one test per role; one per row of §6.5 rule 4 showing absence and the counter; a leading `::`; editability | WP-085 |
| `crates/slip_script/tests/subject_calls.rs` | §6.7 rule by rule | WP-086 |
| `crates/slip_digest/tests/api_keys.rs` | §7.5 rules 1 to 3: the unit that holds a place, its derived units, the rule of a rename, each refusal | WP-087 |
| `crates/slip_script/tests/effects.rs` | §7.1 to §7.7 against the fake subject: each piece kind, the sibling rule's conditions one at a time, each throw, identity by content, groups, reports, declared changes, limits | WP-088 |
| `crates/slip_script/tests/entry.rs` | §2.1 to §2.3: each error of §12 | WP-089 |
| `crates/slip_script/tests/contexts.rs` | §2.6: each forbidden context, in a plain function and in a template; the failure repeats in every run | WP-089 |
| `crates/slip_script/tests/budgets.rs` | §2.5, §2.8: each failure kind; a failed run has no effects | WP-089 |
| `crates/slip_script/tests/api.rs` and `tests/api/format/` | §11: the script test format and its runner | WP-090 |
| `crates/slip_script/tests/api/values/`, `lookup/`, `structure/`, `references/`, `ranges/`, `calls/`, `code/`, `effects/`, `errors/` | §4 to §7 from C++, through the header: one directory per section, at least one test per numbered rule | WP-091 |
| `crates/slip_script/tests/api/examples/` | §10: both examples on subjects that exercise every branch | WP-091 |
| `crates/slip_script/tests/isolation.rs` | §2.4 rule 3, §2.7: N runs in any order give the same outputs and step counts; a failed run leaves the next unaffected | WP-091 |
| `crates/slip_script/tests/subject_keys.rs` | §7.5 rules 1 to 3 over Kiln: the keys declared are the keys the digest gives | WP-095 |
| `crates/slip_cli/tests/transform_map.rs` | a transform run through a worker: the run output arrives in the shard; a failing transform fails the observer; one compile per worker | WP-092 |
| `crates/slip_script/tests/cost.rs` | §9: the cost table, measured (not in the gate) | WP-093 |
| `crates/slip_cli/tests/demo3_small.rs` | §10 end to end on a small corpus: plan, map, reduce, apply, verify | WP-094 |

## Open questions

1. **The standard library of the transform universe.** Version 0 uses the
   library Kiln finds on the machine and makes it part of `TransformId`.
   Shipping a pinned library with Slip would make a transform's behaviour
   the same on every machine; it is a packaging and licensing decision.
2. **`__TIMESTAMP__`** depends on a file's modification time in Kiln
   today. Asked for in `docs/requests/KS2-amendment-1.md`.
3. **Units derived from a declared unit.** §7.5 rule 1 has the host list
   the derived units it can see. An edit can also create or remove derived
   units (a call respelled to a function template instantiates a new
   specialisation). Version 0 cannot declare those; verification will
   report them. Whether verification should treat a derived unit that is
   reachable only from exempt units as exempt is a question for
   `spec/verify.md`.
4. **Checking spelled entities.** The run output says which entity each
   spelled piece means. Verification could check that the name at that
   place in the post-image denotes that entity, which would close most of
   what `expect_changed` leaves unchecked. Not specified anywhere yet.
5. **Stringised arguments.** A tier 1 edit inside an argument that the
   macro also stringises (`assert`) changes a string literal in a unit the
   transform did not declare. Verification reports it. Whether a rename
   should declare such units, and how the host would know, waits for the
   preprocessing record.
6. **Atomicity across subject units.** A rename is all or nothing over
   the whole program; version 0 relies on verification to refuse a
   partial one. A run-wide group needs an identity that every subject
   unit can compute independently.
7. **`severity::error` and failed runs** both say "do not apply". The
   override and the exit status are `spec/exec.md`'s to define.
8. **Ownership** (`is_editable`) is defined by the pipeline. The draft of
   `spec/edits.md` defines it as path prefixes; §6.6 rule 6 should cite
   that section once it is accepted.
9. **A second entry per unit, chosen on the command line**, would let one
   file hold several transforms. Kiln's sessions support it; not needed
   yet.
10. **Identifiers outside the basic character set** in `lookup` and
    `expect_renamed`: the rule "as the source encodes them" is untested.
11. **A declaration with no place** (§7.5 rule 7: `expect_changed` given
    an entity) is in force whether or not any edit was accepted. Whether
    version 0 should refuse it, or tie it to "some edit inside a
    declaration of the entity", needs the ranges of declarations
    (request KR3).
12. **Groups seen by several subject units.** A group's identity is its
    member set (§7.6 rule 1). Two subject units that see one edit in
    different groups (a header call whose second argument is edited only
    where a macro is defined) disagree; whether reduce merges the groups
    or rejects the site is `spec/edits.md`'s.
13. **Observers that read an edited range and did not ask for the edit.**
    A native rename says `keep` there; a transform says nothing, and the
    disagreement is found by verification, later and less precisely.
    Whether the host should record, for each file, that a run examined
    it, so that reduce can count silent observers at a site.
