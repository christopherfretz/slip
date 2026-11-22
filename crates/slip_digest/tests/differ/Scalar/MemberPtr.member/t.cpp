#include "side.h"
// §8 Scalar `MemberPtr.member`: `&D::x` against `&D::y`.
template <auto V> struct A;
struct D { int x; int y; void f(); void g(); };
struct E : D { int z; };
struct F : E {};
#ifdef B
A<&D::y>* x;
#else
A<&D::x>* x;
#endif
