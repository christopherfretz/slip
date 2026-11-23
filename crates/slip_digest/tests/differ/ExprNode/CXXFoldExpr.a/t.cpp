#include "side.h"
// §7 ExprNode `CXXFoldExpr.a`: the left operand present (an init) or absent.
#ifdef B
template<int... Ns> int v = (0 + ... + Ns);
#else
template<int... Ns> int v = (... + Ns);
#endif
