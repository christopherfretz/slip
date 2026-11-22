#include "side.h"
// spec/digest.md §1 rule 2: alias template sugar does not matter.
#ifdef B
template <class T> using P = T*; P<int> v;
#else
template <class T> using P = T*; int* v;
#endif
