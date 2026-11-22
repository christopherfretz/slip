#include "side.h"
// §10.1 AttrKind: `CONSTRUCTOR`: the attribute on one side only.
#ifdef B
__attribute__((constructor)) void f();
#else
void f();
#endif
