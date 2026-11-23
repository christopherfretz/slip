#include "side.h"
// digest-kinds rule §2.2: `template <> template <class U> int A<long>::v = 1;` is written in source, so a declared unit.
template <class T> struct A { template <class U> static int v; };
#ifdef B
template <> template <class U> int A<long>::v = 1;
#endif
