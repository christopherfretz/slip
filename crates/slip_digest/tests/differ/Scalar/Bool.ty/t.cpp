#include "side.h"
// §8 Scalar `Bool.ty`: `true` against 1 (only `bool` has Bool values).
template <auto V> struct A;
#ifdef B
A<1>* x;
#else
A<true>* x;
#endif
