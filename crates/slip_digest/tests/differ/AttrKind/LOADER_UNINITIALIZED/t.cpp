#include "side.h"
// §10.1 AttrKind: `LOADER_UNINITIALIZED`: the attribute on one side only.
#ifdef B
[[clang::loader_uninitialized]] int v;
#else
int v;
#endif
