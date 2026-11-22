#include "side.h"
// §7 ExprKind: `ArrayTypeTraitExpr`: the kind on side A only.
int a = 1, b = 2, c = 3; long l;
#ifdef B
unsigned long v = 1ul;
#else
unsigned long v = __array_rank(int[1]);
#endif
