#include "side.h"
// §10.1 AttrKind: `NODISCARD`: the attribute on one side only.
#ifdef B
[[nodiscard]] int f();
#else
int f();
#endif
