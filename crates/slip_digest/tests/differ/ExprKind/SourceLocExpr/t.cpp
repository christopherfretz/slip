#include "side.h"
// §7 ExprKind: `SourceLocExpr`: the kind on side A only.
int a = 1, b = 2, c = 3; long l;
#ifdef B
unsigned v = 1u;
#else
unsigned v = __builtin_LINE();
#endif
