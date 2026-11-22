#include "side.h"
// §10.1 AttrKind: `NOTHROW`: the attribute on one side only.
#ifdef B
__attribute__((nothrow)) void f();
#else
void f();
#endif
