#include "side.h"
// §10.1 AttrKind: `NODUPLICATE`: the attribute on one side only.
#ifdef B
__attribute__((noduplicate)) void f();
#else
void f();
#endif
