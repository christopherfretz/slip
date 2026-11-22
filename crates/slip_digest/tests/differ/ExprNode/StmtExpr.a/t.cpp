#include "side.h"
// §7 ExprNode: `StmtExpr.a`: the fact differs.
#ifdef B
int v = ({ 2; });
#else
int v = ({ 1; });
#endif
