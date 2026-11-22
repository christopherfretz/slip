#include "side.h"
// §10.1 AttrNode: `CONSTRUCTOR.a-priority`: the fact differs.
#ifdef B
__attribute__((constructor(102))) void f();
#else
__attribute__((constructor(101))) void f();
#endif
