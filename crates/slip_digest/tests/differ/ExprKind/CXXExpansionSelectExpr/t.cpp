// std: c++26
#include "side.h"
// §7 ExprKind `CXXExpansionSelectExpr`.
struct P { int a, b; };
#ifdef B
template<class T> void ex(T t) { (void)t; }
#else
template<class T> void ex(T t) { template for (auto x : t) { (void)x; } }
#endif
