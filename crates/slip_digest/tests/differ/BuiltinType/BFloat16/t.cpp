#include "side.h"
// spec/digest-kinds.md §4.3 BuiltinType `BFloat16`.
#ifdef B
int v;
#else
__bf16 v;
#endif
