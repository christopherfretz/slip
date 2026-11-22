#include "side.h"
// §10.1 AttrNode: `ALLOC_SIZE.b`: the fact differs.
#ifdef B
void *f(int, int) __attribute__((alloc_size(1, 2)));
#else
void *f(int, int) __attribute__((alloc_size(1)));
#endif
