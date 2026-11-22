#include "side.h"
// §8 NullKind `Pointer` against `MemberData`.
template <auto V> struct A;
struct D { int x; int y; void f(); void g(); };
struct E : D { int z; };
struct F : E {};
#ifdef B
A<(int D::*)nullptr>* x;
#else
A<(int*)nullptr>* x;
#endif
