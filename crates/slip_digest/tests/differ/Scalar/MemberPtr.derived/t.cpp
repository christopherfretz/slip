#include "side.h"
// §8 Scalar `MemberPtr.derived`: `E::z` reached from `D` (towards the derived class) against `D::y` (the member differs too).
template <auto V> struct A;
struct D { int x; int y; void f(); void g(); };
struct E : D { int z; };
struct F : E {};
#ifdef B
A<&D::y>* x;
#else
A<static_cast<int D::*>(&E::z)>* x;
#endif
