#include "side.h"
// §10.1 AttrKind: `SELECTANY`: the attribute on one side only.
#ifdef B
__attribute__((selectany)) inline int v = 1;
#else
inline int v = 1;
#endif
