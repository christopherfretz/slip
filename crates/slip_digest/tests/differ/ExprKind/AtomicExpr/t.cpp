#include "side.h"
// §7 ExprKind: `AtomicExpr`: the kind on side A only.
int a = 1, b = 2, c = 3; long l;
#ifdef B
int v = a;
#else
int v = __atomic_load_n(&a, 0);
#endif
