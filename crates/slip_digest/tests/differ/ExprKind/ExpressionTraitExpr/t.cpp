#include "side.h"
// §7 ExprKind: `ExpressionTraitExpr`: the kind on side A only.
int a = 1, b = 2, c = 3; long l;
#ifdef B
bool v = true;
#else
bool v = __is_lvalue_expr(a);
#endif
