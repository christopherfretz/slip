#include "side.h"
// §10.1 AttrNode: `CONSTRUCTOR.a-expression`: the fact differs.
#ifdef B
template <int N> struct X { __attribute__((constructor(N + 1))) static void f(); };
#else
template <int N> struct X { __attribute__((constructor(N))) static void f(); };
#endif
