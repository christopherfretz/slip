#include "side.h"
// §7 ExprKind: `UnaryExprOrTypeTraitExpr`: the kind on side A only.
int a = 1, b = 2, c = 3; long l;
#ifdef B
unsigned long v = 4ul;
#else
unsigned long v = sizeof(int);
#endif
