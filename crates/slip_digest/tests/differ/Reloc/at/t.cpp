#include "side.h"
// §8 Reloc `at`: the pointer at byte 0 against at byte 8.
template <auto V> struct A;
union PU { int* p; struct { long n; int* q; } s; };
int g;
#ifdef B
A<PU{.s = {0, &g}}>* x;
#else
A<PU{.p = &g}>* x;
#endif
