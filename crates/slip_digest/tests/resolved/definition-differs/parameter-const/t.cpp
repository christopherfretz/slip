#include "side.h"
// digest-kinds §11.9 (WP-045, mutation M7): `definition` writes the
// parameters of its defining declaration. For a friend defined in a class
// template and declared in its namespace (digest §2.2), that is another
// declaration than the one the own unit reads: only its parameter is
// `const` (not part of the function's type), and the body does not name it.
void f(int x);
template <class T> struct A {
#ifdef B
  friend void f(const int x) {}
#else
  friend void f(int x) {}
#endif
};
A<int> a;
void g() { f(1); }
