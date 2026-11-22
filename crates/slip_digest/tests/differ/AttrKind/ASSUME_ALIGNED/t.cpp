#include "side.h"
// §10.1 AttrKind: `ASSUME_ALIGNED`: the attribute on one side only.
#ifdef B
__attribute__((assume_aligned(16))) void *f();
#else
void *f();
#endif
