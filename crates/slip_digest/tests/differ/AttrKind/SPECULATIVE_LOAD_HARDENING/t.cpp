#include "side.h"
// §10.1 AttrKind: `SPECULATIVE_LOAD_HARDENING`: the attribute on one side only.
#ifdef B
__attribute__((speculative_load_hardening)) void f();
#else
void f();
#endif
