#include "side.h"
// §10.1 AttrKind: `UNUSED`: the attribute on one side only.
#ifdef B
__attribute__((unused)) void f();
#else
void f();
#endif
