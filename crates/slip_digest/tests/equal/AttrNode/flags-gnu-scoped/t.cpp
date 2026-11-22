#include "side.h"
// §10.1 AttrNode: `flags-gnu-scoped`: the `gnu::` scope is spelling.
#ifdef B
[[deprecated]] void f();
#else
[[gnu::deprecated]] void f();
#endif
