#include "side.h"
// §8 Scalar, digest §7: u8'a' against u8'b'.
template <auto V> struct A;
#ifdef B
A<u8'b'>* x;
#else
A<u8'a'>* x;
#endif
