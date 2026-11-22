#include "side.h"
// §10.1 AttrKind: `INTERNAL_LINKAGE`: the attribute on one side only.
#ifdef B
__attribute__((internal_linkage)) void f();
#else
void f();
#endif
