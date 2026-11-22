#include "side.h"
// §8 Scalar, digest §7: (unsigned char)255 against (unsigned char)1.
template <auto V> struct A;
#ifdef B
A<(unsigned char)1>* x;
#else
A<(unsigned char)255>* x;
#endif
