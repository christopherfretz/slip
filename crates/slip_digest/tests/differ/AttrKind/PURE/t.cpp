#include "side.h"
// §10.1 AttrKind: `PURE`: the attribute on one side only.
#ifdef B
__attribute__((pure)) int f();
#else
int f();
#endif
