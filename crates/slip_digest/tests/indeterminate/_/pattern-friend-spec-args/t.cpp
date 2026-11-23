#include "side.h"
// WP-124 item 4 (Kiln WP-3027 A2): a friend of a class template, its declarator kept off the AST; already indeterminate by spelling/friend-name (WP-104).
template<class T, class U = int> void f(T);
template<class T> struct S {
#ifdef B
  friend void f<T, long>(T);
#else
  friend void f<T, int>(T);
#endif
};
