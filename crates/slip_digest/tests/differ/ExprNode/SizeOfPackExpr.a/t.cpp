#include "side.h"
// §7 ExprNode `SizeOfPackExpr.a`: `sizeof...(Ts)` against `sizeof...(Us)`.
#ifdef B
template<class... Ts> struct S { template<class... Us> static constexpr int v = sizeof...(Us); };

#else
template<class... Ts> struct S { template<class... Us> static constexpr int v = sizeof...(Ts); };

#endif
