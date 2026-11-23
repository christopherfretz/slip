#include "side.h"
// digest-kinds §11.4: the instantiated default member initialiser differs
// with its pattern's.
template <class T> struct A {
#ifdef B
  int m = sizeof(T) + 1;
#else
  int m = sizeof(T);
#endif
};
void h() { A<int> y; }
