#include "side.h"
// spec/digest.md §1 rule 2: alias template sugar does not matter; its arguments are written in an operand unit (site 8, digest §2.4) on side B alone.
#ifdef B
template <class T> using P = T*; P<int> v;
#else
template <class T> using P = T*; int* v;
#endif
