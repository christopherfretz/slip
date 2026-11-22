#include "side.h"
// §10.1 AttrKind: `ALWAYS_DESTROY`: the attribute on one side only.
struct S { ~S(); };
#ifdef B
[[clang::always_destroy]] S s;
#else
S s;
#endif
