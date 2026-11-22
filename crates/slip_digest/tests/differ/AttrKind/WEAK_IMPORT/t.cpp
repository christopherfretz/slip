#include "side.h"
// §10.1 AttrKind: `WEAK_IMPORT`: the attribute on one side only.
#ifdef B
__attribute__((weak_import)) void f();
#else
void f();
#endif
