# pitfalls — the catalogue of semantic hazards

Status: draft (M1). The mutation suite of [digest.md](digest.md) §14 and
[verify.md](verify.md): every way an edit that looks meaning-preserving is
known to change a program, each as a test that a correct edit verifies and
a wrong edit does not. The catalogue grows: a mismatch found on a real
codebase that no row explains becomes a row before it is fixed.

The rows test the trusted base, not a transform. Both edits are written by
hand; nothing here depends on how Slip would have produced them.

## 1. The form of a test

One directory per row, `crates/slip_verify/tests/pitfalls/<id>-<slug>/`
(`<id>` as in the tables below, in lower case; `<slug>` of `[a-z0-9-]`):

```
pre/            the subject: sources and headers; main.cpp is the translation unit
good/           the same tree after the correct edit           (optional, see below)
bad/            the same tree after the wrong edit; bad-2/, bad-3/ … for more
pre-2/          optional: the subject that good-2/ and bad-2/ are edits of (pre-3/ … likewise)
declared.txt    the declared changes, shared by good and every bad
declared-2.txt  optional: the declared changes of good-2/ and bad-2/ instead (declared-3.txt … likewise)
expect.txt      what each tree must give
```

- A **tree** is any directory of the row but the `pre` ones: `good`,
  `bad`, `good-<k>`, `bad-<k>`. The tree `<name>-<k>` is an edit of
  `pre-<k>/` if the row has it, else of `pre/`; `good` and `bad` are edits
  of `pre/`. A second subject is how a row shows the **silent** form of a
  hazard: `pre-2/` is `pre/` with a worse-matching fallback added, so
  that the wrong edit still compiles there, without changing what `bad/`
  shows on `pre/`. A numbered subject may also be an independent
  second shape of the row's hazard, with its own `good-<k>` and
  `bad-<k>` (N11, N18): what it must be is what its row says.
- A directory under a subject named `dep/` is read-only code: every tree
  of that subject must contain it unchanged, and the runner checks that.
  It is what the runner gives as read-only, and the rest of the tree as
  owned (`verify.md` §2.1 "Rules rename owned entities only"): a rule
  renames nothing that `dep/` declares.
- Sources are self-contained: no system header. A row that needs a library
  facility declares the few lines of it that it uses.
- Every tree is one observer: the command `clang++ -std=c++20 -I. -Idep
  -c main.cpp` with the tree as its directory (`std: c++17` in
  `expect.txt`, or another, changes the standard). All paths are
  relative, so a subject and its trees are parsed with one command up to
  the directory.
- `declared.txt` is the text of `verify.md` §2.3 (`declared-<k>.txt`
  for the trees numbered `<k>` when the row has that file: a second
  shape that needs other rules, as one rule per moved entity in place
  of the namespace's). It is resolved once per
  tree (`slip declare` with the subject as `--root` and the tree as
  `--post`), since a `created` line and the new parent of a `relocate`
  are resolved in the post-image. In a `good` tree every line must
  resolve. In a `bad` tree, a line that fails with `NoUnit` is left out
  for that tree and the runner says so: a wrong edit may well lack what
  the correct one creates, and a missing rule or exemption can only add
  findings.
- `expect.txt` has one line per tree, and optional lines `std:` and
  `xfail`; a line that is empty or starts with `#` is ignored:

  ```
  std: c++17
  good:  verified
  bad:   not_parsed
  bad-2: mismatch changed app::caller() @ main.cpp:12
  bad-3: mismatch
  xfail bad-2: the walk does not write statements yet (WP-034)
  ```

  - `verified`: the run's verdict is `verified`.
  - `not_parsed`: the observer's state is `not_parsed`.
  - `mismatch <kind> <entity>`: the state is `mismatch` and the report
    has a finding of that kind whose `name` is `<entity>`. With
    ` @ <file>:<line>` the finding's `where.post` (`where.pre` for
    `removed`, and for a `changed` of `verify.md` §4 step 4 whose
    entity the subject declares and the tree derives) must be that
    file, relative to the tree, and that line ("Places" below).
    A tree may have several such lines (`bad-4:` twice): every one must
    be met. A wrong edit that the comparison is bound to report in two
    ways (the template's unit and an instantiation's, N23) lists both,
    because a verifier blind to either passes the other line.
  - `mismatch` alone: the state is `mismatch` and at least one finding is
    neither `indeterminate` nor `derived_indeterminate`. A tree whose
    findings are all indeterminate has not shown the hazard caught; it
    has shown that the walk gave up. Rows that expect that (N20, R11)
    name it: `mismatch indeterminate <entity>`.
  - A row without `good/` says so with `good: none <reason>`, and a
    numbered subject without a correct edit with `good-<k>: none
    <reason>`. Where no correct edit can be digest-equal by design
    (the digest writes what is written: a capture default, a
    functional cast), the reason says that, not that none exists.
  - `xfail <tree>: <reason>` says the tree's line is not met yet and
    why, naming the defect or the package. It changes the outcome as
    below. **It never excuses a `bad` tree that verifies.** It may
    also cover an `ERROR` from `slip declare` refusing the declared
    file (status 1, an error of `verify.md` §2.3), but only when the
    reason quotes, in backquotes, the error's class (`` `syntax` ``,
    `` `no unit` ``) or a word the error quotes (the keyword
    `` `kept` ``): an unrelated error is never hidden. The tree is
    then `XFAIL`. No other `ERROR` is covered.

**The runner** (`crates/slip_cli/tests/pitfalls.rs`; the rows stay with
the trusted base they test) drives the `slip` executable, as a user
would: for each subject of a row it writes a one-entry compilation
database into a directory of its own, runs `slip plan` and `slip parse`
on it, and for each tree of that subject `slip declare` (if
`declared.txt` has a line) and `slip verify --root <subject> --post
<tree> --round <k> --findings 100000 --locate 100000 --report <file>`,
then reads the report (`verify.md` §7.1).

