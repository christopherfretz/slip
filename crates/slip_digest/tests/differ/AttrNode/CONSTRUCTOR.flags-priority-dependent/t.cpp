#include "side.h"
// §10.1 AttrNode: `CONSTRUCTOR.flags-priority-dependent`: the fact differs.
#ifdef B
template <int N> struct X { __attribute__((constructor(N))) static void f(); };
#else
template <int N> struct X { __attribute__((constructor(101))) static void f(); };
#endif
