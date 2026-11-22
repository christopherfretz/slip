#include "side.h"
// §7 ExprNode: `ShuffleVectorExpr.a`: the fact differs.
typedef int v4 __attribute__((ext_vector_type(4)));
typedef float f4 __attribute__((ext_vector_type(4)));
v4 vv, vv2; v4 *pv;
#ifdef B
v4 v = __builtin_shufflevector(vv, vv2, 0, 1, 2, 3);
#else
v4 v = __builtin_shufflevector(vv, vv, 0, 1, 2, 3);
#endif
