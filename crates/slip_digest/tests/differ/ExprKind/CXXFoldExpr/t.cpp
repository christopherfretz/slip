#include "side.h"
// §7 ExprKind `CXXFoldExpr`.
int f(...);
#ifdef B
template<int... Ns> int v = f(Ns...);
#else
template<int... Ns> int v = (Ns + ...);
#endif
