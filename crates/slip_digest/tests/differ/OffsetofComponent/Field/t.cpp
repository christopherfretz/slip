#include "side.h"
// §7 OffsetofComponent: `Field`: the fact differs.
#ifdef B
template <class T> struct A { unsigned long w = __builtin_offsetof(T, x[0]); };
#else
template <class T> struct A { unsigned long w = __builtin_offsetof(T, x.n); };
#endif
