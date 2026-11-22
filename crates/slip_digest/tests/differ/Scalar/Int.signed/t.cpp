#include "side.h"
// §8 Scalar `Int.signed`: 1 as `int` against `1u` (the type differs too: signedness is the type's).
template <auto V> struct A;
#ifdef B
A<1u>* x;
#else
A<1>* x;
#endif
