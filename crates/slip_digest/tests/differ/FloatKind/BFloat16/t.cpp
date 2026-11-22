#include "side.h"
// §8 FloatKind `BFloat16` against `Half`.
template <auto V> struct A;
#ifdef B
A<(_Float16)1>* x;
#else
A<(__bf16)1>* x;
#endif
