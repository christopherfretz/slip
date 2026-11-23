// std: c++26
#include "side.h"
// §7 ExprNode `CXXExpansionSelectExpr.aux`: the form: dependent against enumerating.
#ifdef B
struct P { int a, b; };
template<class T> void ex(T t) { template for (auto x : t) { (void)x; } }

#else
struct P { int a, b; };
template<class T> void ex(T t) { template for (auto x : {1, 2}) { (void)x; } }

#endif
