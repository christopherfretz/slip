#include "side.h"
// digest §7: a value by structure, `1 + 1` against `2`.
template <auto V> struct A;
#ifdef B
A<2>* x;
#else
A<1 + 1>* x;
#endif
