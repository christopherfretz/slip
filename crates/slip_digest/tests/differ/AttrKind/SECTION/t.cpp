#include "side.h"
// §10.1 AttrKind: `SECTION`: the attribute on one side only.
#ifdef B
int v __attribute__((section("s")));
#else
int v;
#endif
