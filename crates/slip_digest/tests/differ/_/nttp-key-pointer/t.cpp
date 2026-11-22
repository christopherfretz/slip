#include "side.h"
// WP-028: the key of a specialisation holds a pointer argument.
template <auto V> struct A {};
int g, h;
#ifdef B
void f(A<&h>);
#else
void f(A<&g>);
#endif
