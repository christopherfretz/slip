#include "side.h"
// §10.1 AttrKind: `OVERLOADABLE`: the attribute on one side only.
#ifdef B
__attribute__((overloadable)) void f();
#else
void f();
#endif
