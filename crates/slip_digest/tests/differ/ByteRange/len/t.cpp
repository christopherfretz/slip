#include "side.h"
// §8 ByteRange `len`: padding of three bytes (`c` active) against two (`s` active).
template <auto V> struct A;
union U { char c; short s; int i; };
#ifdef B
A<U{.s = 1}>* x;
#else
A<U{.c = 1}>* x;
#endif
