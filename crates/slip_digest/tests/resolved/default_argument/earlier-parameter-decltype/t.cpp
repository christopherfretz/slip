#include "side.h"
// digest-kinds §11.4 (WP-046 review F2): an instantiated default argument
// naming an earlier parameter in a type (`decltype(a)`): the parameter is
// the local entity the function's own stream numbers it as.
template <class T> struct A { void f(T a, long b = decltype(a)()) {} };
A<int> x;
#ifdef B
void h() { x.f(1); }
#endif
