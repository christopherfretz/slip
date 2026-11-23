#include "side.h"
// §7 ExprNode `LambdaExpr.a`: the closure type.
#ifdef B
template<class T> auto lv = [] { return 2; };
#else
template<class T> auto lv = [] { return 1; };
#endif
