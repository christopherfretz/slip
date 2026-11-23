#include "side.h"
// §7 ExprNode `UnresolvedLookupExpr.aux`: the same set, `(f)` suppresses argument-dependent lookup.
int f(int); int f(long);
#ifdef B
template<class T> int v = (f)(T());
#else
template<class T> int v = f(T());
#endif
