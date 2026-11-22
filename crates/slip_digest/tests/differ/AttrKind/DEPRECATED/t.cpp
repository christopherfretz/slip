#include "side.h"
// §10.1 AttrKind: `DEPRECATED`: the attribute on one side only.
#ifdef B
[[deprecated]] void f();
#else
void f();
#endif
