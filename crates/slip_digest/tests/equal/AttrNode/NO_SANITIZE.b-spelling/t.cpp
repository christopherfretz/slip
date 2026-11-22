#include "side.h"
// §10.1 AttrNode: `NO_SANITIZE.b-spelling`: only the spelling differs.
#ifdef B
__attribute__((no_sanitize_address)) void f();
#else
__attribute__((no_sanitize("address"))) void f();
#endif
