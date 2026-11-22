#include "side.h"
// §4.3 BuiltinType `Half`: against `_Float16`.
#ifdef B
_Float16 *v;
#else
__fp16 *v;
#endif
