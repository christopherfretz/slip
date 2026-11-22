#include "side.h"
// §10.1 AttrKind: `CONVERGENT`: the attribute on one side only.
#ifdef B
__attribute__((convergent)) void f();
#else
void f();
#endif
