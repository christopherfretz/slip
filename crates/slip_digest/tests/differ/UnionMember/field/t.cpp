#include "side.h"
// §8 UnionMember `field`: the active member, `c` against `i`.
template <auto V> struct A;
union U { char c; short s; int i; };
#ifdef B
A<U{.i = 0}>* x;
#else
A<U{.c = 0}>* x;
#endif
