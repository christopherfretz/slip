#include "side.h"
// WP-155: a static data member of a class template with a constant
// initialiser that no constant expression reads, instantiated in side A
// only. Kiln does not evaluate it; its initialiser is plainly constant
// (spec/digest.md §11 "The mark", rule 2), so it is not marked.
template <class T> struct Z { static int z; };
template <class T> int Z<T>::z = 0;
#ifndef B
int *pz = &Z<int>::z;
#endif
