#include "side.h"
// §8 Scalar `Pointer.unions`: `&u.b` against `&u.a` (same address, same type; `a` is the union's default member).
template <auto V> struct A;
union U { int a; int b; } u;
#ifdef B
A<&u.b>* x;
#else
A<&u.a>* x;
#endif
