#include "side.h"
// digest-kinds §11.9: a member class of an implicit instantiation is
// defined on use.
template <class T> struct X { struct In { int a; int b; }; };
X<int> x;
#ifdef B
void h() { X<int>::In i; }
#endif
