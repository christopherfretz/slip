#include "side.h"
// §8 Scalar, digest §7: (unsigned __int128)1 << 100 against (unsigned __int128)1 << 101.
template <auto V> struct A;
#ifdef B
A<(unsigned __int128)1 << 101>* x;
#else
A<(unsigned __int128)1 << 100>* x;
#endif
