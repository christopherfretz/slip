#include "side.h"
// §10.1 AttrNode: `NO_SANITIZE.b-coverage`: the fact differs.
#ifdef B
__attribute__((no_sanitize("address", "coverage"))) void f();
#else
__attribute__((no_sanitize("address"))) void f();
#endif
