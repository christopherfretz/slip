#include "side.h"
// §8 Scalar `Null.kind`: a null data-member pointer against a null member-function pointer.
template <auto V> struct A;
struct D { int x; int y; void f(); void g(); };
struct E : D { int z; };
struct F : E {};
#ifdef B
A<(void (D::*)())nullptr>* x;
#else
A<(int D::*)nullptr>* x;
#endif
