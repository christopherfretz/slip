#include "side.h"
// §7 ExprKind `CXXDynamicCastExpr`.
struct P { virtual ~P(); };
struct Q : P {};
P *pp; Q *qp;
#ifdef B
Q *v = static_cast<Q*>(pp);
#else
Q *v = dynamic_cast<Q*>(pp);
#endif
