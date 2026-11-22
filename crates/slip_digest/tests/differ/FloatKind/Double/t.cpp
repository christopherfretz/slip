#include "side.h"
// §8 FloatKind `Double` against `Single`.
template <auto V> struct A;
#ifdef B
A<1.0f>* x;
#else
A<1.0>* x;
#endif
