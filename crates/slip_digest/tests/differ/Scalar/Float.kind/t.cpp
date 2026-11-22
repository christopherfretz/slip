#include "side.h"
// §8 Scalar `Float.kind`: binary16 against bfloat16.
template <auto V> struct A;
#ifdef B
A<(__bf16)1>* x;
#else
A<(_Float16)1>* x;
#endif
