#include "side.h"
// §10.1 AttrKind: `PACKED`: the attribute on one side only.
#ifdef B
struct __attribute__((packed)) S { char c; int i; };
#else
struct S { char c; int i; };
#endif
