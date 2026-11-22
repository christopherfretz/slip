#include "side.h"
// §10.1 AttrNode: `PATCHABLE_FUNCTION_ENTRY.a`: the fact differs.
#ifdef B
__attribute__((patchable_function_entry(2, 0))) void f();
#else
__attribute__((patchable_function_entry(1, 0))) void f();
#endif
