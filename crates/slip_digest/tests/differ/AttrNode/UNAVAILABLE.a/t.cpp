#include "side.h"
// §10.1 AttrNode: `UNAVAILABLE.a`: the fact differs.
#ifdef B
__attribute__((unavailable("m"))) void f();
#else
__attribute__((unavailable)) void f();
#endif
