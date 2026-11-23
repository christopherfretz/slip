#include "side.h"
// digest §2.2: using an enumerator of a member enumeration of an implicit
// instantiation changes no unit.
template <class T> struct A { enum F { f0 }; };
A<int> a;
#ifdef B
void use() { (void)A<int>::f0; }
#endif
