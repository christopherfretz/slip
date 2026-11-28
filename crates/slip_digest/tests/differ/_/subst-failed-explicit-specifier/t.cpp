#include "side.h"
// WP-136 item 2 kind 2 (`SubstFailedKind::ExplicitSpecifier`): a constructor
// template whose `explicit(expr)` fails in substitution.
template <class T> struct Trait {};
#ifdef B
template <> struct Trait<int> { static constexpr bool value = false; };
#else
template <> struct Trait<int> {};
#endif
struct S {
    template <class T> explicit(Trait<T>::value) S(T);
    S(...);
};
S s = 1;
