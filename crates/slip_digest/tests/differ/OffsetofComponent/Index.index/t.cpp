#include "side.h"
// §7 OffsetofComponent: `Index.index`: the fact differs.
int a = 1, b = 2, c = 3; long l;
#ifdef B
template <class T> struct A { unsigned long w = __builtin_offsetof(T, z[b][a]); };
#else
template <class T> struct A { unsigned long w = __builtin_offsetof(T, z[a][b]); };
#endif
