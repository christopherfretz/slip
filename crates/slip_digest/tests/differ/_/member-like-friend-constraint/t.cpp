#include "side.h"
// digest-kinds §9.5: a member-like non-template friend whose
// requires-clause changes.
template <class T> struct A {
#ifdef B
  friend int f(A) requires (sizeof(T) > 1) { return 1; }
#else
  friend int f(A) requires (sizeof(T) > 0) { return 1; }
#endif
};
A<int> a;
