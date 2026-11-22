#include "side.h"
// §10.1 AttrKind: `RETURNS_NONNULL`: the attribute on one side only.
#ifdef B
__attribute__((returns_nonnull)) int *f();
#else
int *f();
#endif
