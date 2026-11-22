#include "side.h"
// §7 OffsetofComponent: `Field.name`: the fact differs.
#ifdef B
template <class T> struct A { unsigned long w = __builtin_offsetof(T, m); };
#else
template <class T> struct A { unsigned long w = __builtin_offsetof(T, n); };
#endif
