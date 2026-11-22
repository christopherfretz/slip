#include "side.h"
// §7 ExprNode: `ShuffleVectorExpr.b`: only the folded index differs at the use.
typedef int v4 __attribute__((ext_vector_type(4)));
typedef float f4 __attribute__((ext_vector_type(4)));
v4 vv, vv2; v4 *pv;
#ifdef B
constexpr int K = 3;
v4 v = __builtin_shufflevector(vv, vv, K, 1, 2, 3);
#else
constexpr int K = 0;
v4 v = __builtin_shufflevector(vv, vv, K, 1, 2, 3);
#endif
