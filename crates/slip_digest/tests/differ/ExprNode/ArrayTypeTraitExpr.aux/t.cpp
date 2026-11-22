#include "side.h"
// §7 ExprNode: `ArrayTypeTraitExpr.aux`: the fact differs.
#ifdef B
unsigned long v = __array_extent(int[1][2], 0);
#else
unsigned long v = __array_rank(int[1][2]);
#endif
