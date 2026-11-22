#include "side.h"
// §11.4 FieldData `init`: absent in an implicit instantiation whatever Kiln holds, so instantiating the initialiser changes no unit.
#ifdef B
template <class T> struct X { int m = 1; }; int s = sizeof(X<int>); void v() { X<int> x; }
#else
template <class T> struct X { int m = 1; }; int s = sizeof(X<int>);
#endif
