#include "side.h"
// §7.1 ExprDeps `0-instantiation`.
int g;
template<int N> struct A { static int i;
#ifdef B
static inline int w = g; };
#else
// a member of a dependent context: instantiation-dependent only
static inline int w = i; };
#endif
