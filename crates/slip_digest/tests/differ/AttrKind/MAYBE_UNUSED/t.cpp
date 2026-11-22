#include "side.h"
// §10.1 AttrKind: `MAYBE_UNUSED`: the attribute on one side only.
#ifdef B
[[maybe_unused]] int v;
#else
int v;
#endif
