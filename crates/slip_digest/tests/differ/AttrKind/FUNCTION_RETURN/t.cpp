#include "side.h"
// §10.1 AttrKind: `FUNCTION_RETURN`: the attribute on one side only.
#ifdef B
__attribute__((function_return("thunk-extern"))) void f();
#else
void f();
#endif
