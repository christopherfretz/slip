#include "side.h"
// WP-155: a static data member of a class template with a constant
// initialiser that no constant expression reads, instantiated in side A
// only: whether Kiln evaluates it at instantiation decides the mark
// (spec/landed/verify/WP-155.md §1 "Constant initialisation").
template <class T> struct Z { static int z; };
template <class T> int Z<T>::z = 0;
#ifndef B
int *pz = &Z<int>::z;
#endif
