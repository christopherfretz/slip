#include "side.h"
// WP-035, digest-kinds §7 `ExprNode` `ParenListExpr.a`.
#ifdef B
template <class T> T w(1, 3);
#else
template <class T> T w(1, 2);
#endif
