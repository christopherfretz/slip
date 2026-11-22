#include "side.h"
// §4.3 BuiltinType `Float64`: against `double`.
#ifdef B
double *v;
#else
_Float64 *v;
#endif
