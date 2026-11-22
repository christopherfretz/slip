#include "side.h"
// §8 FloatKind `X87Extended` against `Double`.
template <auto V> struct A;
#ifdef B
A<1.0>* x;
#else
A<1.0L>* x;
#endif
