#include "side.h"
// §7 ExprNode: `CXXNoexceptExpr.b`: the fact differs.
#ifdef B
struct S { S() noexcept; };
bool v = noexcept(S());
#else
struct S { S(); };
bool v = noexcept(S());
#endif
