#include "side.h"
// §4.3 BuiltinType `Float64x`: against `long double`.
#ifdef B
long double *v;
#else
_Float64x *v;
#endif
