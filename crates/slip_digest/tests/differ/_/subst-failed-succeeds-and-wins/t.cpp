#include "side.h"
// WP-136 item 5 (`spec/digest.md` §2.1, §8): a failing candidate begins to
// succeed and wins overload resolution; the using unit `u()` differs.
template <class T> struct Trait {};
#ifdef B
template <> struct Trait<int> { using type = int; };
#else
template <> struct Trait<int> {};
#endif
template <class T> auto g(T t) -> typename Trait<T>::type;
int g(...);
int u() { return g(1); }
