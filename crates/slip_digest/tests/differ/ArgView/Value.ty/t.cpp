#include "side.h"
// §4.6 ArgView `Value.ty`: the same value 1 as `int` and as `long`.
template <auto V> struct A;
#ifdef B
A<1L>* x;
#else
A<1>* x;
#endif
