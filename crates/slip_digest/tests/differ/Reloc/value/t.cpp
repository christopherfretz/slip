#include "side.h"
// §8 Reloc `value`: the pointer the bytes hold, `&g` against `&h`.
template <auto V> struct A;
struct P { const int* p; };
int g, h;
#ifdef B
A<P{&h}>* x;
#else
A<P{&g}>* x;
#endif
