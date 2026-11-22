#include "side.h"
// WP-028: the key of a specialisation holds a class-type argument.
template <auto V> struct A {};
struct S { int i; };
#ifdef B
void f(A<S{2}>);
#else
void f(A<S{1}>);
#endif
