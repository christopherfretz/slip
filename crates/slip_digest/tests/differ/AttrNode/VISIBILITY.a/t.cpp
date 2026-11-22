#include "side.h"
// §10.1 AttrNode: `VISIBILITY.a`: the fact differs.
#ifdef B
__attribute__((visibility("protected"))) void f();
#else
__attribute__((visibility("hidden"))) void f();
#endif
