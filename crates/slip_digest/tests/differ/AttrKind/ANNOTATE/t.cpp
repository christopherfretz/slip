#include "side.h"
// §10.1 AttrKind: `ANNOTATE`: the attribute on one side only.
#ifdef B
[[clang::annotate("x")]] void f();
#else
void f();
#endif
