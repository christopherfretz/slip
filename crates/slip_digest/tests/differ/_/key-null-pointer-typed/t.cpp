#include "side.h"
// digest §3.4, WP-039 item 1: specialisations on nullptr against a null int pointer: two keys.
template <auto> struct N {};
template <> struct N<nullptr> {};
#ifdef B
template <> struct N<(int *)nullptr> {};
#endif
