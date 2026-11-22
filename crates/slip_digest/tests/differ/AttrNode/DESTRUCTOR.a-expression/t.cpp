#include "side.h"
// §10.1 AttrNode: `DESTRUCTOR.a-expression`: the fact differs.
#ifdef B
template <int N> struct X { __attribute__((destructor(N + 1))) static void f(); };
#else
template <int N> struct X { __attribute__((destructor(N))) static void f(); };
#endif
