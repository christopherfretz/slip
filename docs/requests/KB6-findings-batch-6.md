# KB6 — findings for Kiln, batch 6 (2026-11-23)

From Slip's packages WP-071, WP-073, WP-086, WP-107, WP-113 and WP-115
(merged), WP-054 and WP-120 (in review), WP-041 (in its fix round:
section 9 is provisional), and their independent reviews, at Kiln pin
`913a490bb`. As in [KB1](KB1-findings-batch-1.md) to
[KB5](KB5-findings-batch-5.md), Slip asks only for an answer per item
(a work package number, "as designed", or "not now"). **One item is
pressing: 2.5.** A class given an `aligned` attribute on a
redeclaration after its definition is laid out in two ways by Kiln,
depending on where the first layout happens. Slip's digest cannot see
the difference, so it gives a wrong `equal`, and Slip has no
workaround. Item 1.1 is a feature request and a design item, asked on
the operator's word that Slip may ask Kiln for the features it needs.
WP-052 has nothing new for Kiln (below, "Not in this batch").

Every Kiln citation below was checked at the pin by reading. No item
was checked against Kiln's main: some may already be fixed there.

## How to read an item

**Kind** is one of four. A **defect**: Kiln does something wrong by the
C++ standard, by clang 19 (Slip's reference, `clang++-19`), or by its
own documented record. A **missing record**: the AST or a table does
not keep something that Slip needs. A **question**: the answer may be
"as designed". A **feature request**: something Kiln does not have.
Each item is also marked **design** (answering needs a decision about
what Kiln's representation should be: for Kiln's design agent) or
**implementation** (the right behaviour is not in doubt).

**Slip's need** says which Slip behaviour depends on the item. Slip's
terms, used there:

- **The digest** is Slip's hash of what each entity means, computed per
  translation unit before and after an edit. Two versions of an entity
  are **`equal`** (unchanged), **`differ`** (changed), or
  **indeterminate** (Slip refuses to judge and names a cause, such as
  `spelling/using-enum`). A wrong `equal` is the failure that matters
  most: an edit changed a meaning and Slip says it did not. A false
  difference (a "cost") is noise, never unsafe.
- **A pair test** is a directory with one source in two versions (side
  A, and side B under `#ifdef B` or in a second file) and the verdict
  the digest must give. A test marked `# pending` with `# needs: …`
  waits for what it names, often Kiln.
- **A unit's own spelling** is how that unit wrote a type, qualifier or
  bound, as distinct from its canonical form. Slip writes the own
  spelling where it can change meaning.
- **Verification** re-parses the edited program and compares digests;
  an edit is `verified` only if every entity is as the request says.
- In Slip's rename, **the seed** is the declaration a request names,
  and **a follower** is a declaration Kiln derived from it (a member of
  an instantiated specialisation, a using-declaration's shadow) that
  must take the new name with it. A rename that meets a name it cannot
  place **flags** the seed: `unresolved` (a dependent name it cannot
  follow), `unreachable` (a written name it cannot claim),
  `unsupported`, or `mixed_follower`. A flag refuses the run: an
  over-refusal costs the user, never correctness.
- **The subject** is Slip's view of a unit for transform scripts: its
  calls, their arguments, and the source extents of expressions.

Paths are Kiln's (from Kiln's root, without `crates/`) unless they
start with `slip:`, which is Slip's repository root. A `slip:` path of
WP-041, WP-054 or WP-120 is on Slip's main once that package merges.

## Summary

31 items: 1 feature request, 12 defects, 13 missing records and 5
questions. Six of them (section 9) are provisional.

By need, the most pressing first:

1. **2.5**: a class `aligned` on a redeclaration after its definition.
   A wrong `equal` against what Kiln emits; no workaround.
2. **4.1 and 5.3**: a friend whose target Kiln keeps only as a function
   type. The rename misses the name (4.1: verification then reports
   it) or refuses every rename of a member of a class template that
   such a friend could name (5.3).
3. **4.2**: `n::v<T>` (a variable template with dependent arguments) has
   no referent, so every rename that meets one in a template refuses.
4. **1.1**: the use behind a source-location string. Six pending false
   differences, and two named holes that would be wrong `equal`s in
   C++26.
5. **2.2 and 2.4**: `alignof` and the layout read different
   declarations of an aligned typedef. Slip makes such a typedef and
   every unit that meets it indeterminate.
6. **6.1 and 6.2**: constant array bounds and a partial specialisation's
   non-type arguments keep no expression, so the subject misses the
   calls written there.

The questions (2.6, 2.7, 3.1, 5.6, 6.3) are last in Slip's order.

| # | Kind, mark | Title |
|---|---|---|
| 1.1 | feature request, design | Which use made a source-location string |
| 2.1 | defect, implementation | `alignof` of a reference drops the referee's sugar |
| 2.2 | defect, implementation | `alignof` and the layout read different declarations of a typedef |
| 2.3 | defect, implementation | `.*` and `->*` drop the pointee's sugar |
| 2.4 | defect, implementation | The layout reads later typedef redeclarations and caches by first use |
| 2.5 | defect, implementation | A class's layout depends on where it is first laid out |
| 2.6 | question, implementation | `Ast::vtables` is never filled under `-fsyntax-only` |
| 2.7 | question, implementation | `ms_struct` after the definition is accepted and has no effect |
| 3.1 | question, design | An explicit specialisation chained to a declaration in the template's opening |
| 3.2 | defect, implementation | `redecls_of` of a function template can miss its first declaration |
| 3.3 | defect, implementation | The implicit `std::align_val_t` is not chained with a written one |
| 4.1 | missing record, design | A dependent friend function template specialisation |
| 4.2 | missing record, implementation | `n::v<T>` with a non-dependent template name has no referent |
| 5.1 | missing record, implementation | The join from a dependent entry to its expression |
| 5.2 | missing record, implementation | A member's explicit instantiation: no target entry, the pattern's location |
| 5.3 | missing record, design | A friend of a member of a dependent specialisation |
| 5.4 | missing record, implementation | A member template specialised as a template has no link |
| 5.5 | missing record, implementation | An explicit instantiation after an explicit specialisation |
| 5.6 | question, implementation | Members of an explicitly instantiated class are `ImplicitInstantiation` |
| 6.1 | missing record, design | A constant array bound keeps no expression |
| 6.2 | defect, implementation | A partial specialisation's non-type argument keeps no expression |
| 6.3 | question, implementation | GNU `__sync_*` builtins and calls of a declared function |
| 7.1 | missing record, implementation | `CXXDefaultArgExpr` has no location |
| 8.1 | defect, design | Hidden using-shadows depend on declaration order |
| 8.2 | missing record, implementation | An instantiated using-declaration's shadows are not linked to the pattern's |
| 9.1 | defect, implementation | Addition to KB5 1.1: a bound naming a member through a dependent qualifier |
| 9.2 | defect, implementation | A dynamic exception specification in C++17 emits no filter |
| 9.3 | missing record, implementation | A qualified pseudo-destructor keeps no written qualifier |
| 9.4 | defect, implementation | A pseudo-destructor drops its qualifier's instantiation dependence |
| 9.5 | missing record, design | `using enum` keeps no written qualifier |
| 9.6 | missing record, design | A destructor named by member access keeps no written name |

## 1. Feature request: the use behind a source-location string (WP-115)

### 1.1 Which use made a source-location string

- **Kind**: feature request. **Design.**
- **Source** (`slip:crates/slip_digest/tests/equal/_/name-value-cost-overload-plain-rename`,
  side A):
  ```cpp
  void take(const char *);
  void f(int) { constexpr const char *p = __builtin_FUNCTION(); take(p); }
  void f(long) { constexpr const char *p = __builtin_FUNCTION(); take(p); }
  ```
- **Kiln today.** Kiln makes one string object per content for the
  strings that `__builtin_FUNCTION()`, `__builtin_FILE()` and
  `std::source_location` produce (the plain and pretty function names,
  the file name, the file's base name): `srcloc_string` and
  `frozen_string` (`kiln_sema/src/builtins.rs:3680`, `:3687`, called
  from `record_source_loc`, `:3612`). Both `p` above point to one
  object `"f"`. `frozen_string` also serves reflection's strings
  (`kiln_sema/src/meta/names.rs`, `meta/value.rs`,
  `meta/query/entity.rs`, `meta/recipe.rs`, `meta/ext.rs`), and a
  contract site interns its file, its function's pretty name and its
  predicate's text into the same objects (`kiln_sema/src/contracts.rs`
  `record_contract_site`, `:210`, the strings at `:225`). So a
  predicate written `get` shares the object of `get()`'s plain name.
  Nothing records which made an object.
- **Asked.** For a pointer to such a string held in a constant, a way
  to know which use produced it: which call of which builtin, evaluated
  in which declaration, in which role (plain function name, pretty
  function name, file name). Either the use kept with the pointer (an
  origin record: the builtin's kind and the declaration it was
  evaluated in), or one object per use.
- **The invariant Slip would rely on**: every pointer to a Kiln-made
  string object held in a constant leads to one maker, which is either
  a source-location use of one role evaluated in one declaration, or a
  reflection or a contract site, and Slip can tell which.
- **The cheapest form, as far as Slip can see**: widen the key that
  `frozen_string` interns by, from the content to (content, role,
  declaration evaluated in) for source-location strings, and give
  reflection's and contracts' strings objects of their own. That needs
  no new table: Kiln's existing record of each use (`record_source_loc`
  and the context it records) then lists every use of an object, and
  those uses are all of one role in one declaration. A default argument
  evaluated at a call counts as the caller's use, as Kiln records it
  today (`note_default_arg_use`, `builtins.rs:3850`). Slip knows no
  rule of the language that requires two such strings with equal
  contents to be one object (as for string literals, [lex.string]);
  Kiln should confirm that for code generation too.
