#include "side.h"
// §7 ExprKind `PackExpansionExpr`.
int f(...);
#ifdef B
template<class... Ts> int v = f(1);
#else
template<class... Ts> int v = f(Ts()...);
#endif
