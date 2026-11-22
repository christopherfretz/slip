#include "side.h"
// §4.3 BuiltinType `Float32x`: against `_Float64`.
#ifdef B
_Float64 *v;
#else
_Float32x *v;
#endif
