#include "side.h"
// §7 ExprNode: `AtomicExpr.aux`: the fact differs.
int a = 1, b = 2, c = 3; long l;
#ifdef B
int v = __atomic_fetch_add(&a, 1, 0);
#else
int v = __atomic_add_fetch(&a, 1, 0);
#endif
