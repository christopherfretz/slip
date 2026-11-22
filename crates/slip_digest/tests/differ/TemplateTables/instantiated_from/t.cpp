#include "side.h"
// §9.5 TemplateTables `instantiated_from`: a field of an implicit instantiation
// names the field of the pattern it was instantiated from: the primary's, or
// the partial specialisation's.
template <class T> struct X { int a; };
#ifdef B
template <class T> struct X<T*> { int a; };
X<int*> v;
#else
X<int*> v;
#endif
