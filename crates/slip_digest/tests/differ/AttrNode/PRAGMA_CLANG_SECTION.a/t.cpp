#include "side.h"
// §10.1 AttrNode: `PRAGMA_CLANG_SECTION.a`: the fact differs.
#ifdef B
#pragma clang section bss = "t"
int v;
#else
#pragma clang section bss = "s"
int v;
#endif
