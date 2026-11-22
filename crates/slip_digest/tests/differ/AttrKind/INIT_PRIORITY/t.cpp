#include "side.h"
// §10.1 AttrKind: `INIT_PRIORITY`: the attribute on one side only.
struct S { S(); };
#ifdef B
S s __attribute__((init_priority(200)));
#else
S s;
#endif
