#include "side.h"
// §8 Reloc `at`: the pointer to `g` at byte 0 against at byte 8 of a struct of two pointers (same type, bytes and padding).
template <auto V> struct A;
struct P2 { int* p; int* q; };
int g;
#ifdef B
A<P2{nullptr, &g}>* x;
#else
A<P2{&g, nullptr}>* x;
#endif
