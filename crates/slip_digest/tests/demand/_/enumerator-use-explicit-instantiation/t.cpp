#include "side.h"
// digest §2.2: using an enumerator of a member enumeration of an explicit
// instantiation changes no unit.
template <class T> struct X { enum E { e0 }; };
template struct X<int>;
#ifdef B
void use() { (void)X<int>::e0; }
#else
void use() {}
#endif
