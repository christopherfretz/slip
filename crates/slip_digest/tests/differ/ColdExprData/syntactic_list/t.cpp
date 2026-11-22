#include "side.h"
// WP-035, digest-kinds §7 `ColdExprData` `syntactic_list`: a non-dependent designated list in a template keeps its written form for instantiation; the semantic forms are the same.
struct P { int x, y; };
#ifdef B
template <class T> struct A { static inline P w = {1, 2}; };
#else
template <class T> struct A { static inline P w = {.x = 1, .y = 2}; };
#endif
