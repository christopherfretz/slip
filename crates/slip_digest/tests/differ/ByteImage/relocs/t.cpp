#include "side.h"
// §8 ByteImage `relocs`: a union whose active member is a pointer against one whose is an integer.
template <auto V> struct A;
union V { int* p; long l; };
#ifdef B
A<V{.l = 0}>* x;
#else
A<V{.p = nullptr}>* x;
#endif
