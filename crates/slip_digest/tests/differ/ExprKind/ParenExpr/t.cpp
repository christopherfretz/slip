#include "side.h"
// §7 ExprKind `ParenExpr`.
// written only in a `decltype` profile (expr_dep.rs)
template<class T> struct A { static T t;
#ifdef B
static decltype(t) w; };
#else
static decltype((t)) w; };
#endif
