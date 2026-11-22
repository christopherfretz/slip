#include "side.h"
// §8 Scalar `MemberPtr.ty`: `&D::x` as `int D::*` against `const int D::*`.
template <auto V> struct A;
struct D { int x; int y; void f(); void g(); };
struct E : D { int z; };
struct F : E {};
#ifdef B
A<(const int D::*)&D::x>* x;
#else
A<&D::x>* x;
#endif
