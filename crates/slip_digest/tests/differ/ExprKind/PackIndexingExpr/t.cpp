// std: c++26
#include "side.h"
// §7 ExprKind `PackIndexingExpr`.
#ifdef B
template<int... Ns> int v = (Ns + ...);
#else
template<int... Ns> int v = Ns...[0];
#endif
