#include "side.h"
// §10.1 AttrNode: `MODE.b`: only the spelling differs.
#ifdef B
template <class T> struct X { T v __attribute__((mode(__DI__))); };
#else
template <class T> struct X { T v __attribute__((mode(DI))); };
#endif
