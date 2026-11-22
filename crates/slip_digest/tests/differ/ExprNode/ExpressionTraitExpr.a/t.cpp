#include "side.h"
// §7 ExprNode: `ExpressionTraitExpr.a`: the fact differs.
int a = 1, b = 2, c = 3; long l;
#ifdef B
bool v = __is_lvalue_expr(b);
#else
bool v = __is_lvalue_expr(a);
#endif
