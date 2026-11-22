#include "side.h"
// §7.4 ColdExprData `cast_written-unresolved-construct`.
template<class T> struct A { using CT = const T;
#ifdef B
static inline int w = T(1); };
#else
static inline int w = CT(1); };
#endif
