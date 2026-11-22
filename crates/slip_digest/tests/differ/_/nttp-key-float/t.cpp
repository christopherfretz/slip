#include "side.h"
// WP-028: the key of a specialisation holds a floating-point argument.
template <auto V> struct A {};
#ifdef B
void f(A<2.5>);
#else
void f(A<1.5>);
#endif
