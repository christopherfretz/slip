#include "side.h"
// digest-kinds §11.9 (WP-045 review, N18): a variable's attributes are not
// moved: S<int>::v's own unit holds the `weak` instantiated from its
// declaration in the pattern.
template <class T> struct S {
#ifdef B
  __attribute__((weak))
#endif
  static int v;
};
template <class T> int S<T>::v = 1;
int h() { return S<int>::v; }
