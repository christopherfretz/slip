#include "side.h"
// spec/digest-kinds.md §4.3 BuiltinType `Float128`.
#ifdef B
int v;
#else
__float128 v;
#endif
