#include "side.h"
// §10.1 AttrNode: `PRAGMA_CLANG_SECTION.b`: the fact differs.
#ifdef B
#pragma clang section data = "s"
int v;
#else
#pragma clang section bss = "s"
int v;
#endif
