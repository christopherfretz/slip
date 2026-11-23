#include "side.h"
// digest-kinds §11.9 (WP-045 review F1): as weak-friend-redeclares-member-used,
// without the call.
template <class T> struct X { void f(); };
struct A {
  friend
#ifdef B
  __attribute__((weak))
#endif
  void X<int>::f();
};
X<int> *p;
