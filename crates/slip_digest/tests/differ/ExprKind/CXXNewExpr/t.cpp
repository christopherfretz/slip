#include "side.h"
// WP-035, digest-kinds §7 `ExprKind` `CXXNewExpr`.
#ifdef B
int *v = nullptr;
#else
int *v = new int;
#endif
