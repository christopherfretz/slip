#include "side.h"
// §10.1 AttrKind: `ZERO_CALL_USED_REGS`: the attribute on one side only.
#ifdef B
__attribute__((zero_call_used_regs("used"))) void f();
#else
void f();
#endif
