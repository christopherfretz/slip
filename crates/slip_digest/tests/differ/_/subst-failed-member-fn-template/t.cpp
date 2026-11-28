#include "side.h"
// WP-136 item 4 ("Its kin"): failed substitution of a member function
// template of an instantiation (`S<int>::m<int>`).
template <class T> struct Trait {};
#ifdef B
template <> struct Trait<int> { using type = int; };
#else
template <> struct Trait<int> {};
#endif
template <class T> struct S {
    template <class U> auto m(U u) -> typename Trait<U>::type;
    int m(...);
};
int u() {
    S<int> s;
    return s.m(1);
}
