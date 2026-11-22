#include "side.h"
// §7 ExprNode: `UnaryExprOrTypeTraitExpr.aux`: the fact differs.
#ifdef B
unsigned long v = alignof(int);
#else
unsigned long v = sizeof(int);
#endif