- **Slip's need.** WP-115's digest writes such a string as "the plain
  (or pretty) name of entity X" only where every use of the object in
  the translation unit is of one role and in one entity; otherwise it
  writes the bytes. That is safe, but it costs false differences, held
  as six pair tests pending `# needs: kiln-srcloc-origin`, which the
  answer removes:
  - `equal/_/name-value-cost-overload-plain-rename` (`"f"` in `f(int)`
    and `f(long)`, a rename of `f(int)`);
  - `equal/_/name-value-plain-default-argument-holder` (`"h"` in
    `x::h()` and, through a default argument, `h()`; a rename of `h()`);
  - `equal/_/name-value-holder-own-lambda` (two lambdas of `f()` print
    one pretty name; a relocation);
  - `equal/_/name-value-cost-namespace-lambdas-shared-pretty` (the same
    at namespace scope);
  - `differ/_/name-value-cost-plain-added-earlier` (side B adds
    `a::get()`, so `b::get()`'s `"get"` goes to its bytes and
    `b::get()` is reported changed);
  - `equal/_/name-value-cost-shared-plain-rename-other` (a rule renames
    `f(int)`, whose `"f"` is shared with `f(long)`).

  It also closes two named holes, both C++26 only and not reachable at
  C++20: a reflection string with the bytes of a function's name, used
  only in that function, is written as that function's name, so a
  rename would make it a wrong `equal`; and a contract site's pointer,
  were it stored in a constant, would be written the same way (no
  compile-time path stores one today).
