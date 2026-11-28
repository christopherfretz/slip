#include "side.h"
// WP-136 fix round 1 (`units.rs` kind-4 fail-closed check, M10): a SFINAE
// partial-specialisation ambiguity marks `X<int, int>` with
// `ClassPartialAmbiguity`, and a later explicit specialisation gives that
// same specialisation a definition and members. Because `X<int, int>` still
// carries `INVALID` + `ClassPartialAmbiguity` with a definition/members, the
// walk fails closed with `Unaccounted("decl/class-partial-ambiguity")`.
template <class T, class U> struct X {};
template <class T> struct X<T, int> { using t = int; };
template <class U> struct X<int, U> { using t = int; };
template <class T> auto probe(T) -> typename X<T, int>::t;
int probe(...);
int first = probe(1);
template <> struct X<int, int> { using t = int; };
X<int, int>::t after = 0;
