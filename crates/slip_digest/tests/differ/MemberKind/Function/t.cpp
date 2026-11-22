#include "side.h"
// §8 MemberKind `Function`: `&D::f` against `&D::g`.
template <auto V> struct A;
struct D { int x; int y; void f(); void g(); };
struct E : D { int z; };
struct F : E {};
#ifdef B
A<&D::g>* x;
#else
A<&D::f>* x;
#endif
