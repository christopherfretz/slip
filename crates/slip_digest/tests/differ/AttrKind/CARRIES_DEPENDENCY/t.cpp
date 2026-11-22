#include "side.h"
// §10.1 AttrKind: `CARRIES_DEPENDENCY`: the attribute on one side only.
#ifdef B
[[carries_dependency]] void f();
#else
void f();
#endif
