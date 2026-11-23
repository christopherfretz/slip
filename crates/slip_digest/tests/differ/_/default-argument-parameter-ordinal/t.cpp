#include "side.h"
// digest-kinds §11.4 (WP-046 review F2, M25): an instantiated default
// argument names parameter j as local j, as the function's own stream
// does, whichever parameters it names: `sizeof(b)` is not `sizeof(a)`.
template <class T> struct A {
#ifdef B
  void f(T a, T b, long c = sizeof(b));
#else
  void f(T a, T b, long c = sizeof(a));
#endif
};
A<int> x;
void h() { x.f(1, 2); }
