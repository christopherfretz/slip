#include "side.h"
// §7 OffsetofNode: one member reached through another base, at one offset
struct A { int x; }; struct B1 : A {}; struct B2 : A {};
#ifdef B
struct D : B2 {};
#else
struct D : B1 {};
#endif
unsigned long v = __builtin_offsetof(D, x);
