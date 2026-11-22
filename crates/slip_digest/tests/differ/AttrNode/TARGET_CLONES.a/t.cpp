#include "side.h"
// §10.1 AttrNode: `TARGET_CLONES.a`: the fact differs.
#ifdef B
__attribute__((target_clones("sse4.2", "default"))) void f() {}
#else
__attribute__((target_clones("avx2", "default"))) void f() {}
#endif
