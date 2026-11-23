# KB5 — findings for Kiln, batch 5 (2026-11-23)

From Slip's packages WP-041, WP-052, WP-071, WP-085, WP-086, WP-096,
WP-110 and WP-113 and their independent reviews, at Kiln pin
`913a490bb`. As in [KB1](KB1-findings-batch-1.md) to
[KB4](KB4-findings-batch-4.md): each item has a workaround or a pinned
test on Slip's side, and Slip asks only for an answer per item (a work
package number, "as designed", or "not now"). **One item is pressing:
1.1.** Kiln builds an instantiation-dependent constant array bound
without its expression, so it chooses a different overload than clang
does. Slip's digest cannot see the expression either, so a change to
what the bound names gives a wrong `equal`, and Slip has no workaround.
Items 2.2, 2.3, 2.4 and 5.1 are requests (**R**): Slip's operator sees
them before Slip presses for any. WP-053 has nothing for Kiln.

Every item was checked against Kiln's main at `352806a74`, by reading
only. Three candidates are fixed there, so they are not in this batch.
Slip takes the fixes with its next pin move:

- WP-110: an inclusion that a guard or `#pragma once` skips is now
  recorded with its includer (WP-2111: the preprocessing record's
  `SkippedGuard` and `SkippedOnce` outcomes with `parent()`, Kiln
  `spec/pp.md`).
- WP-085: `sizeof...(Ts)` now records the pack (WP-1932,
  `parse_sizeof_pack`). The pack of a pack-index-specifier that begins a
  nested-name-specifier (`Ts...[0]::m`) now has an entry that links the
  next component (WP-1934, `parse_pack_index_scope`). At the pin,
  neither has an entry (reproduced).
- WP-052: `-working-directory` is now accepted (WP-2121). At the pin
  the driver rejects it (reproduced).

**Kind**: **I** = implementation against behaviour that is already
specified or obvious; **Q** = a question, where the answer may be "as
designed"; **D** = needs a design decision without prior art in Kiln
(hand to whoever does Kiln's design); **R** = a request for something
Kiln does not have (a feature, not a defect: Slip's operator sees these
before Slip presses for any).

Paths are Kiln's unless they start with `slip:`. A `slip:` path of
WP-041, WP-071, WP-085, WP-086, WP-096 or WP-113 is on Slip's main once
that package merges. Each package is in review or in its fix round.
**Reproduced** means that the program was run at the pin through one of
three tools: Kiln's driver (`kiln -fsyntax-only`, `-c` with
`readelf -sW`, or the `-fkiln-dump-written-refs` and
`-fkiln-dump-decl-extents` dumps), or a probe over `Ast::begin_loc` and
`end_loc` in Slip's production parse. Clang 19 (`clang++-19 -std=c++20`)
is the reference. **By reading** means that the item was not run.

## 1. Semantic analysis

