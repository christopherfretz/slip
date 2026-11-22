#include "side.h"
// digest §3.4, WP-039 item 1: specialisations on an enumerator against its value: two keys.
enum E { e0 };
template <auto> struct N {};
template <> struct N<0> {};
#ifdef B
template <> struct N<e0> {};
#endif
