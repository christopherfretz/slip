#include "side.h"
// §7 OffsetofComponent: `Index`: the fact differs.
#ifdef B
template <class T> struct A { unsigned long w = __builtin_offsetof(T, x.n); };
#else
template <class T> struct A { unsigned long w = __builtin_offsetof(T, x[0]); };
#endif
