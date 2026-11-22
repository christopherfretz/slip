#include "side.h"
// §10.1 AttrKind: `NOT_TAIL_CALLED`: the attribute on one side only.
#ifdef B
__attribute__((not_tail_called)) void f();
#else
void f();
#endif