Per tree:

| Outcome | When |
|---|---|
| `PASS` | the line of `expect.txt` is met, and there is no `xfail` for the tree |
| `FAIL` | the line is not met and there is no `xfail`; **or** the tree is a `bad` one and the verdict is `verified`, with or without `xfail` |
| `XFAIL` | the line is not met and there is an `xfail` (and the tree is not a `bad` one that verified) |
| `XPASS` | the line is met and there is an `xfail` |
| `ERROR` | the row is malformed, a `slip` command failed in a way that is no verdict (status 2, 3 or 70; an observer `failed`), or `dep/` differs |

A row is malformed, among other things, when: a `declared-<k>.txt` has
no `pre-<k>/`; a `good-<k>: none` line stands beside a `good-<k>/`
directory; a `pre-<k>/` has neither a `good-<k>/` nor a `good-<k>: none`
line. `declared-<k>.txt` replaces `declared.txt` for every tree of
`pre-<k>/`, good and bad: a declared file belongs to a subject, never to
one tree, or a `bad` tree could carry weaker rules than its `good`.

The test fails on any `FAIL`, `XPASS` or `ERROR`. A `bad` tree that
verifies is a hole in the trusted base and blocks a release. A `good`
tree that does not verify fails too: a verifier that rejects correct
edits is not usable. An `unverifiable` verdict meets no line.

**Places.** The runner has one switch, a constant `PLACES`, with two
states:

| State | A line with ` @ <file>:<line>` is met when |
|---|---|
| `advisory` | the finding is there, and either has no `where` or has it at that file and line |
| `mandatory` | the finding is there with a `where` at that file and line |

- A finding whose `where` is at another file or line does not meet the
  line in either state: a place may be absent before localisation
  exists, never wrong.
- The switch is `advisory` until localisation (`verify.md` §6.1) is on
  main and `mandatory` from then on; the package that merges second of
  the runner and localisation sets it. In the `advisory` state the
  summary counts the lines met without their place (`places skipped`).
- M1's exit needs `mandatory`, with `places skipped` gone from the
  summary.

**The production compiler** (tier T2). Every subject and every tree is
also compiled with `clang++-19`, else `clang++`, found on `PATH`:
`<compiler> -std=<std> -fsyntax-only -I. -Idep main.cpp` in the tree. A
subject and a `good` tree must be accepted; a `bad` tree must be accepted
unless its line is `not_parsed`, and then it must be rejected. A
disagreement is `ERROR`: the row's C++ is not what its author thought, or
it is a Kiln report (then `expect.txt` says so in an `xfail` and
"Open questions" lists it). Without a compiler the check is `SKIP`, and
`ERROR` under `SLIP_REQUIRE_ALL=1` (`spec/harness.md` §1).

**The catalogue against the directories.** Every row of §2 to §4 has
exactly one directory and every directory a row; the runner reads the
ids from this file's tables.

The summary, printed whether or not the test passes:

```
pitfalls: <rows> rows, <trees> trees: PASS <n> XFAIL <n> FAIL <n> XPASS <n> ERROR <n> SKIP(T2) <n>
pitfalls: places <advisory|mandatory>[, <n> skipped]
pitfalls: <n> rows caught only by a failed parse
  <outcome> <row>/<tree>: <what was expected> / <what was found>      every tree that is not PASS
```

Many wrong edits are caught by the parse failing. Those rows are kept: they
document the hazard and pin the verdict. Where a hazard can also change
meaning **silently**, the row has a second `bad` tree arranged so that the
post-image still compiles (a `bad-2/` on a `pre-2/` that adds a
worse-matching fallback), because that is the case only the digest
catches.

## 2. Relocating a namespace

Setting for every row unless it says otherwise: `dep/` declares entities in
`namespace legacy` and keeps them there; owned code declares more in
`namespace legacy` and moves them to a new `namespace acme`. `declared.txt`
is `relocate legacy => :: acme` in its merged form (`verify.md` §2.1): the
rule maps `acme` onto `legacy`.

