#include "side.h"
// §7 ExprNode `MaterializeTemporaryExpr.a`.
#ifdef B
const int &v = 2;
#else
const int &v = 1;
#endif
