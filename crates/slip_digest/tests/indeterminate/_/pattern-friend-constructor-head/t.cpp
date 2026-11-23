#include "side.h"
// WP-124 item 4 (Kiln WP-3027 A3): a friend of a class template, its declarator kept off the AST; already indeterminate by spelling/friend-name (WP-104).
template<class T> concept C = sizeof(T) == 4;
template<class T> struct A { struct X { template<class U> X(U); }; };
template<class T> struct S {
#ifdef B
  template<class U> requires C<U> friend A<T>::X::X(U);
#else
  template<class U> friend A<T>::X::X(U);
#endif
};
