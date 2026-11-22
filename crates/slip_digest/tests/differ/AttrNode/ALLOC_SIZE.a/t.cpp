#include "side.h"
// §10.1 AttrNode: `ALLOC_SIZE.a`: the fact differs.
#ifdef B
void *f(int, int) __attribute__((alloc_size(2)));
#else
void *f(int, int) __attribute__((alloc_size(1)));
#endif
