#include "side.h"
// §7 ExprNode `CXXDynamicCastExpr.aux`.
struct P { virtual ~P(); };
struct Q : P {};
P *pp; Q *qp;
#ifdef B
P *v = dynamic_cast<P*>(qp);
#else
P *v = dynamic_cast<Q*>(pp);
#endif