| # | Kind | Finding | Where |
|---|---|---|---|
| 1.1 | I | **An instantiation-dependent constant array bound is built without its expression** (reproduced). `constant_array(ty, n, None)` keeps the value only, so `char (*)[sizeof(sizeof(f(T())))]` in a template is the type `char (*)[8]`. Clang keeps the expression (`ConstantArrayType` with its size expression when that is instantiation-dependent, [temp.over.link]/5), and substitution checks it. Three symptoms follow. **(a) Kiln picks the wrong overload:** `struct X {}; constexpr int f(int) { return 0; } template <class T> constexpr int g(T, char (*)[sizeof(sizeof(f(T())))]) { return 1; } constexpr int g(...) { return 2; } static_assert(g(X(), nullptr) == 2);`. Clang accepts this: substituting `X` fails, so `g(...)` is chosen. Kiln reports `static assertion failed`, `'1 == 2'`. An adjusted parameter's bound behaves the same way: `void (*)(int[sizeof(f(T()))])`, with no node anywhere that holds it. **(b) An ill-formed instantiation is accepted:** `template <class T> int g(T t) { char a[sizeof(sizeof(f(t)))]; return sizeof a; } int u = g(X());` gets no error from Kiln. Clang reports `no matching function for call to 'f'` in `g<X>`. **(c) The found set of `f` is lost**, so Slip's digest of `g` reads `[8]`. Tests `slip:crates/slip_digest/tests/differ/_/spelling-instantiation-array-bound` and `-local-array` are pending on this item. Unchanged on main at `352806a74`. | `kiln_sema/src/decl.rs:1828`; `kiln_ast/src/types/canon.rs:263-275` (the `size_expr` slot exists); `slip:spec/landed/digest-kinds/WP-041.md` "For Kiln" 2 |
| 1.2 | Q | **A conversion function's name is interned by canonical type** (by reading), so `operator decltype(f(U()))` keeps the operand of the first spelling. Clang also names a conversion by its canonical type, but keeps the written type beside the name (the `TypeSourceInfo` of its `DeclarationNameInfo`). Is a per-spelling name, or the written type beside it, possible? Slip finds the written type through the declaration's type sugar. | `kiln_ast/src/names.rs:159-160`, `:375-377`; `slip:spec/landed/digest-kinds/WP-041.md` "For Kiln" 3 |
| 1.3 | Q | **`TypeContext::proto` returns the canonical prototype from C++17 on** (by reading), and that prototype's dependent `noexcept` is the profile of the first spelling. Slip walks the type's sugar to the written node (`Walk::own_proto`). Is a direct accessor for the written prototype intended? | `kiln_ast/src/types/context.rs:745-752`; `slip:spec/landed/digest-kinds/WP-041.md` "For Kiln" 3 |
| 1.4 | Q | **A dependent `_BitInt(N)` keeps the width of its first spelling** (reproduced). `constexpr int f(int) { return 0; } template <class T> constexpr int g0(T, _BitInt(sizeof(f(T())) * 8) b) { return sizeof(b); } constexpr long f(long) { return 0; } template <class T> constexpr int g(T, _BitInt(sizeof(f(T())) * 8) b) { return sizeof(b); } static_assert(g(1L, 0) == 8);` fails: `g` gets `g0`'s width, which was written before `f(long)`, so the result is `4`. **Clang 19 does the same** (`'4 == 8'`), so the answer may be "as designed". Could each spelling keep its own width, as `decltype` does through its written node? Slip makes such a unit indeterminate (`spelling/bit-int`). | `kiln_ast/src/types/canon.rs:355-358` (keyed by the width's `ProfileId`); `slip:crates/slip_digest/tests/indeterminate/_/spelling-bit-int-first-spelling` |

## 2. The written-reference table (Kiln `spec/ast.md` §18)

