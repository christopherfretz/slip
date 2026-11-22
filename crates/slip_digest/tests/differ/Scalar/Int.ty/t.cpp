#include "side.h"
// §8 Scalar `Int.ty`: 1 as `short` against 1 as `int`.
template <auto V> struct A;
#ifdef B
A<1>* x;
#else
A<(short)1>* x;
#endif
