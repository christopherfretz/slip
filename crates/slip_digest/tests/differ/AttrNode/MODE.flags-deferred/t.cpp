#include "side.h"
// §10.1 AttrNode: `MODE.flags-deferred`: the fact differs.
#ifdef B
template <class T> struct X { T v __attribute__((mode(DI))); };
#else
template <class T> struct X { int v __attribute__((mode(DI))); };
#endif
