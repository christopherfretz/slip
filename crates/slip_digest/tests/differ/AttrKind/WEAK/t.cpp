#include "side.h"
// §10.1 AttrKind: `WEAK`: the attribute on one side only.
#ifdef B
__attribute__((weak)) void f();
#else
void f();
#endif
