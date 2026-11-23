#include "side.h"
// §7 ExprNode `RequiresExpr.b`: the requirements.
#ifdef B
template<class T> bool rv = requires(T t) { t + 2; };

#else
template<class T> bool rv = requires(T t) { t + 1; };

#endif
