#include "side.h"
// §7 ExprNode `FunctionParmPackExpr.b`: the expanded parameters.
#ifdef B
template<class... Ts> struct R { template<class... Us> static auto g(Ts... ts) -> decltype(((ts + Us()) + ...)); };
auto p = &R<int, long, short>::g<int, int, int>;
#else
template<class... Ts> struct R { template<class... Us> static auto g(Ts... ts) -> decltype(((ts + Us()) + ...)); };
auto p = &R<int, long>::g<int, int>;
#endif
