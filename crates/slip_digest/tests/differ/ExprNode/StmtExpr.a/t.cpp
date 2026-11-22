#include "side.h"
// §7 ExprNode: `StmtExpr.a`: the fact differs.
#ifdef B
int f() { return ({ 2; }); }
#else
int f() { return ({ 1; }); }
#endif
