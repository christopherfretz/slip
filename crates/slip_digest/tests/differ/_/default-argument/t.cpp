#include "side.h"
// digest-kinds §11.4: the instantiated default argument differs with its
// pattern's.
template <class T> struct A {
#ifdef B
  void f(int a = sizeof(T) + 1);
#else
  void f(int a = sizeof(T));
#endif
};
A<int> x;
void h() { x.f(); }
