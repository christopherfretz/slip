#include "side.h"
// §10.1 AttrNode: `DESTRUCTOR.flags-priority-dependent`: the fact differs.
#ifdef B
template <int N> struct X { __attribute__((destructor(N))) static void f(); };
#else
template <int N> struct X { __attribute__((destructor(101))) static void f(); };
#endif
