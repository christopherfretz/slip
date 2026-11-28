#include "side.h"
// WP-136 item 2 kind 4 (`SubstFailedKind::ClassPartialAmbiguity`): an
// ambiguous partial specialisation attempt under SFINAE (side A) and no
// attempt at all (side B) are one state for the incomplete type `X<S, S>*`.
namespace a {
struct S {};
template <class T, class U> struct X {};
template <class T> struct X<T, S> { using t = int; };
template <class U> struct X<S, U> { using t = int; };
template <class T> auto probe(T) -> typename X<T, S>::t;
}
int probe(...);
a::X<a::S, a::S> *p = nullptr;
int u() {
#ifdef B
    return ::probe(a::S{});
#else
    return probe(a::S{});
#endif
}
