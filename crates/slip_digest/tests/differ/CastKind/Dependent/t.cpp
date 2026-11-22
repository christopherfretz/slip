#include "side.h"
// §7.2 CastKind `Dependent`.
template<class T> struct A { static T s; static long i;
#ifdef B
static inline int w = (int)i; };
#else
static inline int w = (int)s; };
#endif
