#include "side.h"
// §8 Scalar `Int.bits`: 1 against 2.
template <auto V> struct A;
#ifdef B
A<2>* x;
#else
A<1>* x;
#endif
