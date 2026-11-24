#include "side.h"
// §7 OffsetofNode: a base stepped through against a member named
struct A { int x; };
#ifdef B
struct D { A a; };
unsigned long v = __builtin_offsetof(D, a.x);
#else
struct D : A {};
unsigned long v = __builtin_offsetof(D, x);
#endif
