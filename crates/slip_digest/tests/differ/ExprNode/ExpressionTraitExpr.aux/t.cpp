#include "side.h"
// §7 ExprNode: `ExpressionTraitExpr.aux`: the fact differs.
int a = 1, b = 2, c = 3; long l;
#ifdef B
bool v = __is_rvalue_expr(a);
#else
bool v = __is_lvalue_expr(a);
#endif