- **Workaround in Slip**: the bytes are written wherever the uses span
  more than one entity (false differences, never a wrong `equal`). A
  namespace migration (a relocation) is not affected; a rename of a
  function whose plain name another function shares is.
- **Where**: `slip:spec/landed/digest/WP-115.md` items 1 and "Named
  holes", "For Kiln"; `slip:docs/wp/WP-115.md` "Supervisor's handback".

## 2. Layouts and alignment (WP-107)

Found while writing and reviewing the digest's rule for typedefs that
carry an alignment (`slip:spec/digest-kinds.md` §4.2 rule 9). Items 2.1
to 2.5 compare Kiln with clang 19; the values are the source's.

### 2.1 `alignof` of a reference drops the referee's sugar

- **Kind**: defect. **Implementation.**
- **Source**: `typedef long L __attribute__((aligned(4)));` then
  `alignof(L&)`.
- **Kiln today**: 8. Clang 19: 4, the referee's alignment
  ([expr.alignof]/3). `decl_align` (`kiln_abi/src/layout/mod.rs:518`)
  and `type_info` (`:365`) read the canonical referee and drop its
  sugar.
- **Slip's need**: none waits on it. The digest already writes the
  referee's typedef, one fact more than Kiln reads
  (`differ/_/typedef-alignment-reference`).

### 2.2 `alignof` and the layout read different declarations of a typedef

- **Kind**: defect. **Implementation.**
- **Source**:
  ```cpp
  typedef int R __attribute__((aligned(8)));
  typedef int R;
  ```
- **Kiln today.** The layout reads every declaration (`max_align_attr`,
  `kiln_abi/src/layout/props.rs:78`). `alignof` (the Oven,
  `kiln_oven/src/leaf.rs:609`, and code generation's fold through it,
  `kiln_codegen/src/expr.rs:365`) asks `Ast::layout_attributed_typedef`
  (`kiln_ast/src/layout_sugar.rs:58`), which checks `HAS_ATTRS` only on
  the declaration the sugar node names. So `alignof(R)` is 4 after both
  lines, 8 with the two lines swapped, and 8 for a use between them.
  Clang 19 gives 8 in every order.
- **Slip's need**: the digest makes such a typedef, and every unit whose
  type meets it, indeterminate (`typedef/alignment-by-declaration`).
  The cause goes when 2.2 and 2.4 are fixed.

### 2.3 `.*` and `->*` drop the pointee's sugar

- **Kind**: defect. **Implementation.**
- **Source**: `typedef int I16 __attribute__((aligned(16)));` with
  `I16 S::*mp`, then `alignof(decltype(((S*)0)->*mp))`.
- **Kiln today**: 4. Clang 19: 16.
- **Slip's need**: none waits on it. The digest is `equal` against
  `int S::*mp`, which matches Kiln: the member pointer is not walked,
  and the expression's type is canonical. When Kiln keeps the sugar,
  the using unit differs with no change to Slip.

### 2.4 The layout reads later typedef redeclarations and caches by first use

- **Kind**: defect. **Implementation.**
- **Source**:
  ```cpp
  typedef int R __attribute__((aligned(8)));
  struct S { char c; R r; };
  typedef int R __attribute__((aligned(16)));
  ```
- **Kiln today**: `sizeof(S)` is 32 when `S` is first laid out after the
  second declaration, and 16 when before (and it stays 16: the layout
  is cached). The typedef arm of `type_info`
  (`kiln_abi/src/layout/mod.rs:443-449`) takes `max_align_attr` over
  every declaration present. Clang 19 gives 16 in both orders: it reads
  the declaration the field names, with the attributes that declaration
  inherits from earlier ones (`alignof(R)` is 8 before the second
  declaration and 16 after).
- **Slip's need**: as 2.2, the cause `typedef/alignment-by-declaration`.

### 2.5 A class's layout depends on where it is first laid out

- **Kind**: defect. **Implementation.** **Pressing.**
- **Source**:
  ```cpp
  struct __attribute__((aligned(8))) C { int x; };
  struct S { char c; C m; };
  struct __attribute__((aligned(16))) C;
  unsigned long g() { return sizeof(S); }
  ```
- **Kiln today.** A GNU `aligned` on a redeclaration after the
  definition is kept when the definition already has one
  (`check_new_attrs_after_def`, `kiln_sema/src/after_def.rs:35`), and
  the record layout reads every redeclaration (`max_align_attr` from
  `kiln_abi/src/layout/record.rs:152`). So `sizeof(S) != 16` and
  `alignof(C) != 8` when `S` is first laid out after the redeclaration,
  and 16 and 8 when a `static_assert(sizeof(S) > 0);` before it lays `S`
  out first. Clang 19 gives 16 and 8 in both. An enumeration
  redeclared the same way keeps 8 in Kiln, as in clang.
- **Slip's need**: a wrong `equal` against what Kiln emits (not against
  the language). The digest does not write layouts, so two programs
  that differ only in where the first layout happens are `equal` and
  determinate: it sees only the added `static_assert`, and `C`, `S` and
  `g()` are equal. Closing it on Slip's side would need a cause that
  reaches every unit that reads a layout holding the class.
