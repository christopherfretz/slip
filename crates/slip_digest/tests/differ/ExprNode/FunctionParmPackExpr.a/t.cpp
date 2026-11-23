#include "side.h"
// §7 ExprNode `FunctionParmPackExpr.a`: the pattern's parameter pack, by position (a member template of an instantiation).
#ifdef B
template<class... Ts> struct R { template<class... Us> static auto g(int, Ts... ts) -> decltype(((ts + Us()) + ...)); };
auto p = &R<int, long>::g<int, int>;
#else
template<class... Ts> struct R { template<class... Us> static auto g(Ts... ts) -> decltype(((ts + Us()) + ...)); };
auto p = &R<int, long>::g<int, int>;
#endif
