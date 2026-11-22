#include "side.h"
// §8 ByteRange `start`: padding from byte 1 (`c` active) against from byte 2 (`s` active).
template <auto V> struct A;
union U { char c; short s; int i; };
#ifdef B
A<U{.s = 1}>* x;
#else
A<U{.c = 1}>* x;
#endif
