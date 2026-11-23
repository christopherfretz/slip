#include "side.h"
// §7 ExprNode `UnresolvedLookupExpr.b`: an overload added to the set found.
int f(int);
#ifdef B
int f(long);
template<class T> int v = f(T());
#else
template<class T> int v = f(T());
#endif
