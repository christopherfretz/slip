#include "side.h"
// digest-kinds §11.4: under an explicit instantiation definition too the
// default argument is instantiated on use.
template <class T> struct A { void f(int a = sizeof(T)) {} };
template struct A<int>;
#ifdef B
void h() { A<int>().f(); }
#endif
