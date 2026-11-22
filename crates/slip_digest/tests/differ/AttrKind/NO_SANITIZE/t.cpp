#include "side.h"
// §10.1 AttrKind: `NO_SANITIZE`: the attribute on one side only.
#ifdef B
__attribute__((no_sanitize("address"))) void f();
#else
void f();
#endif
