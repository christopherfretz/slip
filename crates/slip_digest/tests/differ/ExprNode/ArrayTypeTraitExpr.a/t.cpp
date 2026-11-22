#include "side.h"
// §7 ExprNode: `ArrayTypeTraitExpr.a`: the fact differs.
#ifdef B
unsigned long v = __array_rank(int[2]);
#else
unsigned long v = __array_rank(int[1]);
#endif
