#include "side.h"
// §10.1 AttrKind: `FLATTEN`: the attribute on one side only.
#ifdef B
__attribute__((flatten)) void f();
#else
void f();
#endif
