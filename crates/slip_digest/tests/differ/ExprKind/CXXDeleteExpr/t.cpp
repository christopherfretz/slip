#include "side.h"
// WP-035, digest-kinds §7 `ExprKind` `CXXDeleteExpr`.
int *p;
#ifdef B
int v = (p, 0);
#else
int v = (delete p, 0);
#endif
