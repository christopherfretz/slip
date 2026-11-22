#include "side.h"
// §7 ExprNode `deps`.
#ifdef B
template<int N> int v = 1;
#else
template<int N> int v = N;
#endif
