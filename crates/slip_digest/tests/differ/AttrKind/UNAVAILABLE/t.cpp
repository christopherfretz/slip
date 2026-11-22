#include "side.h"
// §10.1 AttrKind: `UNAVAILABLE`: the attribute on one side only.
#ifdef B
__attribute__((unavailable)) void f();
#else
void f();
#endif
