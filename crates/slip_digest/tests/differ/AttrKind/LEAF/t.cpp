#include "side.h"
// §10.1 AttrKind: `LEAF`: the attribute on one side only.
#ifdef B
__attribute__((leaf)) void f();
#else
void f();
#endif