- **Workaround in Slip**: none. Slip's closing package adds a test that
  pins Kiln's two answers, so that the pin move shows the fix.

### 2.6 `Ast::vtables` is never filled under `-fsyntax-only`

- **Kind**: question (an observation in the source). **Implementation.**
- **Source**: not recorded in `slip:spec/landed/digest/WP-107.md` (it
  holds for any class with a virtual function).
- **Kiln today**: `ensure_vtable` has no caller outside `kiln_abi`, so
  `Ast::vtables` stays empty without code generation, and the snippet
  check refuses `-fdump-vtable-layouts`. Is that intended?
- **Slip's need**: the digest's 30 vtable rows stay in
  `slip:crates/slip_digest/tests/tables/untested.txt`, because no
  construct fills the table.

### 2.7 `ms_struct` after the definition is accepted and has no effect

- **Kind**: question (an observation in the source). **Implementation.**
- **Source**: not recorded in `slip:spec/landed/digest/WP-107.md`.
- **Kiln today**: `ms_struct` on a redeclaration after the definition is
  in `attr_allowed_after_def`'s list (`kiln_sema/src/after_def.rs:153`,
  in the function at `:141`), and `Ast::is_ms_struct` reads every
  redeclaration, yet the layout does not change. Where does Kiln mean
  to drop it?
- **Slip's need**: none. The digest is `equal` either way.

## 3. Redeclaration chains (WP-113, "For Kiln" 6 to 8)

Items 1 to 5 of the same list were KB4 1.1, 1.2 and 1.4, and KB5 4.1
and 4.2. These three were seen through Slip's walk of the chains.

### 3.1 An explicit specialisation chained to a declaration in the template's opening

- **Kind**: question. **Design.**
- **Source** (`slip:crates/slip_digest/tests/rules/enclosing/specialisation-in-hidden-opening/a.cpp`):
  ```cpp
  namespace d { template <class T> int g() { return 0; } }
  namespace __attribute__((visibility("hidden"))) d { template <> int g<int>() { return 1; } }
  int u() { return d::g<int>(); }
  ```
- **Kiln today**: `Ast::redecls_of` of `d::g<int>` holds, besides the
  declaration written in the hidden opening, a declaration whose context
  is the template's plain opening, at the location of the template's
  first declaration (or its pattern's). Clang gives the specialisation
  the visibility of the opening it is written in (`GLOBAL HIDDEN
  _ZN1d1gIiEEiv`). Is that declaration meant to be in the chain? If it
  is, it should carry the opening the specialisation is written in.
- **Slip's need**: the declaration made Slip's unit indeterminate (known
  cost 4 of WP-113), which is now fixed on Slip's side.
- **Workaround in Slip**: Slip leaves the declaration out by its location
  (`slip:crates/slip_digest/src/enclosing.rs`, `Governs::Own`). If Kiln
  moves it, the filter must move too.

### 3.2 `redecls_of` of a function template can miss its first declaration

- **Kind**: defect. **Implementation.**
- **Source**: not recorded in `slip:spec/landed/digest/WP-113.md` (not
  reduced). The shape: libstdc++'s `std::swap`, declared and then
  defined in a second opening of `std`, through `<utility>` in a probe
  run with `slip parse`.
- **Kiln today**: `Ast::redecls_of` of the template gave only the later
  declaration, not the template itself.
- **Slip's need**: Slip reads a template's chain to decide what its
  enclosing namespaces give it (visibility).
- **Workaround in Slip**: Slip adds the template explicitly.

### 3.3 The implicit `std::align_val_t` is not chained with a written one

- **Kind**: defect. **Implementation.**
- **Source** (`slip:crates/slip_digest/tests/rules/enclosing/implicit-align-val-t-beside-written/b.cpp`):
  ```cpp
  int *mk() { return new int(1); }
  namespace std { enum class align_val_t : decltype(sizeof 0); }
  void *al(decltype(sizeof 0) n) { return ::operator new(n, std::align_val_t(16)); }
  ```
- **Kiln today**: the new-expression makes Kiln declare
  `std::align_val_t` itself; the later `enum class align_val_t` should
  redeclare it (clang's `getStdAlignValT` declaration is found by the
  later redeclaration). The chain of `std::align_val_t` holds the
  implicit declaration alone.
- **Slip's need**: none today; the pair test passes with the chain as it
  is. If Kiln chains them in an opening of another visibility, Slip's
  unit becomes indeterminate, not a wrong `equal`.

## 4. The rename of functions and variables (WP-071)

### 4.1 A dependent friend function template specialisation

- **Kind**: missing record. **Design.**
- **Source**:
  ```cpp
  namespace n { template <class T> void f(T) {}
                template <class T> class A { friend void f<>(T); }; }
  ```
- **Kiln today**: a `Friend` whose target is a function type
  (`FriendTarget::Type`, `kiln_ast/src/decl.rs:1584`), with no
  declaration. The name is kept only in Sema's `friend_specs` map
  (`kiln_sema/src/lib.rs:1423`, filled at `class/friend.rs:1096`,
  resolved per instantiation at `templ/subst/decls.rs:2104`). Neither
  the written-reference table nor a declaration-extent row gives the
  token. KR1 stage 3 (Kiln WP-1932, on main) asked for friend references
  "when they name an existing entity"; Slip has not checked whether
  that covers a friend kept as a function type.
- **Asked**: a written or dependent entry for the friend's name (with
  the template as its referent once resolved), or at least a
  declaration-extent row, so that the rename can claim the token or
  flag the seed `unreachable`.
