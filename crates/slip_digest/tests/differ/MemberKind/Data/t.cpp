#include "side.h"
// §8 MemberKind `Data` against `Function`.
template <auto V> struct A;
struct D { int x; int y; void f(); void g(); };
struct E : D { int z; };
struct F : E {};
#ifdef B
A<&D::f>* x;
#else
A<&D::x>* x;
#endif
