#include "side.h"
// §8 ByteImage `unions`: a union with no active member against one with.
template <auto V> struct A;
union W { int a; constexpr W() {} constexpr W(int x) : a(x) {} };
#ifdef B
A<W{1}>* x;
#else
A<W{}>* x;
#endif
