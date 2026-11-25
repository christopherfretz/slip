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

## 3. Not asked

- Array bounds and pack indices as written (`spec/digest.md` §2.3's
  limits, B1 and B7 of `spec/landed/digest/WP-153.md`): separate.
- Kiln's written-reference table (`-fkiln-dump-written-refs`, KR1)
  records references as written for the transform; the digest does not
  parse with it, and it holds references, not the operands' trees.

## 4. What Slip does on delivery

Reads each as a holder (`operands.rs` `signature`, `expr_node`), removes
the matching cause, and turns the indeterminate tests
(`tests/indeterminate/_/operands-*`) into `differ` pairs.
