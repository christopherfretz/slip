#include "side.h"
// §7 ExprKind: `ChooseExpr`: the kind on side A only.
int a = 1, b = 2, c = 3; long l;
#ifdef B
int v = 1;
#else
int v = __builtin_choose_expr(1, 1, 2);
#endif
