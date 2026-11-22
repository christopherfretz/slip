#include "side.h"
// §8 Scalar `Float.bits`: 1.0 against 2.0.
template <auto V> struct A;
#ifdef B
A<2.0>* x;
#else
A<1.0>* x;
#endif
