#include "side.h"
// §10.1 AttrNode: `ASSUME_ALIGNED.b`: the fact differs.
#ifdef B
void *f() __attribute__((assume_aligned(16, 4)));
#else
void *f() __attribute__((assume_aligned(16)));
#endif
