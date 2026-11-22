#include "side.h"
// §7 ExprNode: `ExpressionTraitExpr.b`: the result differs (and with it the operand).
int a = 1, b = 2, c = 3; long l;
#ifdef B
bool v = __is_lvalue_expr(a + 0);
#else
bool v = __is_lvalue_expr(a);
#endif
