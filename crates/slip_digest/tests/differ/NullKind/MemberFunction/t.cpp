#include "side.h"
// §8 NullKind `MemberFunction` against `Pointer`.
template <auto V> struct A;
struct D { int x; int y; void f(); void g(); };
struct E : D { int z; };
struct F : E {};
#ifdef B
A<(void (*)())nullptr>* x;
#else
A<(void (D::*)())nullptr>* x;
#endif
