#include "side.h"
// §8 Scalar `Float.ty`: 1 as `double` against 1 as `float`.
template <auto V> struct A;
#ifdef B
A<1.0f>* x;
#else
A<1.0>* x;
#endif
