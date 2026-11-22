#include "side.h"
// §8 ConstValue `Aggregate.0`: the rows of an image.
template <auto V> struct A;
struct S { int i; };
#ifdef B
A<S{2}>* x;
#else
A<S{1}>* x;
#endif
