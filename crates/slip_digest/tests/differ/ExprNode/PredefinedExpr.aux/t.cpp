#include "side.h"
// §7 ExprNode: `PredefinedExpr.aux`: the fact differs.
#ifdef B
const char *f(const char *s = __FUNCTION__);
#else
const char *f(const char *s = __func__);
#endif
