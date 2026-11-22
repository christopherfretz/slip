#include "side.h"
// §8 ByteImage `bytes`: `S{1, 2}` against `S{1, 3}`.
template <auto V> struct A;
struct S { char c; int i; };
#ifdef B
A<S{1, 3}>* x;
#else
A<S{1, 2}>* x;
#endif
