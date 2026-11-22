#include "side.h"
// §10.1 AttrKind: `NOMERGE`: the attribute on one side only.
#ifdef B
__attribute__((nomerge)) void f();
#else
void f();
#endif
