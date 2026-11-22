#include "side.h"
// §7 ExprKind `CXXThisExpr`.
struct Th { int m; int m2;
#ifdef B
int n = 0; };
#else
int n = this->m; };
#endif
