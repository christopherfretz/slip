# KT1 — what a declaration wrote that Kiln does not keep

A request from Slip to Kiln. Status: **draft, not sent** (WP-156 fix
round 1, 2026-11-25; items 2.4 to 2.7 and the written arguments of 2.2
and 2.3 added in fix round 2). Kiln is parked on the operator's word; the
coordinator sends this when Kiln is back. Kiln was read at Slip's pin.

## 1. The Slip need

Slip's digest writes a type by its canonical form. A name inside an
expression that only a written type holds is read from the type as
written (`spec/digest.md` §2.4, operand units):

```cpp
namespace legacy { int make(int); }      // a dependency
template <class T> int make(T);
namespace acme {                         // moved from legacy
Same<decltype(make(0))> auto v = 1;      // make: legacy::make before, ::make<int> after
}
```

Both bindings give `int`, so every unit compares equal unless the
operand `make(0)` is read where it is written. The places below hold
such an operand and Kiln keeps none of what was written there. Slip fails
closed on each today (the operand unit is indeterminate for a named
cause), which refuses correct transforms in the translation units
concerned. Each item below would let Slip read the operand instead.

## 2. The requests

### 2.1 A placeholder's type as declared

`Same<decltype(make(0))> auto v = 1;` and `Same<decltype(make(0))> auto
g() { return 1; }`: after deduction the declaration's type is the
deduced one (`VarData`'s `VarPlaceholder::Auto`; for a function,
`FnSpec::return_deduced` and the function type's return type `int`).
The type-constraint `Same<decltype(make(0))>` is kept only in the type
arena, linked to nothing. `kiln_sema` keeps a function's declared return
type in `Sema::declared_returns`, which is not in the AST.

Asked: the declared type of a variable whose type a placeholder deduced,
and the declared return type of a function whose return type was
deduced, as a side table of the AST keyed by the declaration (the
`Auto` type with its `concept` and `args`, as written). Also for a
`new` of a placeholder type (`new C<…> auto(1)`).

Slip's cause today: `kiln/placeholder-constraint`, when an operand of a
constrained placeholder's arguments that no declaration links is
located between the bounds around the declaration's name (or inside
the `new`).

### 2.2 An explicit instantiation's arguments and declarator as written

`template int ft<sizeof(make(0))>();`, `template void
fx(Wr<decltype(make(0))>);`, `template int vt<decltype(make(0))>;`,
`template int Q<sizeof(make(0))>::f();`: no `args_as_written` is set on
the specialisation (it is for a class's explicit instantiation,
`templ/spec.rs`), and the declarator's types are not kept.

