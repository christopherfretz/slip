#include "side.h"
// §8 Scalar, digest §7: (unsigned short)65535 against (unsigned short)1.
template <auto V> struct A;
#ifdef B
A<(unsigned short)1>* x;
#else
A<(unsigned short)65535>* x;
#endif
