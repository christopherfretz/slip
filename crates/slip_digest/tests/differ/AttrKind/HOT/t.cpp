#include "side.h"
// §10.1 AttrKind: `HOT`: the attribute on one side only.
#ifdef B
__attribute__((hot)) void f();
#else
void f();
#endif
