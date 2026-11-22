#include "side.h"
// §8 ByteImage: a class with an array member, one element differs.
template <auto V> struct A;
struct P { int q[2]; };
#ifdef B
A<P{{1, 3}}>* x;
#else
A<P{{1, 2}}>* x;
#endif
