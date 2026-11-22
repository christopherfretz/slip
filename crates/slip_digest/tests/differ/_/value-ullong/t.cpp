#include "side.h"
// §8 Scalar, digest §7: ~0ULL against 1ULL.
template <auto V> struct A;
#ifdef B
A<1ULL>* x;
#else
A<~0ULL>* x;
#endif
