#include "side.h"
// §10.1 AttrKind: `NOCOMMON`: the attribute on one side only.
#ifdef B
int v __attribute__((nocommon));
#else
int v;
#endif
