#include "side.h"
// §7 ExprNode: `UnaryExprOrTypeTraitExpr.a-expr`: the fact differs.
int a = 1, b = 2, c = 3; long l;
#ifdef B
unsigned long v = sizeof(b);
#else
unsigned long v = sizeof(a);
#endif
