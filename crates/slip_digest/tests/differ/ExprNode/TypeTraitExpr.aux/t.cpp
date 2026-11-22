#include "side.h"
// §7 ExprNode: `TypeTraitExpr.aux`: the fact differs.
#ifdef B
template <class T> struct A { bool w = __is_union(T); };
#else
template <class T> struct A { bool w = __is_class(T); };
#endif
