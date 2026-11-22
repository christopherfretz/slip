#include "side.h"
// WP-035, digest-kinds §7 `ExprNode` `CXXPseudoDestructorExpr.a`.
using I = int;
using CI = const int;
int *pi, *pj;
#ifdef B
int v = (pj->~I(), 0);
#else
int v = (pi->~I(), 0);
#endif
