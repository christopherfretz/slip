#include "side.h"
// §7 ExprKind: `AddrLabelExpr`: the kind on side A only.
int a = 1, b = 2, c = 3; long l;
#ifdef B
void f() { l: void *p = 0; }
#else
void f() { l: void *p = &&l; }
#endif