| # | Kind | Finding | Where |
|---|---|---|---|
| 2.1 | I | **The pack in a pack-indexing expression has no entity** (reproduced). `int g(int, int); template <class... Ts> int z(Ts... ts) { return g(ts...) + ts...[0]; }`: the entry of `ts` in `g(ts...)` has the parameter `ts` as its entity. The entry of `ts` in `ts...[0]` has none and is `DEPENDENT`. The name `ts` is not dependent: it is an id-expression that names the function parameter pack, and only the indexing is dependent (clang's `PackIndexingExpr` holds a `DeclRefExpr` to `ts`). §18.5 lists what counts as a dependent name, and this is not on that list. Expected: the pack as the entity, as for `ts...`. No change on main at `352806a74` (by reading `parse_pack_index_suffix`). | `kiln_parse/src/expr.rs:2684`, `:2829-2833`; `kiln_sema/src/templ/actions.rs:1800` (`act_pack_indexing`); site not located more exactly |
| 2.2 | R | **Where a `decltype(...)::` qualifier starts.** `struct S { static int m; }; S s; int y = decltype(s)::m;`: the entry of `m` has no `qualifier` link and nothing records where the written name begins. This is as specified (`spec/ast.md` §18.4, stage 1: "a `decltype(...)::` component has no entry … and the component after it has no `qualifier` link"; unchanged on main). Slip's subject therefore gives `m` no extent, which is a refusal and not a wrong range. Wanted: the first token of the specifier recorded on the entry after it, or the specifier linked as a component. Slip treats a splice qualifier the same way. | `spec/ast.md` §18.4; `slip:spec/landed/api/WP-085.md` "For Kiln" 10 |
| 2.3 | R | **Operators used as operators, and user-defined-literal calls, have no entry** (reproduced). `S t = s + s;` records `s` twice and nothing for `+`. `int a = 12_k;` records nothing for `_k`. This is as specified: §18.4 "Not in the table" excludes "operator uses such as `a == b`", and so did Slip's own KR1 §2. WP-096's spelling check now needs them: a name that Kiln does not record cannot veto, so the check holds by default. Wanted: one entry per operator token that selects a function (role `Expr`, entity the selected `operator@`), and one per literal suffix that calls a literal operator. Kiln's `CXXOperatorCallExpr` already has the operator token as its location. KB3 2.2 (the call of a user-defined literal begins inside its token) is separate. | `spec/ast.md` §18.4 "Not in the table"; `slip:spec/landed/verify/WP-096.md` "For Kiln" 4 |
| 2.4 | R | **Labels are in neither table** (reproduced: `void h() { f: goto f; }` has no entry for `goto f` and no row for `f:`; the row is KB4 2.7). Labels are excluded from §18 by design, and on main §18.8 counts them as documented non-names. Slip does not need them today: its rename covers functions and variables and does not claim a label. A rename of labels would need the `goto` reference and `&&l`. Listed so that the set is known. | `spec/ast.md` §18.4, §18.8 (main); `slip:spec/landed/edits/WP-071.md` "For Kiln" |

## 3. Expression extents (`slip:spec/landed/api/WP-086.md` "For Kiln"; KE1)

`Ast::begin_loc` and `end_loc` are documented as clang's `getBeginLoc`
and `getEndLoc`. Each item below is a node where they differ from
clang's range. Slip refuses the extent in each case (null, never wrong),
so these are concrete cases for KE1's differential (Kiln WP-2327).

| # | Kind | Finding | Where |
|---|---|---|---|
| 3.1 | I | **A qualified name's begin is the name, when a member calls a static overload through its class** (reproduced). `struct Q { static int f(const Q&, int); int f(int) const; int g() const { return Q::f(*this, 1); } };`: the `DeclRefExpr` of `Q::f` begins at `f`, so the `CallExpr` is `f(*this, 1)`. Clang gives `<Q, f>` and the call `Q::f(*this, 1)`. The qualifier is recorded on the node, but the reference rebuilt from the overload set gets no `expr_begin`, and `begin_loc` falls back to the node's location. Slip found 2 of these in `clang/lib/Sema/SemaExpr.cpp`. | `kiln_ast/src/ast.rs:289` (fallback); `kiln_sema/src/expr.rs:2110-2120` (`set_qualified_begin`, not reached here); site not located more exactly |
| 3.2 | I | **No node covers the braces of a single-element list argument** (reproduced). `struct O { bool a = false; long b = 0; }; int f(const O&); int g(O); int use(O x) { return f({x}) + g({x}); }`. Kiln: `f`'s argument is the conversion of `x` and `g`'s is a `CXXConstructExpr` at `x`, both spanning `x` only. The `InitListExpr`s Kiln builds for `{x}` are in no tree. Clang gives `f` an `InitListExpr <{, }>` and `g` a `CXXConstructExpr ... list <{, }>`. Expected: the argument's extent includes the braces, either through the list node or through the construction's braces. | `kiln_sema/src/init.rs:306-325` ([dcl.init.list]/3.2, clang 19's DR1467 path), `:2235-2245` (the reference path, /3.10) |
| 3.3 | I | **A member variable template's template arguments are not recorded outside a template** (reproduced). `struct V { template <int N> static constexpr int twice = 2 * N; }; int k(int); int use(V d, V* p) { return k(d.twice<7>) + k(p->twice<2>); }`: the `MemberExpr`s end at `twice`, so `<7>` and `<2>` belong to no node. Clang's `MemberExpr` runs from `d` to `>`. `member_var_template_id` calls `set_member_template_args` only `if self.in_template()`. Expected: the arguments recorded (`member_extra`) wherever they are written, as for a member function template. | `kiln_sema/src/class/member_expr.rs:725-730`; `kiln_ast/src/expr.rs:1834`, `:2042` |

## 4. Code generation: symbol visibility (from WP-113's review)

Slip computes the visibility it compares by itself, so Kiln's symbols do
not change Slip's answers. These items are Kiln's objects differing from
clang's. Both are unchanged on main at `352806a74` (`visibility.rs` is
untouched there).

| # | Kind | Finding | Where |
|---|---|---|---|
| 4.1 | I | **A qualified definition at file scope takes its namespace's visibility from the wrong opening** (reproduced, `-c` and `readelf -sW`). `ns_lv` walks from `parent_ctx(d)`, which for such a definition is `d.ctx()`, the namespace's first opening. Clang's semantic context is the opening that the qualifier's lookup finds, which is the latest one before the definition. Results (Kiln / clang): `namespace hidden n { int f(); } namespace n {} int n::f() { … }`: `HIDDEN` / `DEFAULT`. `namespace n { int f(); } namespace hidden n {} int n::f() { … }`: `DEFAULT` / `HIDDEN`. `namespace n {} namespace hidden n { int f(); } int n::f() { … }`: `DEFAULT` / `HIDDEN`. `namespace n { template <class T> int g() { return 0; } } namespace hidden n {} template <> int n::g<int>() { … }`: `DEFAULT` / `HIDDEN`. ("hidden" stands for `__attribute__((visibility("hidden")))`.) **How this differs from KB4 1.4:** there the specialisation is written inside the hidden opening, so its lexical context is that namespace, and either remedy KB4 1.4 proposes fixes it. Here the lexical context is the translation unit, so starting at the lexical context does not help: the walk must start at the opening the qualifier named. Kiln's `spec/codegen.md` §8 "Namespace scope" says "the nearest enclosing namespace's attribute (any of its definitions …)". The code reads one opening (`vis_of(x, …, false)`), and clang uses neither rule. | `kiln_codegen/src/visibility.rs:239-262` (`ns_lv`), `:454-462` (`parent_ctx`), `:472-488`; `spec/codegen.md` §8 |
| 4.2 | I | **The visibility of an undefined reference is taken from the first declaration**; clang takes it from the declaration the use names, the most recent one visible there (reproduced). `namespace h { int f(); } namespace hidden h { int f(); } int u() { return h::f(); }`: Kiln `DEFAULT UND _ZN1h1fEv`, clang `HIDDEN UND _ZN1h1fEv`. A variable behaves the same way (`extern int v;` redeclared in a hidden opening: clang `HIDDEN UND _ZN1h1vE`, Kiln `DEFAULT`). This matters because an ELF link gives the final symbol the most constraining visibility among its references, so a hidden reference hides the symbol. KB4 1.4 and 4.1 concern definitions; this item concerns declarations (`decl_visibility(first, false)`). | `kiln_codegen/src/module.rs:3666`, `:3908`; `visibility.rs:170-176` |

## 5. The driver

| # | Kind | Finding | Where |
|---|---|---|---|
| 5.1 | R | Addition to KB4 4.4: **`-iprefix` and `-iwithprefix` are not accepted** (reproduced: `kiln: error: unknown argument: '-iprefix'`). Clang 19 accepts `-iprefix sub/ -iwithprefix inc` and finds `sub/inc/v.h`. A unit built with them is not parsed, so Slip's post-image is `not_parsed`. `-working-directory` was rejected at the pin too and is accepted on main (above). | `kiln_driver/src/args.rs` (no such options; none on main either); WP-052's review, re-check Q4 |

## Not in this batch

- **Already sent.** WP-071's K1 (no extent row for an explicit
  instantiation of a function or variable template) is KB1 1.1.
  WP-071's K4 and WP-096's item 3 (a dependent entry with no referent
  and no found set) are KB1 1.6, and KR1 stage 4 for the referent per
  specialisation: Kiln's WP-1933, on main at `352806a74`.
