# KT1 — what a declaration wrote that Kiln does not keep

A request from Slip to Kiln. Status: **draft, not sent** (WP-156 fix
round 1, 2026-11-25). Kiln is parked on the operator's word; the
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
operand `make(0)` is read where it is written. Four places hold such an
operand and Kiln keeps none of what was written there. Slip fails
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

Slip's cause today: `kiln/placeholder-constraint`, when the translation
unit's type arena holds a constrained placeholder whose arguments hold
an operand.

### 2.2 An explicit instantiation's arguments and declarator as written

`template int ft<sizeof(make(0))>();`, `template void
fx(Wr<decltype(make(0))>);`, `template int vt<decltype(make(0))>;`,
`template int Q<sizeof(make(0))>::f();`: no `args_as_written` is set on
the specialisation (it is for a class's explicit instantiation,
`templ/spec.rs`), and the declarator's types are not kept.

Asked: per explicit-instantiation directive of a function, a variable
or a member, its template argument list as written
(`args_as_written`, or a list per directive since one specialisation
can be named by several), its declarator's type as written, and its
qualifier as written (2.3).

Slip's cause today: `kiln/explicit-instantiation-written`, when the
directive may have spelt an operand (a non-type argument, a type of the
arena that holds an operand, a qualifier of 2.3).

### 2.3 A declarator's qualifier as written

`template <> int Q<sizeof(make(0))>::s = 5;`, `template <int> using AL =
Pl; int AL<sizeof(make(0))>::s = 2;`, `using enum
Q<sizeof(make(0))>::E;`: a declaration's lexical and semantic contexts
are kept (`lexical_ctx`), its qualifier as written is not (a
using-declaration's is, `Ast::using_qualifier`).

Asked: the nested-name-specifier written on a declarator-id (and on a
`using enum`, and on an explicit instantiation of a member), as an
`NnsId` keyed by the declaration, as `using_qualifier` is.

Slip's cause today: `kiln/declarator-qualifier`, for a declaration
written outside its class when a class around it is named somewhere in
the translation unit by a template-id or `decltype` that holds an
operand.

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
