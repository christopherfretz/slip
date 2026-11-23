#include "side.h"
// digest §3.4, WP-039 item 1: partial specialisations whose patterns differ by a pack against a non-pack: two keys.
template <class...> struct V {};
template <class T> struct S;
template <class T> struct S<V<T>> {};
#ifdef B
template <class... T> struct S<V<T...>> {};
#endif
