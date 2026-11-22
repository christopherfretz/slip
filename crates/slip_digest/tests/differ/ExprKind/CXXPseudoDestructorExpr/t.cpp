#include "side.h"
// WP-035, digest-kinds §7 `ExprKind` `CXXPseudoDestructorExpr`.
using I = int;
using CI = const int;
int *pi, *pj;
#ifdef B
int v = (pi, 0);
#else
int v = (pi->~I(), 0);
#endif
