// std: c++26
#include "side.h"
// §7 ExprNode `PackIndexingExpr.a`: the pack.
#ifdef B
template<int... Ns> struct S { template<int... Ms> static constexpr int v = Ms...[0]; };

#else
template<int... Ns> struct S { template<int... Ms> static constexpr int v = Ns...[0]; };

#endif
