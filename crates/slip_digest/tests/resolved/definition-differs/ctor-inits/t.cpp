#include "side.h"
// digest-kinds §11.9 row `ColdDeclData` `ctor_inits`.
template <class T> struct A {
  int a, b;
#ifdef B
  A() : b(1) {}
#else
  A() : a(1) {}
#endif
};
A<int> x;
