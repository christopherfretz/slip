#include "side.h"
// §7 ExprNode `CStyleCastExpr.b`.
struct Bs { int bm; };
struct Mid : Bs {};
struct Dd : Mid {};
struct Ee : Bs {};
Dd dd; Ee ee; Bs bs;
using BR = Bs&;
int a, b; long l; double d;
#ifdef B
Bs &v = (Bs&)ee;
#else
Bs &v = (Bs&)dd;
#endif
