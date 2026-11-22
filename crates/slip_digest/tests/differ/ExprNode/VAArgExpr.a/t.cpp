#include "side.h"
// §7 ExprNode: `VAArgExpr.a`: the fact differs.
__builtin_va_list ap, aq;
#ifdef B
int v = __builtin_va_arg(aq, int);
#else
int v = __builtin_va_arg(ap, int);
#endif
