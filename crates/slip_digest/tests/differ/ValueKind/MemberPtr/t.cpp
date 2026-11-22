#include "side.h"
// §8 ValueKind `MemberPtr`: `&D::x` against a null pointer to data member (same type).
template <auto V> struct A;
struct D { int x; int y; void f(); void g(); };
struct E : D { int z; };
struct F : E {};
#ifdef B
A<(int D::*)nullptr>* x;
#else
A<&D::x>* x;
#endif
