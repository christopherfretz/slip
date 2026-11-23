#include "side.h"
// The use is in a function only B has (WP-034: a body is digested, so a
// use added to a body A has changes that body).
// digest §2.2: every member of an implicit instantiation is derived, so a
// use of X<int> added in a function adds no declared unit.
template <class T> struct X {
  typedef T type;
  enum E { e1 };
  static int s;
  struct N {};
  using U = T*;
  int m;
};
#ifdef B
void g() { X<int> v; (void)v; }
#endif
