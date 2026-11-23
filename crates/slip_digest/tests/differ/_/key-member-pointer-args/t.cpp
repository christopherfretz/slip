#include "side.h"
// digest §3.4, WP-039 item 1: specialisations on two member pointers: two keys.
struct M { int a; int b; };
template <auto> struct N {};
template <> struct N<&M::a> {};
#ifdef B
template <> struct N<&M::b> {};
#endif
