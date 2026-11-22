#include "side.h"
// §10.1 AttrNode: `ZERO_CALL_USED_REGS.a`: the fact differs.
#ifdef B
__attribute__((zero_call_used_regs("all"))) void f();
#else
__attribute__((zero_call_used_regs("used"))) void f();
#endif
