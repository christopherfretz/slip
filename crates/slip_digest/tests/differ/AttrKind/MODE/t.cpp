#include "side.h"
// §10.1 AttrKind: `MODE`: the attribute on one side only.
#ifdef B
template <class T> struct X { T v __attribute__((mode(DI))); };
#else
template <class T> struct X { T v; };
#endif
