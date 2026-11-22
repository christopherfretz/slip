#include "side.h"
// §7 ExprKind: `PredefinedExpr`: the kind on side A only.
int a = 1, b = 2, c = 3; long l;
#ifdef B
const char *f(const char *s = "");
#else
const char *f(const char *s = __func__);
#endif
