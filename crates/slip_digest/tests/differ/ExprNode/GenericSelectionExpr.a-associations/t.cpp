#include "side.h"
// §7 ExprNode: `GenericSelectionExpr.a-associations`: the fact differs.
int a = 1, b = 2, c = 3; long l;
#ifdef B
int v = _Generic(a, int: 1, short: 2);
#else
int v = _Generic(a, int: 1, long: 2);
#endif
