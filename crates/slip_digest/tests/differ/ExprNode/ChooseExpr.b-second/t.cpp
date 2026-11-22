#include "side.h"
// §7 ExprNode: `ChooseExpr.b-second`: the fact differs.
int a = 1, b = 2, c = 3; long l;
#ifdef B
int v = __builtin_choose_expr(1, a, c);
#else
int v = __builtin_choose_expr(1, a, b);
#endif
