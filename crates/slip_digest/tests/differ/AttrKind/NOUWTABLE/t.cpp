#include "side.h"
// §10.1 AttrKind: `NOUWTABLE`: the attribute on one side only.
#ifdef B
__attribute__((nouwtable)) void f();
#else
void f();
#endif
