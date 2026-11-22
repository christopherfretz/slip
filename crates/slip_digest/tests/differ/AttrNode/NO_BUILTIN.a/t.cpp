#include "side.h"
// §10.1 AttrNode: `NO_BUILTIN.a`: the fact differs.
#ifdef B
__attribute__((no_builtin("memset"))) void f() {}
#else
__attribute__((no_builtin("memcpy"))) void f() {}
#endif
