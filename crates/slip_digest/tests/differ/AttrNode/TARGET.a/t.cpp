#include "side.h"
// §10.1 AttrNode: `TARGET.a`: the fact differs.
#ifdef B
__attribute__((target("sse4.2"))) void f();
#else
__attribute__((target("avx"))) void f();
#endif
