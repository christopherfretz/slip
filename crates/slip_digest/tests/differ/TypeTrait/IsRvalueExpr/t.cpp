#include "side.h"
// §7 TypeTrait: `IsRvalueExpr`: the fact differs.
#ifdef B
template <class T> struct A { T t; bool w = __is_lvalue_expr(t); };
#else
template <class T> struct A { T t; bool w = __is_rvalue_expr(t); };
#endif
