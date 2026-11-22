#include "side.h"
// §8 FloatKind `Half` against `BFloat16`.
template <auto V> struct A;
#ifdef B
A<(__bf16)1>* x;
#else
A<(_Float16)1>* x;
#endif
