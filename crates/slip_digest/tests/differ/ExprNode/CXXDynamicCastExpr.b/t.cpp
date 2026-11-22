#include "side.h"
// §7 ExprNode `CXXDynamicCastExpr.b`.
struct P { virtual ~P(); };
struct Q : P {};
P *pp; Q *qp;
struct Bs { virtual ~Bs(); };
struct Mid : Bs {};
struct Dd : Mid {};
struct Ee : Bs {};
Dd dd; Ee ee;
#ifdef B
Bs &v = dynamic_cast<Bs&>(ee);
#else
Bs &v = dynamic_cast<Bs&>(dd);
#endif
