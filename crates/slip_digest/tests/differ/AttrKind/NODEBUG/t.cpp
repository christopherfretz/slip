#include "side.h"
// §10.1 AttrKind: `NODEBUG`: the attribute on one side only.
#ifdef B
__attribute__((nodebug)) void f();
#else
void f();
#endif
