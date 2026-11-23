#include "side.h"
// §7 ExprNode `UnresolvedLookupExpr.b`, item 1d: only the signature of the overload lookup found changes; the variable template's initialiser is its own, walked in full in its unit, which writes the found set.
#ifdef B
int f(long);
#else
int f(int);
#endif
template<class T> int v = f(T());
