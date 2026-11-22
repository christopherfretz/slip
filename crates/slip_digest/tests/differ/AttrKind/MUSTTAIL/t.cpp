#include "side.h"
// §10.1 AttrKind: `MUSTTAIL`: the attribute on one side only.
#ifdef B
int g(); int f() { [[clang::musttail]] return g(); }
#else
int g(); int f() { return g(); }
#endif
