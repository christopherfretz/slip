#include "side.h"
// §8 Scalar, digest §7: (unsigned _BitInt(200))1 << 150 against (unsigned _BitInt(200))1 << 151.
template <auto V> struct A;
#ifdef B
A<(unsigned _BitInt(200))1 << 151>* x;
#else
A<(unsigned _BitInt(200))1 << 150>* x;
#endif
