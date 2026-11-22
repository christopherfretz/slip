#include "side.h"
// WP-035, digest-kinds §7 `ColdExprData` `designators`: written by `expr_init::designators`; kept only in a template's dependent list.
struct P { int x, y; };
#ifdef B
template <class T> struct A { static inline T w = {.y = 1}; };
#else
template <class T> struct A { static inline T w = {.x = 1}; };
#endif
