#include "side.h"
// §7 ExprNode: `GenericSelectionExpr.b`: the fact differs.
int a = 1, b = 2, c = 3; long l;
#ifdef B
int v = _Generic(l, int: 1, long: 1);
#else
int v = _Generic(a, int: 1, long: 1);
#endif
