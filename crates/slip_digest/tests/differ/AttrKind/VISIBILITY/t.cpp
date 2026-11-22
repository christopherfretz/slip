#include "side.h"
// §10.1 AttrKind: `VISIBILITY`: the attribute on one side only.
#ifdef B
__attribute__((visibility("hidden"))) void f();
#else
void f();
#endif
