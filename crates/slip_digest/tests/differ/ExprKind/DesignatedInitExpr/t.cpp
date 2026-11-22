#include "side.h"
// WP-035, digest-kinds §7 `ExprKind` `DesignatedInitExpr`: a dependent list keeps its designated initialisers.
struct P { int x, y; };
#ifdef B
template <class T> struct A { static inline T w = {1}; };
#else
template <class T> struct A { static inline T w = {.x = 1}; };
#endif
