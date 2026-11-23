#include "side.h"
// digest §2.2: using a member class of X<int> before the explicit
// instantiation definition changes no declared unit. The use is in a
// function body: one at namespace scope moves the order unit.
template <class T> struct X { struct In { int a; int b; }; };
#ifdef B
void use() { X<int>::In i0; }
template struct X<int>;
#else
template struct X<int>;
#endif
