#include "side.h"
// §7 ExprNode: `PredefinedExpr.a`: the string follows from where the use is (moved a line).
#ifdef B

const char *f(const char *s = __func__);
#else
const char *f(const char *s = __func__);
#endif
