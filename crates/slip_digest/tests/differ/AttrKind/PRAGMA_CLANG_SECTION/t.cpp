#include "side.h"
// §10.1 AttrKind: `PRAGMA_CLANG_SECTION`: the attribute on one side only.
#ifdef B
#pragma clang section bss = "s"
int v;
#else
int v;
#endif
