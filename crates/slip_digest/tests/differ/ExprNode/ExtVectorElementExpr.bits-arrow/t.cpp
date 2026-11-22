#include "side.h"
// §7 ExprNode: `ExtVectorElementExpr.bits-arrow`: the fact differs.
typedef int v4 __attribute__((ext_vector_type(4)));
typedef float f4 __attribute__((ext_vector_type(4)));
v4 vv, vv2; v4 *pv;
#ifdef B
int v = (*pv).x;
#else
int v = pv->x;
#endif
