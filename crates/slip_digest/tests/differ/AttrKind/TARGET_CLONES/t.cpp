#include "side.h"
// §10.1 AttrKind: `TARGET_CLONES`: the attribute on one side only.
#ifdef B
__attribute__((target_clones("avx2", "default"))) void f() {};
#else
void f() {};
#endif
