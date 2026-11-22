#include "side.h"
// §8 ByteImage `padding`: the active member `c` leaves three bytes of padding, `i` none (same bytes).
template <auto V> struct A;
union U { char c; short s; int i; };
#ifdef B
A<U{.i = 1}>* x;
#else
A<U{.c = 1}>* x;
#endif
