#include "side.h"
// digest-kinds rule §2.2: `template <> enum A<long>::E : int { x };` is written in source, so a declared unit.
template <class T> struct A { enum E : int; };
#ifdef B
template <> enum A<long>::E : int { x };
#endif
