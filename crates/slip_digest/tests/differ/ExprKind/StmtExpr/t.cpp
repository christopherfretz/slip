#include "side.h"
// §7 ExprKind: `StmtExpr`: the kind on side A only.
int a = 1, b = 2, c = 3; long l;
#ifdef B
int v = 1;
#else
int v = ({ 1; });
#endif
