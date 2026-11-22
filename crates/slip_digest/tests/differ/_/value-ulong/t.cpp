#include "side.h"
// §8 Scalar, digest §7: ~0UL against 1UL.
template <auto V> struct A;
#ifdef B
A<1UL>* x;
#else
A<~0UL>* x;
#endif
