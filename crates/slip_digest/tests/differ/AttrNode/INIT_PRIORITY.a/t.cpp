#include "side.h"
// §10.1 AttrNode: `INIT_PRIORITY.a`: the fact differs.
struct S { S(); };
#ifdef B
S s __attribute__((init_priority(300)));
#else
S s __attribute__((init_priority(200)));
#endif
