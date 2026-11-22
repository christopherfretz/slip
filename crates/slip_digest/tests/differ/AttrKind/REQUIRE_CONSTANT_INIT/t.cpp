#include "side.h"
// §10.1 AttrKind: `REQUIRE_CONSTANT_INIT`: the attribute on one side only.
#ifdef B
[[clang::require_constant_initialization]] int v = 1;
#else
int v = 1;
#endif
