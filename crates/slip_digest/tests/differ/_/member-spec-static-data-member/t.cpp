#include "side.h"
// digest-kinds rule §2.2: `template <> int A<long>::s = 1;` is written in source, so a declared unit.
template <class T> struct A { static int s; };
#ifdef B
template <> int A<long>::s = 1;
#endif
