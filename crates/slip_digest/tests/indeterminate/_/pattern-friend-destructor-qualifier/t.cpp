#include "side.h"
// WP-124 item 4 (Kiln WP-3027 A3): a friend of a class template, its declarator kept off the AST; already indeterminate by spelling/friend-name (WP-104).
template<class T> struct A { struct X { ~X(); }; };
template<class T> struct S {
#ifdef B
  friend A<T*>::X::~X();
#else
  friend A<T>::X::~X();
#endif
};
