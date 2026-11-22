#include "side.h"
// §7 ExprNode: `TypeTraitExpr.b`: the fact differs.
#ifdef B
struct E { int n; };
bool v = __is_empty(E);
#else
struct E {};
bool v = __is_empty(E);
#endif
