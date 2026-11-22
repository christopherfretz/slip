#include "side.h"
// §7 ExprNode: `ShuffleVectorExpr.aux-mask-known`: the fact differs.
typedef int v4 __attribute__((ext_vector_type(4)));
typedef float f4 __attribute__((ext_vector_type(4)));
v4 vv, vv2; v4 *pv;
#ifdef B
template <int N> struct A { v4 w = __builtin_shufflevector(vv, vv, N, 1, 2, 3); };
#else
template <int N> struct A { v4 w = __builtin_shufflevector(vv, vv, 0, 1, 2, 3); };
#endif
