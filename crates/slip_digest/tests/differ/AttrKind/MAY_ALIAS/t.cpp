#include "side.h"
// §10.1 AttrKind: `MAY_ALIAS`: the attribute on one side only.
#ifdef B
struct __attribute__((may_alias)) S {};
#else
struct S {};
#endif
