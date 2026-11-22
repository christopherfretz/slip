#include "side.h"
// §10.1 AttrNode: `ASSUME_ALIGNED.a`: the fact differs.
#ifdef B
void *f() __attribute__((assume_aligned(32)));
#else
void *f() __attribute__((assume_aligned(16)));
#endif
