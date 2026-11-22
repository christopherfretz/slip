#include "side.h"
// §10.1 AttrKind: `ALLOC_SIZE`: the attribute on one side only.
#ifdef B
__attribute__((alloc_size(1))) void *f(int);
#else
void *f(int);
#endif
