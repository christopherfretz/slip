#include "side.h"
// WP-108 (spec/verify.md §4 step 4, "Why step 4"): the explicit
// specialisation that side A writes and the implicit instantiation of the
// primary template that side B makes for the same call have one key, so
// user() is not changed: the call writes the same key on both sides, and
// what it calls is declared on A and derived on B.
template <class T> int pick() { return 1; }
#ifndef B
template <> int pick<int>() { return 2; }
#endif
int user() { return pick<int>(); }
