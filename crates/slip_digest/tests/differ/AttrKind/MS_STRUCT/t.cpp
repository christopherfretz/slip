#include "side.h"
// §10.1 AttrKind: `MS_STRUCT`: the attribute on one side only.
#ifdef B
struct __attribute__((ms_struct)) S {};
#else
struct S {};
#endif
