#include "side.h"
// §8 Scalar `MemberPtr.path`: `&D::x` as `int E::*` against as `int D::*`.
template <auto V> struct A;
struct D { int x; int y; void f(); void g(); };
struct E : D { int z; };
struct F : E {};
#ifdef B
A<&D::x>* x;
#else
A<(int E::*)&D::x>* x;
#endif
