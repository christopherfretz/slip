#include "side.h"
// §10.1 AttrNode: `ALIGNED.a`: the fact differs.
#ifdef B
struct __attribute__((aligned(4))) S {};
#else
struct __attribute__((aligned)) S {};
#endif
