#include "side.h"
// §7 ExprKind `LambdaExpr`.
#ifdef B
template<class T> auto lv = 1;
#else
template<class T> auto lv = [] { return 1; };
#endif
