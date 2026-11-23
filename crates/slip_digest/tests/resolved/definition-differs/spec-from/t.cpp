#include "side.h"
// digest-kinds §11.9 rows `SpecInfo` `from`, `from_args`: the pattern a
// class specialisation is instantiated from.
template <class T> struct X { int a; };
#ifdef B
template <class T> struct X<T*> { int a; };
#endif
X<int*> x;
