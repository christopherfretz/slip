#include "side.h"
// WP-035, digest-kinds §7 `ExprNode` `CXXDeleteExpr.a`.
int *p, *q;
#ifdef B
int v = (delete q, 0);
#else
int v = (delete p, 0);
#endif
