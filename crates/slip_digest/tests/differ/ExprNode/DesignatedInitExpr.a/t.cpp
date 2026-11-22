#include "side.h"
// WP-035, digest-kinds §7 `ExprNode` `DesignatedInitExpr.a`.
struct P { int x, y; };
#ifdef B
template <class T> struct A { static inline T w = {.x = 2}; };
#else
template <class T> struct A { static inline T w = {.x = 1}; };
#endif
