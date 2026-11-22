#include "side.h"
// §4.6 ArgView `Value.value`: `A<1>` against `A<2>`.
template <auto V> struct A;
#ifdef B
A<2>* x;
#else
A<1>* x;
#endif
