#include "side.h"
// §11.4 FieldData `init`: absent in an implicit instantiation whatever Kiln holds, so instantiating the initialiser changes no unit.
#ifdef B
template <class T> struct X { int m = 1; }; int s = sizeof(X<int>); X<int> v;
#else
template <class T> struct X { int m = 1; }; int s = sizeof(X<int>);
#endif
