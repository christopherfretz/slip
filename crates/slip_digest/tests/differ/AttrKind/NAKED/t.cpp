#include "side.h"
// §10.1 AttrKind: `NAKED`: the attribute on one side only.
#ifdef B
__attribute__((naked)) void f();
#else
void f();
#endif
