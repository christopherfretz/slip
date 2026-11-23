#include "side.h"
// digest-kinds §11.4: an instantiated default member initialiser, made
// by the use on side B (A<int>'s default constructor).
template <class T> struct A { int m = sizeof(T); };
int n = sizeof(A<int>);
#ifdef B
void h() { A<int> y; }
#endif
