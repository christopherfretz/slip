#include "side.h"
// §8 Scalar, digest §7: 0.0 against -0.0.
template <auto V> struct A;
#ifdef B
A<-0.0>* x;
#else
A<0.0>* x;
#endif
