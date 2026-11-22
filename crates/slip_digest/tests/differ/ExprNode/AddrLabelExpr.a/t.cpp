#include "side.h"
// §7 ExprNode: `AddrLabelExpr.a`: the fact differs.
#ifdef B
void f() { l: m: void *p = &&m; }
#else
void f() { l: m: void *p = &&l; }
#endif
