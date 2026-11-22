#include "side.h"
// §10.1 AttrNode: `ALLOC_ALIGN.a`: the fact differs.
#ifdef B
void *f(int, int) __attribute__((alloc_align(2)));
#else
void *f(int, int) __attribute__((alloc_align(1)));
#endif
