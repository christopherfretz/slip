#include "side.h"
// digest §3.4, WP-039 item 1: specialisations on a null pointer against 0: two keys.
template <auto> struct N {};
template <> struct N<0> {};
#ifdef B
template <> struct N<nullptr> {};
#endif
