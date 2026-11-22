#include "side.h"
// §7 ExprNode `ParenExpr.a`.
// written only in a `decltype` profile (expr_dep.rs)
template<class T> struct A { static T t, u;
#ifdef B
static decltype((u)) w; };
#else
static decltype((t)) w; };
#endif
