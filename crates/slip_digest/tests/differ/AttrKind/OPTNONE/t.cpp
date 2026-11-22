#include "side.h"
// §10.1 AttrKind: `OPTNONE`: the attribute on one side only.
#ifdef B
__attribute__((optnone)) void f();
#else
void f();
#endif
