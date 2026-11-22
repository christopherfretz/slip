#include "side.h"
// §8 Scalar, digest §7: (signed char)-1 against (signed char)1.
template <auto V> struct A;
#ifdef B
A<(signed char)1>* x;
#else
A<(signed char)-1>* x;
#endif
