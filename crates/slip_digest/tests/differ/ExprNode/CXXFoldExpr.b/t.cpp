#include "side.h"
// §7 ExprNode `CXXFoldExpr.b`: the right operand present (an init) or absent.
#ifdef B
template<int... Ns> int v = (Ns + ... + 0);
#else
template<int... Ns> int v = (Ns + ...);
#endif
