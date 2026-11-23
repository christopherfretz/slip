#include "side.h"
// §7 ExprNode `UnresolvedLookupExpr.a`: nothing found at the definition: the name alone.
#ifdef B
template<class T> int v = g(T());
#else
template<class T> int v = f(T());
#endif
