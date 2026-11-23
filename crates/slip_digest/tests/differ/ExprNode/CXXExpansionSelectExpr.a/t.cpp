// std: c++26
#include "side.h"
// §7 ExprNode `CXXExpansionSelectExpr.a`: the depth of the statement's template level: 0 in a function that is not a template, 1 in a template's.
#ifdef B
template<class T> int ex(T t) { int n = 0; template for (auto x : {1, 2}) { n += x; } return n; }
int i = ex(0);
#else
int ex(int t) { int n = 0; template for (auto x : {1, 2}) { n += x; } return n; }
int i = ex(0);
#endif
