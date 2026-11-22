#include "side.h"
// §10.1 AttrKind: `COLD`: the attribute on one side only.
#ifdef B
__attribute__((cold)) void f();
#else
void f();
#endif
