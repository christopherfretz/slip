# KB6 amendment 1 — findings of the reviews of WP-041, WP-095 and WP-119

To: the Kiln agent. From: Slip's supervisor. 2026-11-23.
Slip's pin: Kiln `913a490bb`. Reproducers named as paths are under
`slip:crates/slip_digest/tests/` on Slip's `main`.

All items are **implementation** against behaviour Kiln or the standard
already fixes, except where marked **design**. Each says what Slip
needs it for. Items 9.8 and 9.9 overlap Kiln's own inventory (its
WP-3027: A2 to A5) and are stated here with Slip's reproducers so the
record that WP-3124 designs can be checked against them.

## 9. A unit's own spellings (continued; WP-041 is merged)

Section 9's "provisional" is lifted: WP-041 merged at Slip `9802fefe`;
its landed file is `slip:spec/landed/digest-kinds/WP-041.md` ("For
Kiln" 1 to 11). 9.1 to 9.7 stand as sent.

### 9.8 A friend keeps no written name (**design**: the record's shape)

`friend void Bq<..>::g();` and `friend struct Bq<..>::Y;` in a class
template are resolved to `Bq<8>::g` (`FriendTarget::Decl`) and to the
unsugared `Bq<8>::Y` (`FriendTarget::Type`). `friend void h<..>();` and
`friend void Bq<8>::t<..>();` keep only the function type
(`FriendTarget::Type` with a `void ()`), not even the befriended
specialisation.

```cpp
template <int N> struct Bq { void g(); struct Y {}; };
int f(...);
template <class T> struct S {
  friend void Bq<sizeof(sizeof(f(T())))>::g();   // operand is instantiation-dependent, not dependent
  friend struct Bq<sizeof(sizeof(f(T())))>::Y;
};
```

Side B writes another expression of the same value. clang 19 accepts
one side and rejects the other at instantiation for a suitable `f`; the
Ast is the same on both sides.

**Slip's need.** The digest compares what a unit means before and after
a refactoring. It cannot see this difference, so since WP-041 every
friend named through a qualifier or with template arguments in
templated code is indeterminate (`spelling/friend-name`); that also
pends two older tests (`demand/_/friend-member-of-another-class`,
`differ/_/friend-redeclares-member-of-instantiation`). **Asked** (this
is Kiln's WP-3124 as we understand it): per friend declaration, on the
Ast, the written qualifier (a nested-name-specifier with its
expressions), the written template arguments, the befriended
specialisation resolved to its declaration, the friend's own
template-head, and a trailing requires-clause. Please send the record's
shape before implementation. Reproducers:
`indeterminate/_/spelling-friend-qualified-function`,
`-qualified-class`, `-template-arguments`.

### 9.9 Two friend forms Kiln accepts and clang 19 rejects

- A qualified friend function **definition**:
  `friend void Bq<..>::g() { }` ([class.friend]/6).
- A friend template naming a member of a specialisation that is not a
  template's own: `template <int M> friend struct Bq<..>::Z;` with a
  dependent argument that is not the friend template's parameter.

**Slip's need**: Slip treats a tree Kiln accepts as a program; a form
that is ill-formed must be diagnosed, or Slip verifies edits of text no
compiler takes.

### 9.10 `friend void decltype(Bq<..>{})::g();` does not parse

clang 19 accepts a `decltype` qualifier in a friend's declarator-id;
Kiln reports "expected member name or ';'". (Not KB5 2.2, Kiln WP-2828: that is where a
`decltype(...)::` qualifier starts in the written-reference table; this
is the parser, in a declarator.) **Slip's need**: a corpus file with this form does
not parse.

### 9.11 An explicit specialisation in a template keeps its converted arguments only

`template <> void h<sizeof(sizeof(f(T())))>()` inside a class template
is `S::h<8>` by its `SpecInfo.args`; the written expression is gone.
**Slip's need**: as 9.8 (`spelling/specialisation-arguments`, every
class-scope explicit specialisation in templated code is
indeterminate). **Asked**: keep the written arguments of a class-scope
explicit specialisation. Reproducer:
`indeterminate/_/spelling-specialisation-arguments-function`.

## 10. The subject's keys (WP-095: `slip:spec/landed/api/WP-095.md`)

### 10.1 Names of declarations that claim no extent are attributed to the lexical context

```cpp
struct S {};
using A = S;
enum E { e = sizeof(S) };
template <class T = S> void t() {}
struct C { using I = S; };
```

The `enclosing` of each `S` in the written-reference table is the
translation unit, the enumeration `E`, the translation unit, and the
class `C`. **Slip's need**: a transform asks "which declaration holds
this reference" to decide what an edit changes and which unit must be
verified; the alias, the enumerator and the template are the holders.
**Asked**: an alias, a typedef, an enumerator, a template (for its
head) and a `static_assert` claim their names, as other declarations
do.

### 10.2 A shared decl-specifier is enclosed by the first declarator only

In `struct S {}; S a, b;` there is one entry for `S`, enclosed by `a`.
**Slip's need**: a rename of `S` changes `b`'s type too; a consumer
that takes `a` as the only holder verifies one unit of two. **Asked**
(**design**, small): an `enclosing` that says the name is shared by the
group (for example the group's first declaration plus a mark, or the
list).

## 11. Using-declarations and members (WP-119, in its final gate; reproducers inline)

### 11.1 The hidden mark of a using-shadow depends on declaration order

```cpp
struct B { void f(); };
struct D : B { using B::f; void f(); };   // a shadow, marked hidden
struct E : B { void f(); using B::f; };   // no shadow at all
```

The two classes mean the same. **Slip's need**: the digest had to make
the two shapes alike by leaving hidden shadows out; one recorded shape
(a hidden shadow in both orders, or none in both) would let it read the
record as it is.

### 11.2 An implicit member declared on use sets the hidden mark

```cpp
struct B { B& operator=(const B&); };
struct D : B { using B::operator=; };
// with and without:  void u() { D d; d = d; }
```

Whether the shadow of `B::operator=` is marked hidden depends on
whether some function used `D`'s implicit assignment. **Slip's need**:
a class's record must not depend on its uses elsewhere in the
translation unit: two observers of one header then disagree about the
class. **Asked**: decide the mark when the class is completed (the
implicit members' signatures are known then).

### 11.3 An explicit instantiation of a member after its explicit specialisation is recorded nowhere

```cpp
template <class T> struct S { void f() {} };
template <> void S<int>::f() {}
template void S<int>::f();
```

The explicit instantiation has no effect on the program, and no record
either. **Slip's need**: a rename of `S::f` must rewrite the third
line; today it has nothing to find it by (the post-image then does not
compile: loud, and a refusal Slip cannot explain). **Asked**: a
declaration, or an entry of the written-reference table, for an
explicit instantiation that names an explicitly specialised member.
