#include "side.h"
// WP-124 item 4 (Kiln WP-3027 A5): a friend of a class template, its declarator kept off the AST; already indeterminate by spelling/friend-name (WP-104).
template<class T> struct Bx { int g(int); };
template<class T> struct S {
#ifdef B
  friend int Bx<T*>::g(int);
#else
  friend int Bx<T>::g(int);
#endif
};
