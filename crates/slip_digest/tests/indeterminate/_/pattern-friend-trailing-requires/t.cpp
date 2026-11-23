#include "side.h"
// WP-124 item 4 (Kiln WP-3027 A4): a friend of a class template, its declarator kept off the AST; already indeterminate by spelling/friend-name (WP-104).
template<class T> concept C = sizeof(T) == 4;
template<class T> concept D = sizeof(T) == 8;
template<class T> struct A { struct X { template<class U> void h(U) requires C<U>; template<class U> void h(U) requires D<U>; }; };
template<class T> struct S {
#ifdef B
  template<class U> friend void A<T>::X::h(U) requires D<U>;
#else
  template<class U> friend void A<T>::X::h(U) requires C<U>;
#endif
};
