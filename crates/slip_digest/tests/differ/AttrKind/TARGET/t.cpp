#include "side.h"
// §10.1 AttrKind: `TARGET`: the attribute on one side only.
#ifdef B
__attribute__((target("avx"))) void f();
#else
void f();
#endif
