#include "side.h"
// WP-034 (§6): the discarded branch of the explicit instantiation changes the pattern only: Kiln stores an empty block for it.
void g(int);
#ifdef B
template <class T> void f(T) { if constexpr (sizeof(T) == 4) { g(1); } else { g(3); } }
template void f<int>(int);
#else
template <class T> void f(T) { if constexpr (sizeof(T) == 4) { g(1); } else { g(2); } }
template void f<int>(int);
#endif
