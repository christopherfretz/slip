#include "side.h"
// §8 UnionMember `field`: the active member, `a` against `b` (same type, bytes and padding).
template <auto V> struct A;
union U2 { int a; int b; };
#ifdef B
A<U2{.b = 0}>* x;
#else
A<U2{.a = 0}>* x;
#endif
