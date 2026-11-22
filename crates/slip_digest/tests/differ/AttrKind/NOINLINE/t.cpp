#include "side.h"
// §10.1 AttrKind: `NOINLINE`: the attribute on one side only.
#ifdef B
__attribute__((noinline)) void f();
#else
void f();
#endif
