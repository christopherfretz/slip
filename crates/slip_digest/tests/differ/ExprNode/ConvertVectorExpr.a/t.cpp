#include "side.h"
// §7 ExprNode: `ConvertVectorExpr.a`: the fact differs.
typedef int v4 __attribute__((ext_vector_type(4)));
typedef float f4 __attribute__((ext_vector_type(4)));
v4 vv, vv2; v4 *pv;
#ifdef B
f4 v = __builtin_convertvector(vv2, f4);
#else
f4 v = __builtin_convertvector(vv, f4);
#endif
