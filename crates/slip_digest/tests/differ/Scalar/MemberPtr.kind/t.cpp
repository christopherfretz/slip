#include "side.h"
// §8 Scalar `MemberPtr.kind`: a data member against a member function (the type differs too).
template <auto V> struct A;
struct D { int x; int y; void f(); void g(); };
struct E : D { int z; };
struct F : E {};
#ifdef B
A<&D::f>* x;
#else
A<&D::x>* x;
#endif
