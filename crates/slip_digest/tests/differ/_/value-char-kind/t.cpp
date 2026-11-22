#include "side.h"
// §8 Scalar, digest §7: 'a' against u8'a'.
template <auto V> struct A;
#ifdef B
A<u8'a'>* x;
#else
A<'a'>* x;
#endif
