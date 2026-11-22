#include "side.h"
// §10.1 AttrKind: `DISABLE_TAIL_CALLS`: the attribute on one side only.
#ifdef B
__attribute__((disable_tail_calls)) void f();
#else
void f();
#endif
