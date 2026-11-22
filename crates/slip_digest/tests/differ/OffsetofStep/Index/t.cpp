#include "side.h"
// §7 OffsetofStep: `Index`: the steps differ in number and kind.
struct Q { int o; int p; };
struct S { int n; Q x[4]; };
int a = 1, b = 2, c = 3; long l;
#ifdef B
unsigned long v = __builtin_offsetof(S, x[a].p);
#else
unsigned long v = __builtin_offsetof(S, x[a]);
#endif
