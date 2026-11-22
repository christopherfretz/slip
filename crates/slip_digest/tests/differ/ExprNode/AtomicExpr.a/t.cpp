#include "side.h"
// §7 ExprNode: `AtomicExpr.a`: the fact differs.
int a = 1, b = 2, c = 3; long l;
#ifdef B
int v = __atomic_load_n(&b, 0);
#else
int v = __atomic_load_n(&a, 0);
#endif
