#include "side.h"
// §7 ExprKind: `TypeTraitExpr`: the kind on side A only.
int a = 1, b = 2, c = 3; long l;
#ifdef B
bool v = false;
#else
bool v = __is_class(int);
#endif
