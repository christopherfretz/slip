#include "side.h"
// digest-kinds rule §2.2: `template <> template <class U> struct A<long>::C { void m(); };` is written in source, so a declared unit.
template <class T> struct A { template <class U> struct C; };
#ifdef B
template <> template <class U> struct A<long>::C { void m(); };
#endif