- **Slip's need**: today a silent miss. The rename neither claims the
  name nor flags the seed, so the edited program does not compile ("no
  candidate function template"); verification reports it, never a
  wrong `equal`. Pending: `s9_5-unreachable-friend-specialisation-in-template`,
  `-not-instantiated`, `-with-arguments` in
  `slip:crates/slip_refactor/tests/rename/`. 5.3 is the same shape.

### 4.2 `n::v<T>` with a non-dependent template name has no referent

- **Kind**: missing record. **Implementation.**
- **Source**:
  ```cpp
  namespace n { template <class T> constexpr bool v = true; }
  template <class T> bool k() { return n::v<T>; }
  ```
- **Kiln today**: the entry for `v` is a dependent entry with no
  referent. Only the template-id is dependent; the template name `v` is
  not. (KB1 1.6 asked for a dependent entry's found set; this is the
  referent of a name that lookup has already resolved.)
- **Asked**: the variable template as the entry's referent. Slip
  expected KR1 stage 4 (Kiln WP-1933, on main) to give it and re-judges
  at its pin move; if stage 4 gives entries per specialisation only,
  the pattern's entry still has none, and this is the ask.
- **Slip's need**: a known over-refusal. The name is claimed
  `unresolved`, so every use of a variable template with dependent
  arguments in a template refuses the run. Pending:
  `known-dependent-variable-template-id` (wants
  `replace spell(<n::v>, "w")`).

## 5. Dependent names and members of class templates (WP-073)

Stage 4 (K-e) and WP-1980 are in "Not in this batch". Each item here is
met by `unresolved` or by a flag; none is a wrong edit.

### 5.1 The join from a dependent entry to its expression

- **Kind**: missing record. **Implementation.**
  Addition to KR1 amendment 1 §2 (sent 2026-11-22).
- **Source**:
  ```cpp
  namespace n { void f(int); template <class T> void t(T x) { f(x); } }
  ```
- **Kiln today**: the link from a dependent entry to its
  unresolved-lookup expression is the private map `pending:
  HashMap<ExprId, WrittenRefId>` (`kiln_ast/src/written_refs.rs:179`).
  Its only setter is `defer` (`:229`), `resolve` empties it (`:270`), and
  there is no getter. Kiln's `spec/ast.md` §18.4 promises no location
  equality between such an entry and its expression. What is new
  against KR1 amendment 1 §2 (which asked for the join of resolved
  names): the dependent entries, and a join that survives `resolve`.
- **Asked**: a public getter from a dependent `WrittenRefId` to its
  `ExprId`, kept after `resolve`, or the found set recorded on the entry.
- **Slip's need**: part A of WP-073 (renaming a dependent call through
  its found set) is not built. Pending: the five `s9_4-dependent-call-*`
  directories.
- **Workaround in Slip**: the name is claimed `unresolved`, and the old
  name is left as `dependent` residue in the run's report.

### 5.2 A member's explicit instantiation: no target entry, the pattern's location

- **Kind**: missing record. **Implementation.**
  Addition to KB1 1.1 (no extent row) and to KR1 stage 3's
  specialisation targets (`SpecTarget`, Kiln WP-1932).
- **Source**:
  ```cpp
  namespace n { template <class T> struct S { void f() {} };
                template void S<long>::f(); }
  ```
- **Kiln today**: the role `SpecTarget` is declared
  (`kiln_ast/src/written_refs.rs:70-71`) and nothing outside that file
  records it. The explicit instantiation also has no redeclaration of
  its own, no extent row, and the pattern's location. The last is new.
- **Asked**: the stage 3 `SpecTarget` entry for the instantiation's
  name.
- **Slip's need**: pending `s9_2-member-explicit-instantiation-spec-target`.
- **Workaround in Slip**: the follower is flagged `unreachable`
  (`s9_5-unreachable-member-explicit-instantiation`).

### 5.3 A friend of a member of a dependent specialisation

- **Kind**: missing record. **Design.**
- **Source**:
  ```cpp
  namespace n { template <class T> struct S { void f(); }; }
  class X { template <class T> friend void n::S<T>::f(); };
  ```
- **Kiln today**: a `Friend` whose target is a function type, with no
  declaration: 4.1's shape. Its extent row gives the name's span; Kiln
  does not record which class the friend names.
- **Asked**: a declaration, or a written entry for the name whose
  referent is the member of the template.
- **Slip's need**: an over-refusal. Every seed in a class template
  pattern is flagged `unreachable` whenever such a friend may spell the
  old name, whatever class it names: a friend of the partial
  specialisation's own `n::S<T*>::f`, a different entity, refuses a
  rename of `n::S::f`. Tests `s9_5-unreachable-friend-of-dependent-member`
  and `-declared-only`.

### 5.4 A member template specialised as a template has no link

- **Kind**: missing record. **Implementation.**
- **Source**:
  ```cpp
  namespace n { template <class T> struct S { template <class U> void g() {} };
                template <> template <class U> void S<int>::g() {} }
  ```
- **Kiln today**: `S<int>::g` is declared in the instantiated `S<int>`
  with no `instantiated_from`, no template link on any declaration, and
  no member-specialisation record.
- **Asked**: the link to `S::g` (as clang's
  `getInstantiatedFromMemberTemplate`, with the member-specialisation
  flag).
- **Slip's need**: Slip cannot tell which seed such a member
  specialises, so it flags every seed in a pattern `unsupported`
  (`s9_5-unsupported-member-template-specialised-as-template`): any
  unlinked member of any class template with the old name refuses the
  run.

