#include "side.h"
// §7.1 ExprDeps `0-type`.
template<class T> struct A { static T s; static int i;
#ifdef B
static inline int w = i; };
#else
static inline int w = s; };
#endif
