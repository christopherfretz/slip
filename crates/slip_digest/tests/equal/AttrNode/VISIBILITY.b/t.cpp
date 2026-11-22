#include "side.h"
// §10.1 AttrNode: `VISIBILITY.b`: only the spelling differs.
#ifdef B
__attribute__((visibility("internal"))) void f();
#else
__attribute__((visibility("hidden"))) void f();
#endif
