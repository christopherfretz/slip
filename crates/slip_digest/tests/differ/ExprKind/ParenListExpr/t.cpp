#include "side.h"
// WP-035, digest-kinds §7 `ExprKind` `ParenListExpr`: a dependent variable's parenthesised initialiser.
#ifdef B
template <class T> T w{1, 2};
#else
template <class T> T w(1, 2);
#endif
