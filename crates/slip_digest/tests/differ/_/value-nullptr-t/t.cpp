#include "side.h"
// §8 Scalar, digest §7: nullptr against (int*)nullptr.
template <auto V> struct A;
#ifdef B
A<(int*)nullptr>* x;
#else
A<nullptr>* x;
#endif
