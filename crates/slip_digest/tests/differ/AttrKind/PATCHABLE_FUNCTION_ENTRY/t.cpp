#include "side.h"
// §10.1 AttrKind: `PATCHABLE_FUNCTION_ENTRY`: the attribute on one side only.
#ifdef B
__attribute__((patchable_function_entry(1, 0))) void f();
#else
void f();
#endif
