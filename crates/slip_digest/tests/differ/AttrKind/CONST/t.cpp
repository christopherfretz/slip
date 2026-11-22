#include "side.h"
// §10.1 AttrKind: `CONST`: the attribute on one side only.
#ifdef B
__attribute__((const)) int f();
#else
int f();
#endif
