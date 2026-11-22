#include "side.h"
// §10.1 AttrKind: `STANDALONE_DEBUG`: the attribute on one side only.
#ifdef B
struct __attribute__((standalone_debug)) S {};
#else
struct S {};
#endif
