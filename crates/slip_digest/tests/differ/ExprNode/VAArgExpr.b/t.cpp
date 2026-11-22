#include "side.h"
// §7 ExprNode: `VAArgExpr.b`: the fact differs.
__builtin_va_list ap;
#ifdef B
int v = __builtin_va_arg(ap, unsigned);
#else
int v = __builtin_va_arg(ap, int);
#endif
