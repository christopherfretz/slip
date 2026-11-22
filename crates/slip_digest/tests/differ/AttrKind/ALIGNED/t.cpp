#include "side.h"
// §10.1 AttrKind: `ALIGNED`: the attribute on one side only.
#ifdef B
alignas(8) int v;
#else
int v;
#endif