The directive's own exception specification (`template void
g<int>(int) noexcept(sizeof(make(0)) == 4);`, also with `extern`, and
for a member, `template void G6<int>::f() noexcept(...)`) is not kept
either: the specialisation's is substituted from the pattern. Nor is
the directive's location (the point of instantiation is set only when
it is invalid, and an `extern template` and a later definition share
one declaration), so a non-type argument written as a literal (`extern
template int at<3>();`) cannot be told from one written with an
operand.

Asked: per explicit-instantiation directive of a function, a variable
or a member, its template argument list as written
(`args_as_written`, or a list per directive since one specialisation
can be named by several), its declarator's type as written (with its
exception specification), its qualifier as written (2.3) and its
location.

Slip's cause today: `kiln/explicit-instantiation-written`, when the
directive may have spelt an operand (a non-type argument, a type
argument that is canonically a held type no declaration links, a
function type equal up to its exception specification to such a
function type, a qualifier of 2.3).

### 2.3 A declarator's qualifier as written

`template <> int Q<sizeof(make(0))>::s = 5;`, `template <int> using AL =
Pl; int AL<sizeof(make(0))>::s = 2;`, `using enum
Q<sizeof(make(0))>::E;`: a declaration's lexical and semantic contexts
are kept (`lexical_ctx`), its qualifier as written is not (a
using-declaration's is, `Ast::using_qualifier`).

Asked: the nested-name-specifier written on a declarator-id (and on a
`using enum`, and on an explicit instantiation of a member), as an
`NnsId` keyed by the declaration, as `using_qualifier` is, with its
template arguments as written (Kiln keeps a specialisation's converted
arguments: `Q<1>` and `Q<sizeof(make(0))>` are one).

Slip's cause today: `kiln/declarator-qualifier`, for a declaration
written outside its class when a class around it is named by a
template-id with an operand that no declaration links, or is a
specialisation whose non-type argument may have been written with an
expression located before the declaration's name.

### 2.4 A friend specialisation's declarator

`template <class T> void tf(T); struct F2 { friend void
tf<>(Wr<decltype(make(0))>); };`: the friend-form row keeps the
friend's qualifier and template arguments as written (Kiln `spec/ast.md`
§23, read by Slip since fix round 2), but its target is the
specialisation, whose parameter types are substituted; the declarator's
types as written are linked to nothing. A friend read from a module
image carries no row at all.

Asked: the friend's declarator type as written, in the row; the row for
a friend read from a module image.

Slip's causes today: `kiln/friend-specialization-declarator`,
`kiln/friend-form-not-carried`.

### 2.5 A destructor named by a template-id or `decltype`

`wv.~Wr<decltype(make(0))>();`: the member access resolves to the
destructor and keeps no type-name as written (outside a template too;
in a template Slip's spelling level already refuses,
`spelling/destructor-name`).

Asked: the destroyed type as written on a member access that names a
destructor, as `CXXPseudoDestructorExpr` keeps it.

Slip's cause today: `kiln/destructor-name`, for a member access naming
a destructor of a class that a template-id or `decltype` with an
operand no declaration links names.

### 2.6 A `_BitInt`'s width as written

`_BitInt(sizeof(make(0)) * 8) bi = 0;`: `TypeView::BitInt` keeps the
width as a number, `DependentBitInt` as a profile; the expression is
not kept.

Asked: the width expression as written, on both.

Slip's cause today: `kiln/bit-int-width`, for a pattern's `_BitInt(e)`,
and for another's where an expression that is no literal is located
within the declaration's bounds (2.7) or inside the expression.

### 2.7 Where a declaration ends

Slip bounds what one declaration spelt by the names of the
declarations around it and the ends of their bodies and initialisers
(`kiln/placeholder-constraint`, `kiln/declarator-qualifier`,
`kiln/bit-int-width`). Kiln's
declaration extents are opt-in (`decl_extents`, for the transform), so
the digest does not read them. With 2.1 to 2.3 and 2.6 delivered this
is not needed.

### 2.8 Every expression parsed and kept on no node (fix round 3)

The general form of 2.2 to 2.6, and the one fact that ends the list.
Kiln parses and evaluates expressions that no node reachable from a
declaration holds: a directive's non-type argument and `noexcept`, a
friend's declarator, a declarator qualifier's argument, a destructor
template-id's argument, a `_BitInt` width, the argument of
`vector_size`, `ext_vector_type` and `address_space`, and whatever
else is folded to a value.

Asked: one list, in the AST and the image, of `(ExprId, DeclId)`:
every expression root that was parsed and then folded or dropped, with
the declaration whose text holds it (the translation unit's for a
directive). Slip reads each as an operand of its holder; a type name
alone in such an expression (`sizeof(X)`) is then walked as any other.
Design part (Kiln's Fable): which holder a directive's expression has,
and whether the list or a slot per kind (2.2 to 2.6) is the better
shape; Slip needs either, not both.

### 2.9 Promises about the written-reference table (fix round 3)

From fix round 3 the digest parses with the table (`spec/ast.md` §18)
and proves its closure with it: every entry that has an entity and is
not dependent is walked by a stream, or written by its entity. Slip
leans on these, read from §18 at `60fb5ab9d`; each is asked as a
stated promise with a test, or a correction if it is not so:

1. A name written in any of the places of 2.1 to 2.6 and 2.8, and in
   a dynamic exception specification, has an entry: its `entity` is
   the overload chosen, its `enclosing` the declaration of §18.3 (an
   explicit instantiation of a function or variable: the context its
   qualifier enters).
2. The same when the name comes from a macro's body or a macro's
   argument.
3. The join of §18.11.1 (`name_lo(id) == loc()` and the same entity)
   holds for a `DeclRefExpr` and `MemberExpr` whose name comes from a
   macro's body or argument, and the operator join of §18.13.7 for an
   operator and a literal suffix.
4. `enclosing` never names a declaration of an instantiation for an
   entry of the main table, except the class an explicit
   instantiation's qualifier enters (fact (a) below).
5. An entry of a type-constraint has role `Concept`, and the entries
   of a declaration are in source order up to its `Redecl` entry.

What the probes found at `60fb5ab9d` (fix round 3, commit 1,
`crates/slip_digest/tests/ledger_facts.rs`; every listed shape has its
entry, entity and holder, and every walked name, operator and literal
suffix joins, in macros too). Five facts go against the promises as
worded, or against §18.3:

- (a) 4 as worded: an explicit instantiation of a member,
  `template auto GM<int>::f() noexcept(e);`, holds `e`'s names by the
  implicit instantiation `GM<int>` (the context the qualifier enters,
  as §18.3 says). Promise 4 is to read: "except the class an explicit
  instantiation's qualifier enters".
- (b) Against §18.3 and 4: in `template <> int Q<4>::s = make(x);` and
  `template <> auto QR<4>::g(X1) -> decltype(make(x)) {...}`, the
  initializer's and the trailing return type's names are held by the
  instantiated class `Q<4>` (`QR<4>`), not by the declaration. The
  qualifier's own names, the parameters' and the body's are held by
  the declaration. (Presumably the class is created after the
  declaration's mark, so the claim keeps it as a nested holder.)
  Slip takes what this gives (the coordinator's ruling): the entry
  belongs to the class's unit, which does not walk it, so it is a
  site-9 entry there, the same on both sides; if the class's unit is
  not written, the entry is unclaimed and the translation unit fails
  closed. The corpus command counts the shape by its own label. Asked:
  the declaration as the holder, as §18.3 says.
- (c) Against §18.3: a late-parsed exception specification in a class
  (`struct ND { void f() noexcept(e); };`, a friend's `noexcept`) holds
  `e`'s names by the class, not by the function; a late-parsed default
  argument is held by its parameter, as §18.3 says.
- (d) An observation, not a request: a function definition's
  `Redecl` entry is recorded after its parameters' and its body's
  entries (`template <> void QF<4>::f() { make(x); }`: qualifier,
  body, then `Redecl`); a variable's after its qualifier and before
  its initializer. Slip depends on no order beyond the qualifier
  chain: T2's range ends at the last entry of the `Redecl` entry's
  qualifier chain (the coordinator's ruling of 2026-11-28).
- (e) Against 3 (§18.11.1): a destructor named by `decltype`
  (`wv.~decltype(e)()`) has no `member` entry: the `MemberExpr` at the
  `decltype` joins nothing. Its operand's names have entries.

A note from fix round 3's commit 4, not a request (the coordinator's
ruling): a local class declared in a lambda inside an array bound
(`void f() { int a[([] { struct L { static constexpr unsigned m(); };
return L::m(); }())]; }`) is a named over-refusal of the digest,
`operands/unwalked-local`. The bound is a value only, no stream walks
it, so `L::m` is written by its entity, and `L` is in no numbering of
`f`. A walked array bound (the bound as written, KR9 §5.11) or the
lambda's locals numbered in `f` would make it determinate.

### 2.10 A lean recording switch (fix round 3)

The digest reads `range.lo`, `entity`, `enclosing`, `role`, `flags`
and the qualifier chain. It does not read macro provenance beyond the
token's own location, the per-specialisation rows (§18.9), nor
declaration extents. §18.7.6 gives about +10% parse time and +37%
resident memory for the full table on `SemaExpr.cpp`.

Asked: a switch that records only the main table's entries, and its
cost on the same file. With 2.8 delivered the digest needs the table
only as a test oracle and stops recording it.

## 3. Not asked

- Array bounds and pack indices as written (`spec/digest.md` §2.3's
  limits, B1 and B7 of `spec/landed/digest/WP-153.md`): separate.
- The operands' trees from the written-reference table: it holds
  references, not trees, and the digest reads it only to account for
  names (2.9). (Before fix round 3 this file said the digest does not
  parse with the table; it now does.)

## 3.1 State of delivery (2026-11-28)

2.1 is delivered at Kiln main `00415ed7f` (WP-3640, PCH 188, IMAGE
102): constraint arguments and array bounds around a deduced
placeholder, in `DeclArena::written_decl_type`, on specialisations
too. Slip's pin is `60fb5ab9d`; the read lands with the pin move, a
package of its own. Slip does not use `Ast::decl_written_exprs`, so
WP-3641 (no deduced-return expressions there) changes nothing for it.
2.7 is withdrawn: fix round 3 bounds nothing by location.

## 4. What Slip does on delivery

Reads each as a holder (`operands.rs` `signature`, `expr_node`), removes
the matching cause, and turns the indeterminate tests
(`tests/indeterminate/_/operands-*`) into `differ` pairs.
