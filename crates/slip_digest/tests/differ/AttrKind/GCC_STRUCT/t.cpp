#include "side.h"
// §10.1 AttrKind: `GCC_STRUCT`: the attribute on one side only.
#ifdef B
struct __attribute__((gcc_struct)) S {};
#else
struct S {};
#endif
