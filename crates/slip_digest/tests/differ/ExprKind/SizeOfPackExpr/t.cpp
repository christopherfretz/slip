#include "side.h"
// §7 ExprKind `SizeOfPackExpr`.
#ifdef B
template<class... Ts> constexpr int v = 1;
#else
template<class... Ts> constexpr int v = sizeof...(Ts);
#endif
