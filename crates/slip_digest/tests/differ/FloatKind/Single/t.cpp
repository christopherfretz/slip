#include "side.h"
// §8 FloatKind `Single` against `Double`.
template <auto V> struct A;
#ifdef B
A<1.0>* x;
#else
A<1.0f>* x;
#endif
