#include "side.h"
// §7 ExprKind `RequiresExpr`.
#ifdef B
template<class T> bool rv = true;
#else
template<class T> bool rv = requires(T t) { t + 1; };

#endif
