#include "side.h"
// §7 ExprNode `PackExpansionExpr.a`: the pattern.
int f(...);
#ifdef B
template<class... Ts> int v = f(Ts(0)...);
#else
template<class... Ts> int v = f(Ts()...);
#endif