### 5.5 An explicit instantiation after an explicit specialisation

- **Kind**: missing record. **Implementation.**
- **Source**:
  ```cpp
  namespace n { template <class T> struct S { void f() {} };
                template <> void S<int>::f() {}
                template void S<int>::f(); }
  ```
- **Kiln today**: Kiln warns `sema_explicit_inst_after_spec_ignored` and
  returns before `update_member_spec`
  (`kiln_sema/src/templ/funcs.rs:1692-1701`, `:1733`). There is no
  declaration, no extent row and no member-specialisation kind.
- **Asked**: a record of the explicit instantiation's name (the stage 3
  `SpecTarget` entry), even when the instantiation has no effect.
- **Slip's need**: pending
  `s9_5-unreachable-explicit-instantiation-after-specialisation`. Today
  the follower's `unsupported` refuses the run. Once Slip's WP-119 lifts
  that flag, a rename would leave this name, and the edited program is
  diagnosed (loud, not silent).

### 5.6 Members of an explicitly instantiated class are `ImplicitInstantiation`

- **Kind**: question. **Implementation.**
- **Source**: `template struct S<short>;` (the class of 5.2's source).
- **Kiln today**: the members get the kind `ImplicitInstantiation`;
  clang says explicit instantiation definition. Intended?
- **Slip's need**: none. The members are followers either way and
  their names are the pattern's.
  `s9_2-class-template-explicit-instantiation-of-the-class` pins the
  current behaviour, so a change shows.

## 6. The subject: calls and expression extents (WP-086)

Items 1 to 3 of WP-086's list are KB5 3.1 to 3.3. These were found by
running the subject's calls against clang 19's over Slip's tests and
Kiln's conformance tests.

### 6.1 A constant array bound keeps no expression

- **Kind**: missing record. **Design.** Addition to KB5 1.1: there the
  bound is instantiation-dependent and Kiln chooses the wrong overload;
  here the bound is an ordinary constant, so nothing is wrong in Kiln's
  semantics, but the written expression is gone.
- **Source**:
  ```cpp
  constexpr int f(int x) { return x; }
  int arr[f(2)];
  ```
  The same for `sizeof(int[f(13)])` and a parameter `int (&x)[f(19)]`.
- **Kiln today**: the array type has `size_expr: None` once the bound
  folds to a constant (`kiln_sema/src/decl.rs:1828`, `:2304`, `:2321`,
  `:2325`; `kiln_sema/src/templ/subst/types.rs:451`), so the call
  `f(2)` is in no node that a walk can reach from the declaration.
- **Asked**: keep the written bound's expression with the constant array
  type, as the dependent and variable-length forms do. Where to keep it
  without changing the canonical type is Kiln's decision.
- **Slip's need**: a named hole of the subject's version 0
  (`slip:spec/api.md` §6.7 rule 8): a script's `calls_to` misses a call
  written in an array bound. Pending test
  `s6_7_rule3_pending_in_an_array_bound`.

### 6.2 A partial specialisation's non-type argument keeps no expression

- **Kind**: defect (against Kiln's own record). **Implementation.**
- **Source**:
  ```cpp
  constexpr int f(int x) { return x; }
  template <class T, int N> struct P {};
  template <class T> struct P<T, f(1)> {};
  ```
- **Kiln today**: `PartialSpecInfo::args`
  (`kiln_ast/src/templates.rs:273`) is documented as "the arguments as
  written", but holds the argument's value, not the expression `f(1)`.
  A type argument such as `P<T, A<f(1)>>` keeps it.
- **Asked**: keep the written expression of a non-type argument of a
  partial specialisation.
- **Slip's need**: the same hole as 6.1. Pending test
  `s6_7_rule3_pending_in_a_partial_specialization_value`.

### 6.3 GNU `__sync_*` builtins and calls of a declared function

- **Kind**: question. **Implementation.**
- **Source**: not recorded in `slip:spec/landed/api/WP-086.md` (30 such
  calls in Kiln's conformance tests).
- **The source says**: Kiln makes no call of a declared function for the
  `__sync_*` builtins, so the subject does not count them as calls, and
  clang 19 does.
- **Read at the pin, for this batch**: `builtin_sync_call`
  (`kiln_sema/src/builtins/atomic.rs:319`) retargets the call to the
  `_N` builtin's declaration through a new `DeclRefExpr`, as clang's
  `BuiltinAtomicOverloaded` does. So the difference may be on Slip's
  side (for example, a callee with no written reference). Slip checks
  this before Kiln spends time on it; the question for Kiln is only
  whether the retargeted call is meant to be a call of that declaration.
- **Slip's need**: none pending. The 30 calls are counted as clang calls
  the subject lacks in the reverse differential.

## 7. Where a finding is placed (WP-054, in review)

### 7.1 `CXXDefaultArgExpr` has no location

- **Kind**: missing record. **Implementation.**
- **Source**: not recorded in `slip:docs/wp/WP-054.md` (any call that
  uses a default argument).
- **Kiln today**: every call of `Builder::default_arg` passes
  `Loc::INVALID`: `kiln_sema/src/expr.rs:6428`, `new_delete.rs:1288`,
  `enable_if.rs:313`, `class/special/define.rs:1523`. Clang keeps the
  use's location (`CXXDefaultArgExpr::getUsedLocation`).
- **Asked**: the location of the use. This is not KE1's extent, which
  stays `None` for a default argument (nothing is written at the use).
- **Slip's need**: Slip places a digest difference at the node where the
  two sides' bytes first differ, so that a user sees where a change
  is. A difference in a default argument used at a call is placed
  nowhere (empty file, line 0): such cases are part of the 43 pairs of
  class `Unplaced` in `slip:spec/landed/verify/WP-054.md`. Not blocking.

## 8. Virtual families and using-declarations (WP-120, in review)

### 8.1 Hidden using-shadows depend on declaration order

- **Kind**: defect. **Design.**
- **Source**:
  ```cpp
  struct B { virtual void f(); void f(int); };
  struct D : B { using B::f; void f() override; };
  ```
  against the same `D` with `void f() override;` before `using B::f;`.
- **Kiln today**: in the first order, Kiln makes a shadow of `B::f()`
  and marks it hidden by `D::f()` (`hidden_shadow`; by this batch's
  reading, `kiln_sema/src/using.rs:1228-1231`). In the second order it
  makes no shadow for `B::f()` at all (`introduce_shadow`,
  `using.rs:943-946`). The two classes are the same
  ([namespace.udecl]/14).
- **Asked**: one form for both orders. Either would do for Slip; making
  no shadow for a hidden declaration also closes 8.2's case.
- **Slip's need**: the "unhide" idiom (a using-declaration beside an
  override of the same name). Renaming the virtual family seeded at
  `D::f` refuses (`mixed_follower` on the hidden shadow) when the
  using-declaration comes first, and renames and verifies when the
  override comes first. The digest writes a hidden shadow as a unit, so
  the right C++ edit (the using-declaration kept) shows a removed unit.
- **Workaround in Slip**: none on Kiln's side. Slip's alternative is a
  digest change (leaving out shadows marked hidden), proposed for its
  WP-119.

### 8.2 An instantiated using-declaration's shadows are not linked to the pattern's

- **Kind**: missing record (an observation in the source, which says it
  is not a defect). **Implementation.**
- **Source**:
  ```cpp
  template <class T> struct F : Base { using Base::f; };
  ```
  with `F<int>` instantiated.
- **Kiln today**: `F<int>`'s using-declaration is `instantiated_from` the
  pattern's (no `templ_link`). Its shadow is linked to nothing, so its
  key holds its own plain name, and a rename needs a rule of its own
  for it.
- **Asked**: a link from each instantiated shadow to the pattern's
  shadow, so that shadows map through the pattern as members do.
- **Slip's need**: low.
- **Workaround in Slip**: Slip finds the shadows through the
  using-declaration's link (`instantiated_using`).

## 9. A unit's own spellings (WP-041, provisional)

WP-041 is in its fix round. Its landed file and handback are on its
branch; items 9.4 to 9.6 and the "Asked" of 9.3 are in its working tree
and not yet committed. The reproducers are pair tests under
`slip:crates/slip_digest/tests/`. Already sent, not repeated: the fold
operator set is KB4 1.3 (Kiln WP-2625; new since then is only the unary
reproducer `indeterminate/_/spelling-fold-operator-unary`, and the cost:
every `+` fold is indeterminate in a unit that includes libc++'s
`<string>` until it is fixed); the three questions are KB5 1.2 to 1.4;
pairs q001 and q006 are KB5 1.1.

### 9.1 Addition to KB5 1.1: a bound naming a member through a dependent qualifier

- **Kind**: defect. **Implementation.**
- **Source** (`differ/_/spelling-prefix-array-bound`, the review's y44):
  ```cpp
  template<int N> struct Bq { static constexpr int value = 1; static int k(); enum { E1 = 1 }; using type = int; };
  int f(int); int h(int);
  #ifdef B
  template<class T> void g(T t, char (*)[Bq<sizeof(sizeof(h(T())))>::value + 1]);
  #else
  template<class T> void g(T t, char (*)[Bq<sizeof(sizeof(f(T())))>::value + 1]);
  #endif
  ```
- **Kiln today**: as KB5 1.1 (`kiln_sema/src/decl.rs:1828`,
  `constant_array(ty, n, None)`). What is new: the bound is not itself a
  `sizeof` of a dependent call but a member named through an
  instantiation-dependent qualifier, and the found set in that
  qualifier is lost the same way ([temp.over.link]/5).
- **Slip's need**: a third pending test beside KB5 1.1's two; the two
  sides give one digest.

### 9.2 A dynamic exception specification in C++17 emits no filter

- **Kind**: defect (against clang 19). **Implementation.**
- **Source** (`differ/_/exc-dynamic-is-kept-in-cxx17`, `-std=c++17
  -Wno-dynamic-exception-spec`): `void f() throw(int);`
- **Kiln today**: Kiln keeps `throw(int)` in C++17 and reads it only in
  the override check (`check_override_exc_spec`,
  `kiln_sema/src/class/virtual/mod.rs:931`). Its code generation emits
  the run-time filter before C++17 only
  (`kiln_driver/src/codegen_cmd.rs:593`, `dynamic_exception_specs`).
  Clang 19 emits it in C++17 too.
- **Slip's need**: none. The digest writes the list, so it is right
  whichever Kiln settles on.

### 9.3 A qualified pseudo-destructor keeps no written qualifier

- **Kind**: missing record. **Implementation.**
- **Source** (`indeterminate/_/spelling-pseudo-destructor-qualifier`):
  ```cpp
  template<int N> struct Bq { using I = int; };
  struct X {};
  int f(int); int h(X);
  #ifdef B
  template<class T> void g(T t, int *p) { p->Bq<sizeof(sizeof(h(t)))>::I::~I(); }
  #else
  template<class T> void g(T t, int *p) { p->Bq<sizeof(sizeof(f(t)))>::I::~I(); }
  #endif
  ```
- **Kiln today**: the node keeps the aux bit `qualified`
  (`pseudo_destructor_qualified`, `kiln_ast/src/expr.rs:3260`) and the
  destroyed type. The qualifier is kept (`expr_nns`,
  `pseudo_dtor_first`) only when it is dependent itself.
- **Asked**: keep the written qualifier of every qualified
  pseudo-destructor.
- **Slip's need**: such a pseudo-destructor in templated code is
  indeterminate (`spelling/pseudo-destructor-qualifier`), even when the
  qualifier holds nothing instantiation-dependent (`p->ns::S::I::~I()`).
  The digest row `CXXPseudoDestructorExpr.aux-qualified` stays in
  `tests/tables/untested.txt` until the pin move that brings the fix.

### 9.4 A pseudo-destructor drops its qualifier's instantiation dependence

- **Kind**: defect. **Implementation.**
- **Source**: 9.3's.
- **Kiln today**: `AstBuilder::pseudo_destructor_q`
  (`kiln_ast/src/builder.rs:1337`) computes the node's dependence as
  `deps(base) | type_deps(destroyed)`, so an instantiation-dependent
  qualifier makes neither the node nor its enclosing expression
  dependent.
- **Asked**: the qualifier's dependence in the node's.
- **Slip's need**: as 9.3. Slip no longer asks Kiln's dependence flag
  for this and treats all templated code alike.

### 9.5 `using enum` keeps no written qualifier

- **Kind**: missing record. **Design.**
- **Source** (`indeterminate/_/spelling-templated-out-of-line-member`,
  `// std: c++20`):
  ```cpp
  template<int N> struct Q { using I = int; enum E { e1 }; };
  int *p;
  int f(int);
  #ifdef B
  long f(long);
  #endif
  template<class T> struct S { void h(); };
  template<class T> void S<T>::h() { using enum Q<sizeof(sizeof(f(T())))>::E; }
  ```
- **Kiln today**: `Sema::act_on_using_enum`
  (`kiln_sema/src/using.rs:1317`) keeps the enumeration only.
- **Asked**: keep the written qualifier (or the written type) of a
  `using enum`.
- **Slip's need**: every `using enum` in templated code is indeterminate
  (`spelling/using-enum`), also `using enum ns::S::E;`.

### 9.6 A destructor named by member access keeps no written name

- **Kind**: missing record. **Design.**
- **Source** (`indeterminate/_/spelling-destructor-name-specialisation`):
  ```cpp
  template<int N> struct X {};
  int f(int);
  #ifdef B
  long f(long);
  #endif
  template<class T> void g(T t, X<8> *q) { q->~X<sizeof(sizeof(f(t)))>(); }
  ```
- **Kiln today**: `q->~X<…>()` is a `MemberExpr` to `X<8>::~X`; the
  type-name and its arguments are gone.
- **Asked**: keep the written type of a destructor name in a member
  access.
- **Slip's need**: every destructor named by member access in templated
  code is indeterminate (`spelling/destructor-name`), `q->~X()` as much
  as `q->~X<…>()`. Before fix round 4 this was a wrong `equal`.

### What looked unfinished in WP-041

- The fix round 4 text, the "Asked" of 9.3, and items 9.5 and 9.6 are
  uncommitted changes in the package's worktree. The branch's last
  commit (`f4d3ad8e`) has "For Kiln" items 1 to 5 only.
- The fix round 4 has not been re-checked by the reviewer yet. Fix round
  2 was handed back with no gate run, by instruction.
- Whether the row `CXXPseudoDestructorExpr.aux-qualified` stays in
  `untested.txt` or becomes `unsupported` is left to the reviewer.

## Not in this batch

- **Already sent.** WP-086's items 1 to 3 (the begin of `Q::f(*this,
  1)`, the braces of `f({x})`, a member variable template's arguments)
  are KB5 3.1 to 3.3. WP-113's items 1 to 5 are KB4 1.1, 1.2 and 1.4,
  and KB5 4.1 and 4.2. WP-071's K1, K4, stage 3 roles and labels are
  KB1 1.1, KB1 1.6, KR1 stage 3 and KB5 2.4. WP-052's F7 (an absolute
  `__has_include` is not in the lookup log) is KB4 4.2: when it is
  fixed, Slip's `slip:crates/slip_cli/tests/verify_job.rs`
  `f7_absolute_has_include` (ignored, `needs: kiln`) runs with no new
  code, beside KB4's `absolute_name_not_recorded`.
- **Dropped earlier.** WP-086's anonymous-union members with no table
  entry: KB5 found that the written names have entries, and the nodes
  without one are implicit accesses of the unnamed member. WP-086's
  count on LLVM units (28 and 17) is the same observation.
- **Already scheduled by Kiln.** WP-073's stage 4 (K-e, Kiln WP-1933):
  per-specialisation entries for `T::f`, `x.f` and `x->f`; four
  `s9_4-dependent-member-*` tests are pending on it. WP-1980 (at the
  pin, a name in a `static_assert` in a class is enclosed by the class,
  `spec/ast.md:1744`); WP-073's source for it is `namespace n { void
  f(int); template <class T> struct S { static_assert(sizeof(f(T())) >
  0); }; }`. Slip re-judges both at its pin move.
- **For Slip, not Kiln.** The "For the digest" parts of WP-073, WP-107
  and WP-120, and WP-115's C1 (a default argument's immediate value
  written only by a `default_immediates` unit).
