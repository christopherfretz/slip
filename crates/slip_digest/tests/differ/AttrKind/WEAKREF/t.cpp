#include "side.h"
// §10.1 AttrKind: `WEAKREF`: the attribute on one side only.
#ifdef B
static void g() __attribute__((weakref("h")));
#else
static void g();
#endif
