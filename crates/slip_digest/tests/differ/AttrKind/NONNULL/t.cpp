#include "side.h"
// §10.1 AttrKind: `NONNULL`: the attribute on one side only.
#ifdef B
__attribute__((nonnull)) void f(int *);
#else
void f(int *);
#endif
