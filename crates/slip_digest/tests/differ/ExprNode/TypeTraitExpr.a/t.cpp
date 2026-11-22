#include "side.h"
// §7 ExprNode: `TypeTraitExpr.a`: the fact differs.
#ifdef B
template <class T> struct A { bool w = __is_same(T, long); };
#else
template <class T> struct A { bool w = __is_same(T, int); };
#endif
