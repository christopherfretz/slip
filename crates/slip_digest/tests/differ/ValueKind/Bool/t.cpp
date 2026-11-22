#include "side.h"
// §8 ValueKind `Bool`: `false` against `0`.
template <auto V> struct A;
#ifdef B
A<0>* x;
#else
A<false>* x;
#endif
