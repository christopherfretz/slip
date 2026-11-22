#include "side.h"
// §7 ArrayTraitExtra: `dim`: the fact differs.
#ifdef B
template <class T> struct A { unsigned long w = __array_extent(T, 1); };
#else
template <class T> struct A { unsigned long w = __array_extent(T, 0); };
#endif
