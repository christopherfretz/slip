#include "side.h"
// §10.1 AttrNode: `NONNULL.a`: the fact differs.
#ifdef B
void f(int *, int *) __attribute__((nonnull(1)));
#else
void f(int *, int *) __attribute__((nonnull));
#endif
