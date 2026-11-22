#include "side.h"
// §8 Scalar, digest §7: (short)-1 against (short)1.
template <auto V> struct A;
#ifdef B
A<(short)1>* x;
#else
A<(short)-1>* x;
#endif
