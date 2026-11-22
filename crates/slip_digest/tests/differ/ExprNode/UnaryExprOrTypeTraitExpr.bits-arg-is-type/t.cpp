#include "side.h"
// §7 ExprNode: `UnaryExprOrTypeTraitExpr.bits-arg-is-type`: the fact differs.
int a = 1, b = 2, c = 3; long l;
#ifdef B
unsigned long v = sizeof(a);
#else
unsigned long v = sizeof(int);
#endif
