#include "side.h"
// §10.1 AttrNode: `MODE.a`: the fact differs.
#ifdef B
template <class T> struct X { T v __attribute__((mode(SI))); };
#else
template <class T> struct X { T v __attribute__((mode(DI))); };
#endif
