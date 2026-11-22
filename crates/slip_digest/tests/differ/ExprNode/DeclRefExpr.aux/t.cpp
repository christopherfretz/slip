#include "side.h"
// §7 ExprNode `DeclRefExpr.aux`.
const int c = 1;
#ifdef B
const int &v = c;
#else
int v = c;
#endif
