#include "side.h"
// digest-kinds §11.9: a member enumeration of an implicit instantiation;
// its enumerators are `definition`'s whether or not a use defines it.
template <class T> struct X { enum E { e0, e1 }; };
X<int> x;
#ifdef B
int h() { return X<int>::e1; }
#endif
