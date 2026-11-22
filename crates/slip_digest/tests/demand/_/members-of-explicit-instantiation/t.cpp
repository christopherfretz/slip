#include "side.h"
// digest §2.2, digest-kinds §5.2: under an explicit instantiation
// definition every member is a declared unit, listed in its class
// (WP-045 decision 2); using them changes no unit.
template <class T> struct X { int a; void f() {} static int s; };
template <class T> int X<T>::s = 0;
template struct X<int>;
#ifdef B
void use() { X<int> v; v.f(); (void)X<int>::s; }
#else
void use() {}
#endif
