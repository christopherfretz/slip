#include "side.h"
// §7 ExprKind `FunctionParmPackExpr`.
#ifdef B
template<class... Ts> struct R { template<class... Us> static auto g(Ts... ts) -> decltype(((Us() + Us()) + ...)); };
auto p = &R<int, long>::g<int, int>;
#else
template<class... Ts> struct R { template<class... Us> static auto g(Ts... ts) -> decltype(((ts + Us()) + ...)); };
auto p = &R<int, long>::g<int, int>;
#endif
