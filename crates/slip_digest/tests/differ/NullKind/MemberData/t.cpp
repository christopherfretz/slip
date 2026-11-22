#include "side.h"
// §8 NullKind `MemberData` against `MemberFunction`.
template <auto V> struct A;
struct D { int x; int y; void f(); void g(); };
struct E : D { int z; };
struct F : E {};
#ifdef B
A<(void (D::*)())nullptr>* x;
#else
A<(int D::*)nullptr>* x;
#endif
