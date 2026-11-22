#include "side.h"
// §8 FloatKind `Quad` against `X87Extended`.
template <auto V> struct A;
#ifdef B
A<1.0L>* x;
#else
A<(__float128)1>* x;
#endif
