#include "side.h"
// §8 Scalar, digest §7: 'a' against 'b'.
template <auto V> struct A;
#ifdef B
A<'b'>* x;
#else
A<'a'>* x;
#endif
