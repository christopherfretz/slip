#include "side.h"
// §7 ExprNode `CXXUnresolvedConstructExpr.aux`: `T({})` against `T{}`. A braced form holds its braces as one `InitListExpr` argument, so only the bit tells `T({})` from `T{}`.
#ifdef B
template<class T> T v = T{};
#else
template<class T> T v = T({});
#endif
