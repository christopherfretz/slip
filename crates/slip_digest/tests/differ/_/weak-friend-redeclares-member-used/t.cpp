#include "side.h"
// digest-kinds §11.9 (WP-045 review F1): a friend written in a class that
// is no instantiation redeclares a member of one, weak on side B. The
// declaration is written in source: its `weak` stays in X<int>::f()'s own
// unit (a weak undefined reference against a strong one).
template <class T> struct X { void f(); };
struct A {
  friend
#ifdef B
  __attribute__((weak))
#endif
  void X<int>::f();
};
void use(X<int>& x) { x.f(); }
