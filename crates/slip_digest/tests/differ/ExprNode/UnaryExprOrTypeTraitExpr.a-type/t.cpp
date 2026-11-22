#include "side.h"
// §7 ExprNode: `UnaryExprOrTypeTraitExpr.a-type`: the fact differs.
#ifdef B
unsigned long v = sizeof(long);
#else
unsigned long v = sizeof(int);
#endif
