#include "side.h"
// §7 ExprNode: `SourceLocExpr.a`: the context follows from where the use is (moved a line).
#ifdef B

const char *v = __builtin_FUNCTION();
#else
const char *v = __builtin_FUNCTION();
#endif
