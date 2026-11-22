#include "side.h"
// §7 ExprNode `CXXDynamicCastExpr.a`.
struct P { virtual ~P(); };
struct Q : P {};
P *pp; Q *qp;
P *pp2;
#ifdef B
Q *v = dynamic_cast<Q*>(pp2);
#else
Q *v = dynamic_cast<Q*>(pp);
#endif
