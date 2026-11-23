#include "side.h"
// §7 ExprKind `LambdaExpr`: the kind on side B only.
#ifdef B
int f() { return [] { return 1; }(); }
#else
int f() { return 1; }
#endif
