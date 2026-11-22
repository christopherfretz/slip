#include "side.h"
// §8 Scalar, digest §7: -1L against 1L.
template <auto V> struct A;
#ifdef B
A<1L>* x;
#else
A<-1L>* x;
#endif
