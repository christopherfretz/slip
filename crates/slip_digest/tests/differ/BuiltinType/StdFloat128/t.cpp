#include "side.h"
// §4.3 BuiltinType `StdFloat128`: against `__float128`.
#ifdef B
__float128 *v;
#else
_Float128 *v;
#endif
