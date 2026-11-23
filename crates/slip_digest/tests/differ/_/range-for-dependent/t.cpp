#include "side.h"
// WP-034 (§6): a pattern whose range is type-dependent: no begin, end, cond or inc.
#ifdef B
template <class T> void f(T &t) { for (long e : t) ; }
#else
template <class T> void f(T &t) { for (int e : t) ; }
#endif
