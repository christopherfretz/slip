#include "side.h"
// §8 Scalar `Bool.value`: `true` against `false`.
template <auto V> struct A;
#ifdef B
A<false>* x;
#else
A<true>* x;
#endif
