#include "side.h"
// §7 ExprNode: `OffsetOfExpr.a`: the fact differs.
struct S { int n; int m; int x[4]; int y[4]; int z[4][4]; };
int a = 1, b = 2, c = 3; long l;
#ifdef B
unsigned long v = __builtin_offsetof(S, x[b]);
#else
unsigned long v = __builtin_offsetof(S, x[a]);
#endif
