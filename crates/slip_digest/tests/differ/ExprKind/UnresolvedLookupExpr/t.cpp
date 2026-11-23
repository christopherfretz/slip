#include "side.h"
// §7 ExprKind `UnresolvedLookupExpr`.
int f(int); int f(long);
int h(int);
#ifdef B
template<class T> int v = f(T());
#else
template<class T> int v = h(T()) + T::x;
#endif
