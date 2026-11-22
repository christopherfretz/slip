#include "side.h"
// WP-028: the key of a specialisation holds a pointer-to-member argument.
template <auto V> struct A {};
struct D { int x; int y; void f(); void g(); };
struct E : D { int z; };
struct F : E {};
#ifdef B
void f(A<&D::y>);
#else
void f(A<&D::x>);
#endif
