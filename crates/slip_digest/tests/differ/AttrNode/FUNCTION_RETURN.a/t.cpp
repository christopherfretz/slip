#include "side.h"
// §10.1 AttrNode: `FUNCTION_RETURN.a`: the fact differs.
#ifdef B
__attribute__((function_return("thunk-extern"))) void f();
#else
__attribute__((function_return("keep"))) void f();
#endif
