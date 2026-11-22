#include "side.h"
// §7 ExprNode: `SourceLocExpr.aux`: the fact differs.
#ifdef B
unsigned v = __builtin_COLUMN();
#else
unsigned v = __builtin_LINE();
#endif
