# KD3 — pointers to two members of one union are one constant

Sent to the Kiln agent 2026-11-22. Found by the independent review of
Slip's WP-028 (constant values in the digest), at Slip's pin
`62d190007`, through the snippet front end (`-std=c++20`).

```cpp
template <auto V> struct A {};
union U { int a; int b; } u;
static_assert(!__is_same(A<&u.a>, A<&u.b>));   // fails in Kiln
```

Kiln prints both types as `A<&u.a>`. `&u.a` and `&u.b` are different
pointer values ([basic.compound]/3: they point to different objects),
so they are not template-argument-equivalent ([temp.type]/2) and the
two specialisations are distinct types; clang keeps them apart (its
lvalue path holds the union member).

Cause, as far as we read it: `Scalar::Pointer` holds base, offset,
`designated` and `past`, and the comment at `PtrBase`
(`kiln_ast/src/consts/value.rs`, around lines 228 to 233) says the
designator path is "recovered from the base's type and the offset …
never stored". Two union members at one offset with one type are then
one `ConstValueId`. The same loss: `&u.a + 1` against `&u.b + 1`, and
`(void*)&u.a` against `(void*)&u.b` (there `designated` is no help).

What Slip needs: a pointer constant that tells union members apart
(the active-member path, or at least the field at each union on the
path), in `Scalar::Pointer` or beside it, so that the digest's rows
can write it. Until then it is a recorded hazard of the digest
(`spec/landed/digest-kinds/WP-028.md`) and a pair test marked
`# pending`, `# needs: kiln`.

Not urgent for the namespace migration; it is a wrong-code class of
defect for Kiln itself (two types merged).

## Also seen, lower priority

- Parsing a member-call chain is quadratic: `a.f().f()…` with 5,000
  calls takes 1.6 s in the front end, 20,000 takes 23.6 s (Slip
  `crates/slip_digest/tests/expr_chains.rs`; an operator chain of the
  same length is linear).
- Informational, no request: `ExprFlags::HAD_MULTIPLE_CANDIDATES` is
  never set by sema, and `ExprFlags::ADL` only for operator calls
  (`overload/oper.rs:1160`) and a structured binding's `get`. Slip's
  rows for both are `out`, so nothing depends on them.
