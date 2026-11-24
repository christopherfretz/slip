# KR8 — review of Kiln's WP-3403 design: hidden using-shadows in both orders

Slip's answer to Kiln's design for the hidden-shadow row of
[spec/landed/edits/WP-127.md](../../spec/landed/edits/WP-127.md) "For
Kiln" (hidden shadows in both orders; the non-virtual hider). Status:
**answered, GO for phase 1** (2026-11-24).

## What Kiln proposed

Kiln WP-3145/3146 (`690711450`, not yet on Kiln's main; later than
Slip's pin) already make a using-shadow for a base member that a member
of the class declared **before** the using-declaration hides or
overrides, marked hidden with its hider (`DeclArena::hidden_shadow`),
as for a member declared after. Option A keeps that and adds: a fix for
an instantiated class-template specialisation whose hiding member is
declared after the using-declaration (the mark is missing there);
total accessors `Ast::shadow_hider(shadow)`, `Ast::using_named(using)`
(every shadow of the using-declaration, hidden or not; the visible set
is those with no hider), `Ast::shadows_hidden_by(member)`,
`Ast::overridden_methods(fn)`; tests. A virtual override: the shadow
is hidden by the overrider in both orders, the override relation is
the separate list. Constructors: a derived constructor with the same
parameter list is the hider; inherited copy and move constructors that
overload resolution excludes are not marked. Templates: the hider is
the member template; marks are per specialisation.

Rejected by Kiln, and by Slip: a side row of (base member, hider)
pairs with no shadow.

## The Slip need

The rename reads what a using-declaration names
(`crates/slip_refactor/src/rename/followers.rs` `named_set`) by its
own [class.member.lookup] (`member_lookup`), because the shadows
depend on the order of the members; that reading does not do
dominance and refuses there. The digest writes no unit for a hidden
shadow (`crates/slip_digest/src/units.rs` `hidden_shadow`) so that the
two orders digest alike. One recorded shape lets both read the front
end.

## Answers

1. The shape is enough, with the requirements below.
2. Both accessors are wanted (`overridden_methods` replaces Slip's
   raw read of `decls.cold.overrides`; `shadows_hidden_by` serves the
   rename when the renamed entity is the hider; a scan, no storage).
3. A record is wanted where a second using-declarator names an entity
   an earlier one already introduced through another class
   (`using B1::f; using B2::f;`): both declarators are edited or
   neither. Preferred: the shape Kiln has at namespace scope (the
   second using-declaration's own shadow, a redeclaration of the
   first). Acceptable: `using_named` returning the earlier
   using-declaration's shadow, documented, with an accessor from a
   shadow to its using-declaration. The member-enumerator case
   (`using E::A;` in the class of `E`): state what `using_named` gives.
4. The dump may keep leaving hidden shadows out. Slip does not read it.
5. Yes: one mark, the hider is the overrider; the overrides list tells
   an override from a plain hiding.
6. `instantiated_shadow` (Kiln WP-3400) stays as the structural map
   from a pattern's shadow to the specialisation's, documented as
   "may be hidden there". Asked: a test and a sentence for what an
   instantiated found list holds when that shadow is hidden in the
   specialisation.

## Requirements

- **R1.** A mark does not depend on what the translation unit used. A
  shadow hidden by an implicitly declared member is marked today only
  once the member is declared; the hider must at least stay
  recognisable as implicit (the digest filters on it), and the spec
  says when such a mark is set.
- **R2.** The pattern's mark where correspondence depends on a
  template parameter (`template <class T> struct D : B { using B::f;
  void f(T); };`) is stated (expected: unmarked in the pattern, marked
  per specialisation), with a test in both orders.
- **R3.** The hider is named by a stated declaration of it, and for a
  non-virtual hiding is exactly the member that corresponds.
- **R4.** Whether `lookup_local` and found lists can hold a hidden
  shadow is stated, one rule for both orders.

## What Slip does when it lands

At the pin move that brings `690711450`: nothing has to change (the
digest's filter and `member_lookup`'s skip already read the mark). A
package then drops `member_lookup` for `using_named`, and the
`wp127_dominance_limit_refused` limit with it.
