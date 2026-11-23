#include "side.h"
// WP-034 (§6): the taken branch reaches the instantiation's digest.
void g(int);
#ifdef B
template <class T> void f(T) { if constexpr (sizeof(T) == 4) { g(3); } else { g(2); } }
template void f<int>(int);
#else
template <class T> void f(T) { if constexpr (sizeof(T) == 4) { g(1); } else { g(2); } }
template void f<int>(int);
#endif
