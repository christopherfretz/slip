#include "side.h"
// §8 ByteImage `ty`: two classes of one layout.
template <auto V> struct A;
struct S1 { int i; };
struct S2 { int i; };
#ifdef B
A<S2{1}>* x;
#else
A<S1{1}>* x;
#endif
