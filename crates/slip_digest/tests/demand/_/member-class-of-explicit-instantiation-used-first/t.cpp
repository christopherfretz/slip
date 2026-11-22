#include "side.h"
// digest §2.2: using a member class of X<int> before the explicit
// instantiation definition changes no declared unit.
template <class T> struct X { struct In { int a; int b; }; };
#ifdef B
X<int>::In i0;
template struct X<int>;
#else
template struct X<int>;
#endif
