#include "side.h"
// §8 ValueKind `Float`: `0.0` against `0LL` (eight zero bytes both).
template <auto V> struct A;
#ifdef B
A<0LL>* x;
#else
A<0.0>* x;
#endif
