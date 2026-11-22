#include "side.h"
// §7 ExprNode `DeclRefExpr.bits-immediate-escalating`.
consteval int id(int x) { return x; }
#ifdef B
constexpr int f(int x) { return x; }
#else
constexpr int f(int x) { return id(x); }
#endif
