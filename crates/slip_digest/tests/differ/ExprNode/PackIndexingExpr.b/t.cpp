// std: c++26
#include "side.h"
// §7 ExprNode `PackIndexingExpr.b`: the index.
#ifdef B
template<int... Ns> int v = Ns...[1];
#else
template<int... Ns> int v = Ns...[0];
#endif
