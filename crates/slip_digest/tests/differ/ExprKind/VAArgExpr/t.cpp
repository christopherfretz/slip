#include "side.h"
// §7 ExprKind: `VAArgExpr`: the kind on side A only.
__builtin_va_list ap;
#ifdef B
int v = 1;
#else
int v = __builtin_va_arg(ap, int);
#endif
