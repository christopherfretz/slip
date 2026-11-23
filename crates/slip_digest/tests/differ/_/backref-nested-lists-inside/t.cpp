#include "side.h"
// digest §4.3 back-references, `backref-nested-lists-inside`: the innermost initialiser of nested designated lists in a template is a node of two written forms; it differs.
struct N0 { int x; }; struct N1 { N0 a; }; struct N2 { N1 a; };
#ifdef B
template <class T> struct A { static inline N2 w = {.a = {.a = {.x = 1}}}; };
#else
template <class T> struct A { static inline N2 w = {.a = {.a = {.x = 2}}}; };
#endif
