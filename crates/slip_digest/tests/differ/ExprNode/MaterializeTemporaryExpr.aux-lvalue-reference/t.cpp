#include "side.h"
// §7 ExprNode `MaterializeTemporaryExpr.aux-lvalue-reference`.
#ifdef B
int &&v = 1;
#else
const int &v = 1;
#endif
