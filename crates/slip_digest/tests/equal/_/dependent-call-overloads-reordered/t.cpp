#include "side.h"
// §7 ExprNode `UnresolvedLookupExpr.b`: the declarations found are a set; their order of declaration is not written.
#ifdef B
int f(long); int f(int); int f(char);
#else
int f(char); int f(int); int f(long);
#endif
template<class T> int v = f(T());
