#include "side.h"
// §7 ExprNode `CXXFunctionalCastExpr.b`.
struct Bs { int bm; };
struct Mid : Bs {};
struct Dd : Mid {};
struct Ee : Bs {};
Dd dd; Ee ee; Bs bs;
using BR = Bs&;
int a, b; long l; double d;
#ifdef B
Bs &v = BR(ee);
#else
Bs &v = BR(dd);
#endif
