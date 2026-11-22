#include "side.h"
// §7 ExprKind: `ConvertVectorExpr`: the kind on side A only.
typedef int v4 __attribute__((ext_vector_type(4)));
typedef float f4 __attribute__((ext_vector_type(4)));
v4 vv, vv2; v4 *pv;
#ifdef B
v4 v = vv;
#else
v4 v = __builtin_convertvector(vv, v4);
#endif