| Id | Hazard | Correct edit | Wrong edit | The wrong edit gives |
|---|---|---|---|---|
| N01 | Moved code names a staying type without qualification (`Dep`, found through the enclosing `legacy`) | `legacy::Dep` | left as `Dep` | `not_parsed`; with a global `::Dep` in `pre/`: `mismatch removed` the function whose signature held `legacy::Dep` |
| N02 | A use outside the namespace spells a moved entity `legacy::x::T` | `acme::x::T` | left | `not_parsed` |
| N03 | An unqualified call finds a **moved** function by argument-dependent lookup through a **staying** argument type | callee qualified `acme::use(d)` | left | `not_parsed`; with a variadic `::use(...)`: `mismatch changed` the caller |
| N04 | An unqualified call finds a **staying** function through a **moved** argument type | `legacy::touch(m)` | left | `not_parsed`; with a fallback: `mismatch changed` the caller |
| N05 | An operator found through the namespace of a moved operand, declared by the dependency (`a == b` using `legacy::operator==`) | `using legacy::operator==;` added in `acme`, declared `created` (two lines: the using-declaration and its shadow are two units) | left | `not_parsed`; where the operands convert to a built-in type: `mismatch changed` the caller (the built-in operator on converted values) |
| N06 | A dependency template calls a customisation point unqualified (`prepare(t)` inside `legacy::store<T>`); the hook for a moved type is owned | type and hook both move | type moves, hook stays in a `namespace legacy` block | `not_parsed`; with a generic `legacy::prepare` in `dep/`: `mismatch derived_changed` the member of `legacy::store<legacy::Mine>` whose instantiated body holds the call (`put`: a member's body is the member's unit, `digest-kinds.md` §11.9, not the class specialisation's) **and no declared unit differs** Subject 3: the hook is probed, not called (`if constexpr (requires { hook(t); })` in the dependency's `legacy::probe<T>`), so a hook left behind needs no fallback function: `mismatch derived_changed` the probe's instantiation, and no declared unit differs |
| N07 | An explicit specialisation of a dependency template written in an owned `namespace legacy` block | the block is split; the specialisation stays in `legacy` and names `acme::Mine` | moved into `acme` with the rest | `not_parsed` |
| N08 | `using namespace legacy;` in client code that then names a moved entity without qualification | `using namespace acme;` added (the same unit after mapping: nothing to declare) | left | `not_parsed`; with a global of that name: `mismatch changed` the user Subjects 3 and 4: the directive is owned, at global scope, and a dependency header included after it relies on it (`client::use()` calls `make(1)`): `using namespace acme;` added beside it verifies; left, `not_parsed`, and with a global `make(long)`, `mismatch changed` the dependency's `client::use()` |
| N09 | A namespace alias (`namespace L = legacy;`) used to name moved and staying entities | a second alias for `acme`, declared `created` (an alias is a unit), uses of moved entities respelled | alias retargeted to `acme` | `not_parsed` at a use of a staying entity |
| N10 | A using-declaration `using legacy::T;` naming a moved entity | `using acme::T;` | left | `not_parsed` |
| N11 | A friend function or an elaborated type specifier that declares an entity into the nearest enclosing namespace | moves with its class | second shape: the friend is defined outside its class at namespace scope; the class moves and that definition is left in a `namespace legacy` block | `good`: the injected entity is relocated like any other. The wrong edit compiles: the friend now declares an `acme` function nobody defines, the leftover definition is another function, and under the rule the two have one key: `mismatch indeterminate` |
| N12 | `extern "C"` function declared inside `legacy` | moves with the block; its key does not change. Second shape, which is the one that tests the key: declared with one rule per moved entity and **no rule** for the C function, which moves to `acme` too: `verified`, it is the same function whatever its namespace | renamed with a prefix | `mismatch removed` the C function |
| N13 | `inline namespace v1` inside `legacy` | `acme { inline namespace v1 {…} }` | `inline` dropped | `mismatch changed legacy::v1` (and uses through `legacy::` no longer parse; second tree qualifies them `acme::v1::` so that only the namespace differs) |
| N14 | The unnamed namespace inside `legacy` | moves with the block | hoisted out of the namespace; second wrong edit, which is the one that tests the fact: the unnamed namespace's braces removed inside `acme`, so that its members get external linkage | `mismatch removed` its members (both wrong edits; the second differs from `good` only in the unnamed namespace) |
| N15 | An owned macro whose body spells `legacy::make<T>(0)` (an argument, so that the fallback of the silent form is a worse match and not a tie) | body edited | body left | `not_parsed`; with `legacy::make` also declared by `dep/`: `mismatch changed` each function that expands it |
| N16 | A dependency macro that spells `legacy::T` for an argument naming a moved entity | `namespace legacy { using acme::Mine; }` added and declared `created` (two lines) | nothing added | `not_parsed` |
| N17 | A stringised name (`#T` in a macro, giving `"legacy::Mine"`) | use left spelling `legacy::Mine` through a compatibility declaration (declared `created`, two lines), or the unit declared `changed` | respelled `acme::Mine` with nothing declared | `mismatch changed` the user: the string is a value of the program |
| N18 | `typeid(T).name()`, `__PRETTY_FUNCTION__`, `std::source_location` inside moved code | moved; nothing else to do | second shape: the subject consumes the value at compile time (`char buf[sizeof(__PRETTY_FUNCTION__)]`, the length as a template argument) and is moved with nothing more declared | `good`: `verified`, the value, the builtin's own type and the constant built from it are symbolic (`digest.md` §9). The consuming shape has no correct edit short of declaring the unit `changed`: `mismatch changed` the function |
| N19 | A moved class template and an owned partial specialisation of it in another header | both move | the specialisation's block is left in `legacy` | `not_parsed` |
| N20 | A moved function is also declared by the dependency (a forward declaration in `dep/`) | not moved (it is not owned alone) | moved | `mismatch indeterminate legacy::f`, with no place (which of the two sites is reported is not determined): `acme::f` mapped onto the staying `legacy::f` is two entities with one key |
| N21 | Relocation into a namespace that **already exists** and has a function of the name moved code calls unqualified (`helper(1)` meant `::helper(int)`; `acme::helper(long)` now hides it). Declared with one rule per moved entity, not a merged namespace | `::helper(1)` | left | `mismatch changed` the caller |
| N22 | The same, where an entity of the moved name and signature already exists in the destination | — | moved anyway | `good: none`; `not_parsed` if both are defined, else `mismatch removed` the pre-existing `acme` entity, which the rule captured |
| N23 | A default argument or default member initialiser in moved code that names a staying entity | qualified | left | as N01, and the silent tree must show `mismatch changed` the **function whose default argument it is**: a caller's unit holds only the parameter's position (`digest-kinds.md` §7 "Shared nodes"), so no caller differs although each caller's behaviour does. Further shapes: a default member initialiser (`changed` the field); a function template's default argument (`derived_changed` its instantiated default argument) A function template's head (subjects 5 and 6): a default template argument (`changed` the template, and the caller, which names another specialisation) and a requires-clause (the clause is in the template's key: `removed` the template, and `changed` the caller) |
| N24 | A base class named without qualification (`struct Mine : Base`) where `Base` stays | `legacy::Base` | left | `not_parsed`; with a global `::Base`: `mismatch changed legacy::Mine` |
| N25 | A friend declaration in an owned class that befriends a **dependency** function template by an unqualified name (`template <class T> friend int peek(const T &);` meaning `legacy::peek`, which reads a private member; a template, since a dependency included first cannot name the owned class) | `template <class T> friend int legacy::peek(const T &);`, qualified | left: the friend now first declares a new `acme::peek` and the dependency's function loses its access | `not_parsed` where `legacy::peek<Mine>` is instantiated; a second subject that does not instantiate it is silent: under the rule the new function and the staying one have one key, `mismatch indeterminate` the template, with no place. Further subjects: a non-template friend of a function taking a dependency type (`friend int peek(const Probe &);`, declaring `acme::peek` in `acme`): `mismatch indeterminate legacy::peek(const legacy::Probe &)`; the access checked in SFINAE (`peek(const T &t) -> decltype(t.secret)` with a fallback `peek(...)`), silent with a change of behaviour: `mismatch changed` the caller and `mismatch indeterminate` the template; `friend struct Dep;` naming a dependency class, which in `acme` declares `acme::Dep`: `not_parsed` |
| N26 | Argument-dependent lookup through the class a nested type is a member of: `advance(it)` with `it` a `legacy::list<Mine>::iterator` finds an owned hook only through `legacy` (the template arguments of the enclosing class associate nothing) | call qualified `acme::advance(it)` | left | `not_parsed`; with a generic `legacy::advance` in `dep/`: `mismatch changed` the caller |
| N27 | Argument-dependent lookup through a base class that stays: owned `Derived : legacy::Base`, `visit(d)` finds the dependency's `legacy::visit(Base &)` | none at the call: `legacy` is still associated through the base | — | `good` only: a call left unqualified verifies, and so does the same call qualified (two `good` trees). The row is against false findings |
| N28 | A dependency template applies an operator to its argument (`o << t` inside `legacy::log<T>`, `o` of the dependency's `legacy::io::out`, so that `legacy` is not associated through it), found at instantiation through the argument's namespace; the operator for a moved type is owned | type and operator both move | type moves, operator stays in a `namespace legacy` block | `not_parsed`; with a generic operator in `dep/` declared before `log`: `mismatch derived_changed legacy::log<legacy::Mine>(legacy::io::out &, const legacy::Mine &)` and no declared unit differs |
| N29 | Owned specialisations of dependency templates that must stay where they are, in each form: a partial specialisation on a moved template (`traits<acme::Box<T>>`), a variable template's explicit specialisation, a function template's, and one in a nested namespace of the dependency that the owned `namespace legacy` block reopens (`namespace lib { template <> struct hash<Mine> {}; }`, the dependency's `legacy::lib::hash`). With them, what must also stay in the template's scope: a deduction guide for a dependency template (`wrap(Mine) -> wrap<Mine>;`, [temp.deduct.guide]) and an explicit instantiation definition or declaration (`template struct vec<Mine>;`, `extern template struct vec<Mine>;`, [temp.explicit]) | left in place, argument respelled | moved into `acme` with the rest | `not_parsed`, one tree per form |
| N30 | Splitting a `namespace legacy` block puts an explicit specialisation after moved code that already instantiates the template | split so that the specialisation comes first | specialisation after the use | `not_parsed` (explicit specialisation after instantiation) |
| N31 | An inline namespace of the dependency (`namespace legacy { inline namespace v2 {} }`) that owned code reopens to declare its entities | `namespace acme { inline namespace v2 { … } }` | moved to plain `acme` | `mismatch removed legacy::v2::Mine` (and `added`): the parent changed from `legacy::v2` to `legacy`, which the rule does not cover, though every use still compiles |
| N32 | Dependency code, included after the owned code it uses, that reaches an owned entity through its own `using namespace legacy;` (`client::use()` calls `make(1)` unqualified) | `namespace legacy { using acme::make; }` added and declared `created` (two lines) | nothing added | `not_parsed`; with a fallback `::make(long)` (global: the directive's names are found as if declared in the global namespace, so a fallback in `client` would hide `legacy::make` in the subject too): `mismatch changed client::use()` |
| N33 | A name in an owned class template that does not depend on a template parameter binds at the definition to a staying function (`helper()` is `legacy::helper`, never the dependent base's member) | `legacy::helper()` | `this->helper()`, which compiles and now means the base's member | `mismatch changed` the member function of the template; left as it was: `not_parsed`, or with a global `::helper`: `mismatch changed` likewise |
| N34 | The namespace is opened by a dependency macro (`LEGACY_BEGIN` … `LEGACY_END`) around owned code | owned code written with plain `namespace acme { }` (or a macro of its own) | owned code redefines the dependency's macro before a dependency header that uses it is included, moving the dependency's entities too | `mismatch removed` the dependency's entities so moved (silent: everything compiles). Under the merged rule the finding rests on `verify.md` §2.1 "Rules rename owned entities only": `acme::Dep` is declared in `dep/`, so no rule maps it and it keeps `(acme, Dep)`. A second subject declared with one rule per moved entity gives the same finding without that sentence Subject 3: a dependency header included inside an owned block that is then moved: `mismatch removed` the dependency's entity |
| N35 | An owned class that the dependency forward-declares (`namespace legacy { struct Mine; void use(Mine *); }`) | not moved (it is not owned alone); what else is owned moves | definition moved | `not_parsed` where owned code passes the class to the dependency (`acme::Mine *` does not convert to `legacy::Mine *`); a second subject without that call: `mismatch indeterminate legacy::Mine`, the staying incomplete `legacy::Mine` and the moved class have one key, with no place |
| N36 | Argument-dependent lookup with no call written: a range-based `for` over a moved type whose `begin`/`end` are owned free functions; a structured binding through `get<I>` | hooks move with the type | type moves, hooks stay in a `namespace legacy` block | `not_parsed`; with generic hooks in `dep/`, in a namespace the type is associated with that is not `legacy` (`legacy::util`, through a base `legacy::util::Tag`: the lookup is by argument-dependent lookup alone): `mismatch changed` the function that holds the loop or the binding |
| N37 | A dependency macro that forms a **call** (`#define LEGACY_CALL(f, x) legacy::f(x)`) used on a moved function, where the dependency declares an overload of that name | a compatibility `using acme::f;` in `legacy` declared `created` (two lines), or the call written without the macro | left | `mismatch changed` the caller: the dependency's overload is silently taken |
| N38 | An owned `extern "C"` function moves into a namespace that already declares a function of that name (an existing destination: one rule per moved entity, none for the C function): a C function with the same type, or a function with C++ linkage | where the destination's is the same C function, moved: the two declarations are of one function whatever the namespace; else left in a `namespace legacy` block | none where the destination's is the same C function; else moved | `verified`. Where both functions have C linkage there is no subject with conflicting types: they are of one name in C linkage wherever both are visible, so the subject is already ill-formed. Where the destination's has C++ linkage: with the same type, `not_parsed` (a different language linkage); with another type (`acme::c_fn(long)`, called by existing code as `acme::c_fn(1)`), the moved C function overloads it and captures that call: `mismatch changed` the caller |
| N39 | The reverse of N21: a moved function captures the destination's existing calls (`acme::g()` calls `f(1)`, which chose `acme::f(long)`; the owned `legacy::f(int)` moves in). Declared with one rule per moved entity | `f` left in a `namespace legacy` block | moved | `mismatch changed` the destination's caller, code the edit did not touch (N38 `bad-3` is the same capture by a C function) |
| N40 | A moved template's **dependent** unqualified call relied on lookup in `legacy` at the definition (`helper(t)` with `T = int`, `legacy::helper(int)` the dependency's); argument-dependent lookup on `int` adds nothing. N33 is the non-dependent case, N01 the non-template | `using legacy::helper;` in `acme`, declared `created` (the using-declaration and its shadow) | left | `not_parsed` at the instantiation; with a global `::helper(long)`: `mismatch changed` the template (its unit holds the declarations found at the definition, `digest-kinds.md` `UnresolvedLookupExpr`) and `derived_changed` the instantiation |
| N41 | Dynamic initialisation in one translation unit follows the order of definitions ([basic.start.dynamic]): `legacy` opened twice around a global, the variables' initialisers reading each other | each block moved in place | the moved variables gathered into one `namespace acme` block | `mismatch changed <order>`: no initialiser differs (E09's fact, made by a relocation) |
| N42 | The ownership rule against false findings (`verify.md` §2.1 "Rules rename owned entities only"), three subjects: owned code declared through a dependency macro (`LEGACY_DECLARE(Mine)`: the expansion location makes it owned); the dependency also opens `acme` (its `acme::Theirs` is not mapped); an owned overload for a dependency type left in `legacy` and declared `kept` | moved; in subject 3, left and declared `kept` | — | no wrong edit: every `good` tree `verified` |
| N43 | A literal operator moved: owned code uses the cooked `operator""_m(unsigned long long)` through `using namespace legacy;`, and the dependency declares a global raw `operator""_m(const char *)`. A use of a literal operator cannot be qualified | `using namespace acme;` added beside the directive (as N08) | left | `mismatch changed` the user: lookup finds only the raw form, which is then called with the literal's characters ([lex.ext]/3) |
| N44 | An inline function in the unnamed namespace inside `legacy` (`t _ZN6legacy12_GLOBAL__N_16helperEv`, a local symbol) | moves with the block | second shape (`declared-2.txt`): hoisted out of the unnamed namespace into `acme`, beside a function that stays in it, and the hoist declared (`relocate legacy::(anonymous namespace)::helper() => acme :: =`), so every key is as before; it is now `W _ZN4acme6helperEv`, a weak external symbol merged with any other translation unit's | `mismatch changed` the function: its row `enclosing` holds the linkage, which no rule maps (WP-113; before, verified) |
| N45 | A relocation beside a conversion function of a class template (Demo 1's failure on LLVM, WP-131): `W<bool>::operator bool()` is instantiated from `W::operator T()`, whose name is another type. Setting: `relocate a::detail => a :: impl`, which matches nothing of `W`. Four subjects: the three lines of WP-131; a conversion function template beside it; the class template in `dep/`; conversions to a type the relocation moves (`W<a::detail::X>`, `operator a::detail::X()` in a template) | relocated; nothing else touched | the conversion's result type changed in the same edit (`operator long`); `operator T` made `explicit`, so that `return s;` resolves to the template `operator U<int>` | `mismatch changed` the caller. Every `good` tree `verified`: the member keeps its own conversion name under any rule table (`digest.md` §3.4); before WP-131 each was `changed`, `added` and `removed` at the conversion function |
| N46 | A move into a namespace that `dep/` also opens (WP-140, `verify.md` §2.1 "So `acme` mapped to `legacy`"). Setting: `dep/` opens `acme`, not `legacy`. Five subjects: the move; nested, `acme::detail` opened by both; `dep/`'s `acme::v1` inline where `legacy::v1` was not; a member left in `legacy`, declared `kept`; `dep/` also opening `legacy` (the motivating migration's shape: the dependency keeps the old namespace) | moved into owned openings of `acme`; in subject 4, `h` left and declared `kept` | the moved opening made `[[deprecated]]`, which deprecates the dependency's `acme`; subject 3, `legacy::v1` moved into the inline `acme::v1` (no correct edit) | `mismatch changed acme`; `mismatch changed legacy::v1`. Every `good` tree `verified`: `acme` has a unit in the owned view, keyed `legacy`, with the namespace's one digest (`digest.md` §3.4); before WP-140 each was `mismatch removed legacy`. A member left behind with no `kept` is the run's stale check, which has no line here (`rules/views/member-left`) |

## 3. Renaming an entity

Setting: one entity is renamed in place; `declared.txt` is one `relocate`.

| Id | Hazard | Correct edit | Wrong edit | The wrong edit gives |
|---|---|---|---|---|
| R01 | The new name joins an existing overload set, and a call that chose another member of it now prefers the renamed function (`g(1)` chose `g(long)`; `f(int)` becomes `g(int)`) | — | renamed | `good: none`; `mismatch changed` the caller of `g` |
| R02 | A renamed member hides a base-class member that the class's own functions use unqualified | uses qualified `Base::y` | left | `mismatch changed` the member function |
| R03 | A virtual function renamed in the base and not in one override (no `override` written) | every override renamed, one rule each | one left | `mismatch` on the derived class or its function: it no longer overrides (`digest.md` "Open questions", overriding relations) Subject 2, the opposite: `B::f` renamed to the name of a non-virtual `D::g()`, which starts overriding: `good-2: none`; `mismatch changed` the derived function |
| R04 | A renamed customisation point called unqualified from a dependency template | — | renamed | `good: none`; as N06 |
| R05 | A local variable renamed to the name of another local in an enclosing scope that the block also uses | another name; nothing declared (a local is no unit) | that name | `mismatch changed` the function. A second `good` tree renames the outer local: names of locals are not digested |
| R06 | A name produced by token pasting (`PREFIX##_init`) | the macro argument edited | the definition edited, the paste left | `not_parsed`; with a global function of the old name in `pre-2/`: `mismatch changed` the caller, which the paste now resolves to the global |
| R07 | A member renamed to a name that a `requires` or `decltype` probe elsewhere tests for (`requires { t.size(); }`), making a constrained overload viable for the class | — | renamed | `good: none`; `mismatch changed` the caller that now selects the constrained overload, or `derived_changed` on the template that does Subject 2, the opposite: the probed member renamed, so `if constexpr (requires { t.size(); })` takes its other branch: `good-2: none`; `mismatch derived_changed` the instantiation, and no declared unit differs |
| R08 | Two names swapped (`f` ↔ `g`), two rules | every use swapped | one call left | `mismatch changed` that caller |
| R09 | A renamed enumerator used as a template argument and in a `case` | all uses | — | `good` only: constant values are by value, the enumerator by key |
| R10 | A renamed type whose name was reached through an alias elsewhere | alias target respelled | — | `good` only |
| R11 | A function renamed where the translation unit keeps a block-scope declaration of the old name (`void g() { void f(int); f(1); }`) | that declaration renamed | left | `mismatch indeterminate`, no place: the stale declaration and the renamed function are two entities with one key. (The same staleness in a translation unit that does not see the renamed declaration is `verify.md` §8, agreement between observers.) Subjects 2 and 3: a friend declaration left (`friend int helper(X &);`), which declares a new function ([namespace.memdef]/3): where the renamed function used the friendship, `not_parsed`; where the friend is the definition, `mismatch indeterminate`, no place (two entities with one key; the program does not link) |
| R12 | A renamed function's name observed through `__func__` | renamed (subject 1: `__func__` as a value) | — | `good`: the value is symbolic (`digest.md` §9). Two more subjects where the rename has no correct form, each `good-<k>: none` and its `bad-<k>`: the length as an array bound (`char a[sizeof(__func__)]`) and as a value (`return sizeof(__func__);`): `mismatch changed` the function |
| R13 | A `using Base::f;` in a derived class: renaming one overload of `Base::f` drops it from the using-declaration's set, and `d.f(1)` takes `Derived::f(long)` | the call through `Derived` renamed too; declared besides the rule: the shadow `removed`, `Derived` `changed` | left | `mismatch changed` the caller |
| R14 | `__attribute__((cleanup(fn)))` names a renamed function through ordinary lookup (unlike the names in strings of `verify.md` §8) | renamed | left | `not_parsed`; with a global function of the old name: `mismatch changed` the function whose variable has the attribute |
| R15 | A stringising registration macro (`REGISTER(f)` expanding to `reg("f", &f)`): the program holds the old name as a string. The rename counterpart of N17 | the expansion written out with the old string | the macro's argument renamed | `mismatch changed` the registering function: the string is a value |
| R16 | A class template renamed, with a user deduction guide and an alias template through which class template argument deduction runs ([over.match.class.deduct]/3) | the guide and the alias renamed with it | the guide left | `not_parsed`. `good` is against false findings: the guide and the specialisations are keyed by the template's key (`digest.md` §3.2), so they map |
| R17 | Names the language looks up: a member `begin` renamed makes a range-based `for` fall back to a free `begin` found by argument-dependent lookup; a member `get` renamed makes a structured binding fall back to a free `get` | — | renamed | `good: none`; `mismatch changed` the function with the loop or the binding |
| R18 | A template parameter renamed to the name of a member of a non-dependent base, which hides it in the template's definition ([temp.local]/9). Nothing declared: a template parameter is written by position | another name | the base member's name | `mismatch changed` the member (the pattern) and the caller, and `derived_changed` the instantiation |
| R19 | The new name is a function-like macro's (`LOG(x)`): every use is expanded | — | renamed | `good: none`; `not_parsed`; where the macro forwards its argument: `mismatch removed` the renamed function (no entity is named `LOG`) and `changed` a caller of the overload set the expansion joins |
| R20 | The new name hides a name that a default looks up: a default member initialiser, a default template argument. The rename analogue of N23 | — | renamed | `good: none`; `mismatch changed` the field; for the template, `changed` the template and the caller |
| R21 | A local class renamed whose name the program reads (`__PRETTY_FUNCTION__` in a member). Nothing declared: a local type is no unit and its name is not digested (`digest.md` §4.3) | where nothing reads the name, renamed: `verified` | renamed where it is read | `good-2: none`; `mismatch changed` the function |
| R22 | An elaborated-type-specifier left after a class rename declares a new class: `struct Old *` in a namespace-scope function's parameter ([basic.scope.pdecl]/7), or `friend struct Old;` ([namespace.memdef]/3), which befriends the new class instead. The class analogue of R11's left friend | renamed | left | `mismatch indeterminate` the class, no place (the stale class and the renamed one have one key), and `mismatch changed` the user: the caller that now takes another overload, or the member whose access probe now fails in substitution |
| R23 | A pragma that names the renamed function left: `#pragma weak f`, `#pragma redefine_extname f sym`. The front end attaches the attribute to the declaration the pragma names, so this is not `verify.md` §8's names in strings | the pragma renamed | left | `mismatch changed` the renamed function (it loses `weak` or its symbol name). `redefine_extname` applies to C language linkage, whose renaming waits on the rule of `verify.md` §2.3 |

## 4. Edits that rename nothing

Setting: `declared.txt` is empty, except where a row says otherwise (E11's `removed` lines and the `changed` lines of `declared-4.txt` and `declared-5.txt`, E12's `created` line); the edit claims
layout only.

| Id | Hazard | Correct edit | Wrong edit | The wrong edit gives |
|---|---|---|---|---|
| E01 | Qualifying a member call suppresses virtual dispatch | `this->f()` | `Base::f()` | `mismatch changed` the caller |
| E02 | Qualifying a call disables argument-dependent lookup (`using std::swap; swap(a, b);` with a user `swap`) | the other call qualified, `std::swap(i, j)` on `int`, where no argument-dependent lookup applies | `std::swap(a, b)` as well | `mismatch changed` the caller |
| E03 | Parentheses that are not redundant: `decltype((x))`; `(f)(x)` | `((x) + 1)`; a second tree with redundant parentheses around operands of `+` | `decltype((x))` (`bad`), `(f)(x)` (`bad-2`) | `mismatch changed` the function, in each `bad` tree |
| E04 | Reordering `#include`s changes what a non-dependent call inside a template sees at its definition | an order that keeps it | the other order | `mismatch changed` the template Subject 2: a partial specialisation's header moved after a use that instantiates the class (ill-formed, no diagnostic required, [temp.class.spec]/1; clang and Kiln both keep the primary's instantiation): `good-2: none`; `mismatch derived_changed` the specialisation and its member |
| E05 | Moving a function definition past a later, better overload (`h(long)`, caller, `h(int)`) | moved past another function only, still before `h(int)` (a cast that keeps `h(long)` is another expression: "Open questions") | moved past `h(int)` | `mismatch changed` the caller |
| E06 | Respelling through a namespace alias, a using-declaration, an injected class name, a typedef of the same type | any of them | a typedef that adds `const` | `mismatch changed` Subject 5: a parameter respelled through an alias, in a function that returns `__PRETTY_FUNCTION__` (which prints the type as written) and in one where nothing prints it: `verified`, because a name-derived string only passed on is equal in the unit digest whatever changes its content (`digest.md` §9); the names report (WP-112) is what tells the user. The same respelling in a function that consumes the string (`return sizeof(__PRETTY_FUNCTION__);`, through an alias of another length): `mismatch changed` the function, the consumer is written by value |
| E07 | `struct` to `class` on a redeclaration; `override` added; a parameter renamed; a redundant `this->` | each | — | `good` only |
| E08 | An initialiser respelled so that `auto` deduces another type (`auto x = 1;` to `auto x = 1u;`) | `auto x = (1);` | `1u` | `mismatch changed` |
| E09 | Two namespace-scope variable definitions with dynamic initialisers exchanged | two functions exchanged | the variables | `mismatch changed` the order unit |
| E10 | A default argument changed on one declaration | — | changed | `good: none`; `mismatch changed` the function whose default it is (a caller that relies on the default writes only the parameter's position) |
| E11 | Removing an `#include` that only seemed unused: it held an explicit class specialisation, a better overload, or an explicit function specialisation | an include nothing depended on | that include (`bad`, `bad-2`, `bad-3`) | `declared.txt` has a `removed` line for everything each removed header declared. `bad`: `derived_changed` the instantiation that used the class specialisation; `bad-2`: `changed` the caller of the overload; `bad-3`: `changed` the function specialisation, now declared in one image and derived in the other (`verify.md` §4 step 4); `bad` gives `changed traits<Mine>` for the same reason beside its `derived_changed`. Subject 4: an include replaced by a forward declaration, where `delete p` on the now incomplete type calls no destructor ([expr.delete]/5): `good-4: none`; `mismatch changed` the function. `good-5` (subject `pre-5`): `bad-3`'s edit with `declared-5.txt`'s `changed pick<int>()`, what a tool that removes the specialisation on purpose declares: verified. `pre-6` keeps the class specialisation in every tree and differs only in what an instantiation finds: `bad-6` removes `tag.h`, whose `tag(Mine)` the instantiation `box<Mine>` found by argument-dependent lookup, and gets `derived_changed box<Mine>::found` with no declared unit differing (what a check of declared units alone misses); `good-6` removes `unused.h` |
| E12 | A declaration inserted in an inner namespace shadows a name used later in it | inserted with a fresh name, declared `created` | with the used name | `mismatch changed` the user (and `added` the inserted declaration). A block scope cannot carry the row: any statement inserted into a block changes its function Subject 2 is the block scope, with `good-2: none`: a declaration inserted there that hides the used name gives `mismatch changed` the function, never silence |
| E13 | Lines inserted above a use of `__LINE__` | — | inserted | until locations are symbolic (`digest.md` "Open questions"): `mismatch changed` the user. This row pins today's behaviour and is rewritten when that question closes |
| E14 | A `using namespace` moved from function scope to namespace scope | — | moved | `good: none`; `mismatch changed` a later function whose lookup now finds something else (also `added` the directive, `changed` the function it left) |
| E15 | A conversion made explicit where the implicit one chose another path (`T x = y;` to `T x(y);` with an `explicit` constructor present) | — | `T x(y)` | `good: none`, by design: a written functional cast `T(y)` is not the implicit conversion; `mismatch changed` |
| E16 | A lambda's default capture replaced, or a capture's kind changed | — | `[=]` for `[&]`; on a second subject `[n]` for `[&n]` | `good: none`, by design: the capture default and the implicit bit are written (in a template, for `this`, for unevaluated uses and constants not odr-used, the default decides what is captured); `mismatch changed` the function Subject 3: a template, and `[&]` against `[&n]` with `n` a constant that is not odr-used, so the closures differ in size: `mismatch changed` the template and `derived_changed` the instantiation |
| E17 | Reordering `#include`s changes which operator functions a dependent operator expression finds at the template's definition ([temp.dep.candidate]; `digest-kinds.md` §9.5 `oper_lookups`): `::operator==(a::X, a::X)` against the built-in `==` on `X` converted to `int` | an order that keeps the operator before the template | the operator's header after the template | `mismatch changed` the template |
| E18 | Reordering class members: data members (positional aggregate initialisation; mem-initializers run in declaration order, [class.base.init]/13), virtual functions (the virtual table's order) | non-virtual member functions reordered; the written mem-initializers reordered | data members or virtual functions reordered | `good: none` for the aggregate; `mismatch changed` the class (and the constructor whose mem-initializers now run in another order); for virtual functions also `derived_changed` the class's `<key_function>` |
| E19 | Removing `inline` or `constexpr`: `constexpr` from a function whose call initialises a variable (constant initialisation turned dynamic, `__builtin_is_constant_evaluated()` flipped); `inline` from a header variable (a definition in every translation unit, [basic.def.odr]) | — | removed | `good: none`, by design: a written `inline` is digested apart from an implied one (`digest-kinds.md` `0-inline`, `0-implicit-inline`); `mismatch changed` the function or the variable |
| E20 | `&x` respelled as `addressof(x)` (a row-local one, over `__builtin_addressof`) where `operator&` is overloaded, and the reverse | — | respelled | `good: none`; `mismatch changed` the function |
| E21 | Reordering `#include`s where the preprocessor decides: a `#define` that a later header tests; a header moved into a `#pragma pack` region; include guards replaced by `#pragma once` where another header tests the guard macro. Distinct from E04: no lookup differs, the tokens do | an include nothing tests moved; a header moved across a `#pragma pack` region without entering it; `#pragma once` for a guard nothing tests | the other | `mismatch changed` the entity the tokens make: the typedef and its user, the packed class, the function |
| E22 | Spellings taken as equivalent that are not, one subject each: `S() {}` to `S() = default;`; `0` to `nullptr`; `push_back(5)` to `emplace_back(5)` with an explicit constructor; `for (auto x : a)` to `for (auto &x : a)`; a cast to an rvalue removed from the `return` of a reference parameter; `enum` to `enum class`, its uses qualified; a cast to an rvalue added to the `return` of a local, which disables the named return value optimisation | — | respelled | `good: none` each; `mismatch changed` the user, and the class or the enumeration where it is the one respelled |
| E23 | Reordering base classes: initialisation order ([class.base.init]/13.2) and layout follow the base-specifier-list. Name lookup does not ([class.member.lookup]) | — | reordered | `good: none`: the base order is the class; `mismatch changed` the class |

## 5. What a row is for

Each row names the fact the digest must hold for the wrong edit to be
caught. When a row's test first passes, the rows of `digest-kinds.md` that
carry that fact name it as their must-differ test where they have no
smaller one (`digest.md` §13 obligation 5). A row whose wrong edit is
caught only by a failed parse (`not_parsed` in every `bad` tree) is marked
so in its `expect.txt`, and counted separately in the suite's summary: it
exercises the pipeline, not the digest.

## Tests

- `crates/slip_cli/tests/pitfalls.rs` — runs every directory under
  `crates/slip_verify/tests/pitfalls/` as §1 says, and fails if a row of
  this file has no directory or a directory has no row. Its own cases, on
  rows the test writes to a temporary directory: each outcome of the
  table, a `bad` tree that verifies with an `xfail` (still `FAIL`),
  `mismatch` alone against findings that are all indeterminate, the `@`
  place one line off (not met in either state of `PLACES`), a place
  absent (met in `advisory` and counted, not met in `mandatory`), a
  `bad` tree whose `created` line does not
  resolve, a changed `dep/`, a malformed `expect.txt`, `pre-2/`.

## Open questions

- A row is resolved and verified through the executable, some ten
  process starts per tree. If the suite's time matters, the runner can
  call `slip_cli`'s functions in process; the executable is the form a
  user's result has, which is why it is the first.
- `xfail` lines are the list of what the trusted base does not catch
  yet. M1's exit needs none (`docs/design/06-engineering-plan.md` §5);
  the summary line is what is watched.
- Whether every `mismatch` line should carry an `@` place, so that the
  catalogue pins localisation for every row
  (`docs/design/06-engineering-plan.md` §3 "caught with the right
  location"). Today a row gives one where the hazard sits on one line;
  where it is given it is checked as "Places" says.
- R11 and the agreement between observers: the single-observer form is
  written above; the form with two observers (one sees the renamed
  declaration, the other only a stale one) needs the check that `verify.md`
  §8 defers, and a test format with more than one translation unit.
- Rows for macros beyond N15–N17 and R06 wait for `spec/macros.md`.
- Rows that need more than one configuration (an entity declared under
  `#if` in one observer only) wait for the coverage report.
- Coroutines, modules and Objective-C are not represented.
- Whether a non-dependent explicit cast whose conversion equals the
  implicit one (`h(static_cast<long>(1))` for `h(1)` calling `h(long)`)
  should be written as that conversion. It would make edits that pin an
  overload verify (E05's cast form); it needs a normalisation rule in the
  trusted base, and constant folding for `h(1L)`. Undecided; the digest
  is strict for now.
- The production-compiler check of §1 uses whatever `clang++` provisioning
  installs; a row that the two compilers disagree on is a Kiln report, and
  is marked in `expect.txt` until it is resolved.
