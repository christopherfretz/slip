#include "side.h"
// digest §3.4, WP-039 item 1: explicit specialisations of a variadic template: two keys.
template <class... T> struct V {};
template <> struct V<int> {};
#ifdef B
template <> struct V<int, int> {};
#endif
