#include "side.h"
// §10.1 AttrKind: `ALLOC_ALIGN`: the attribute on one side only.
#ifdef B
__attribute__((alloc_align(1))) void *f(int);
#else
void *f(int);
#endif