- **Already asked in KR1 stage 3** (Kiln's WP-1932, on main), the roles
  that WP-071 and WP-096 found unrecorded at the pin: a
  using-declaration's name, a designator, a type-constraint's concept, a
  friend's or elaborated specifier's name, and an explicit
  instantiation's target.
- **Already asked in KR4**, WP-096's item 2 (a pasted use of a macro
  argument has no range): acceptance case 5, stage 3 (Kiln's WP-2112).
- **Already in KB4.** WP-085's items 1 to 9 are KB4 2.1 to 2.7 and
  KB1 1.3 and 1.4. Its qualified friend (`namespace n { struct S; }
  struct T { friend struct n::S; };`, reproduced: the only row of
  `n::S` is the friend's) is KB1 1.3 and KB4 2.1 together. WP-041's
  fold operator set is KB4 1.3, and its profile questions are KB4 1.7
  and 1.8.
- **Not reproduced, dropped.** WP-086's "anonymous-union members with no
  table entry": `a`, `s.b` and a namespace-scope anonymous union's `x`
  all have entries with the field as their entity. The nodes without
  one are the implicit accesses of the unnamed member, which have no
  written name. WP-086's implicit object argument of an
  explicit-object member call has clang's shape, so that one is for Slip.
- **Not asked.** WP-113's review found one more clang behaviour: a
  reference emitted before a later plain redeclaration keeps the earlier
  declaration's visibility. That is clang's emission order, and Slip
  does not ask Kiln to copy it. WP-110's other holes are Slip's (a
  pasted name, the run-level check of WP-116).
