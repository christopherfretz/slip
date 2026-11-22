#include "side.h"
// WP-035, digest-kinds §7 `ExprNode` `CXXDeleteExpr.bits-array-form`.
int *p;
#ifdef B
int v = (delete p, 0);
#else
int v = (delete[] p, 0);
#endif
