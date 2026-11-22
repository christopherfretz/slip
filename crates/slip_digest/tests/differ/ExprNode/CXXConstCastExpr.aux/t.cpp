#include "side.h"
// §7 ExprNode `CXXConstCastExpr.aux`.
template<class T> struct A { static const int *ci; static const T *ct;
#ifdef B
static inline const void *w = const_cast<T*>(ct); };
#else
static inline const void *w = const_cast<int*>(ci); };
#endif
