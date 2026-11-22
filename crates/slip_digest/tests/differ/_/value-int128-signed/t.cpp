#include "side.h"
// §8 Scalar, digest §7: (__int128)-1 against (__int128)1.
template <auto V> struct A;
#ifdef B
A<(__int128)1>* x;
#else
A<(__int128)-1>* x;
#endif
