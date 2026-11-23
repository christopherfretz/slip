#include "side.h"
// §7 ExprNode `CXXFoldExpr.aux`: the operator.
#ifdef B
template<int... Ns> int v = (Ns * ...);
#else
template<int... Ns> int v = (Ns + ...);
#endif
