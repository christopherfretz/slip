#include "side.h"
// WP-045 decisions 1-3: every member kind under an explicit
// instantiation definition, used after it.
template <class T> struct X {
  int a;
  int f() { return 1; }
  int g();
  static int s;
  struct In { int b; };
  enum E { e0, e1 };
  template <class U> U t(U u) { return u; }
};
template <class T> int X<T>::g() { return 2; }
template <class T> int X<T>::s = 3;
template struct X<int>;
#ifdef B
int use() { X<int> v; X<int>::In i; return v.a + v.f() + v.g() + X<int>::s + X<int>::e1 + i.b + v.t(1); }
#endif
