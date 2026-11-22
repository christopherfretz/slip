#include "side.h"
// §7.1 ExprDeps `0-value`.
#ifdef B
template<int N> int v = 1;
#else
template<int N> int v = N;
#endif
