#include "side.h"
// §8 Scalar, digest §7: -1LL against 1LL.
template <auto V> struct A;
#ifdef B
A<1LL>* x;
#else
A<-1LL>* x;
#endif
