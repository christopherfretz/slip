#include "side.h"
// §10.1 AttrKind: `NO_DESTROY`: the attribute on one side only.
struct S { ~S(); };
#ifdef B
[[clang::no_destroy]] S s;
#else
S s;
#endif
