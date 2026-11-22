#include "side.h"
// digest §2.2: under an explicit instantiation definition, fields are declared
// units and listed while member functions and static members are derived and
// not listed; using them changes no declared unit.
template <class T> struct X { int a; void f() {} static int s; };
template <class T> int X<T>::s = 0;
template struct X<int>;
#ifdef B
void use() { X<int> v; v.f(); (void)X<int>::s; }
#else
void use() {}
#endif
