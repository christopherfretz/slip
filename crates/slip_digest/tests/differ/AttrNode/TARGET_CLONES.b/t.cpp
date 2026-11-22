#include "side.h"
// §10.1 AttrNode: `TARGET_CLONES.b`: the fact differs.
#ifdef B
__attribute__((target_clones("avx2", "avx2", "default"))) void f() {}
#else
__attribute__((target_clones("avx2", "default"))) void f() {}
#endif
