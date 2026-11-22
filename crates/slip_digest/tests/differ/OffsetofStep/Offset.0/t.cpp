#include "side.h"
// §7 OffsetofStep: `Offset.0`: the fact differs.
struct S { int n; int m; int x[4]; int y[4]; int z[4][4]; };
int a = 1, b = 2, c = 3; long l;
#ifdef B
unsigned long v = __builtin_offsetof(S, y[a]);
#else
unsigned long v = __builtin_offsetof(S, x[a]);
#endif
