#include "side.h"
// §7 ExprNode `ImplicitCastExpr.b`.
struct Bs { int bm; };
struct Mid : Bs {};
struct Dd : Mid {};
struct Ee : Bs {};
Dd dd; Ee ee; Bs bs;
using BR = Bs&;
#ifdef B
Bs &v = ee;
#else
Bs &v = dd;
#endif
