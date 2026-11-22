#include "side.h"
// §7 ExprNode: `OffsetOfExpr.b`: the fact differs.
struct S { int n; int m; int x[4]; int y[4]; };
#ifdef B
unsigned long v = __builtin_offsetof(S, m);
#else
unsigned long v = __builtin_offsetof(S, n);
#endif
