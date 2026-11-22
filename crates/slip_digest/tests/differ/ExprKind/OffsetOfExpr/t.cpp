#include "side.h"
// §7 ExprKind: `OffsetOfExpr`: the kind on side A only.
struct S { int n; int m; int x[4]; int y[4]; int z[4][4]; };
#ifdef B
unsigned long v = 4ul;
#else
unsigned long v = __builtin_offsetof(S, m);
#endif
