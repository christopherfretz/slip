#include "side.h"
// §10.1 AttrKind: `MAYBE_UNDEF`: the attribute on one side only.
#ifdef B
void f(int x __attribute__((maybe_undef)));
#else
void f(int x);
#endif
