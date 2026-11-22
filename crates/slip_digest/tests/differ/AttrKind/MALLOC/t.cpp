#include "side.h"
// §10.1 AttrKind: `MALLOC`: the attribute on one side only.
#ifdef B
__attribute__((malloc)) void *f();
#else
void *f();
#endif
